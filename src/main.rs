use lambda_http::{run, service_fn, Error, Request, Body};
use axum::{
    extract::Request as AxumRequest,
    http::{HeaderMap, StatusCode},
    middleware::{self, Next},
    response::Response,
    Router,
};
use tower::ServiceExt;
use tower_http::cors::CorsLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod mcp_server;
use mcp_server::McpServer;

#[tokio::main]
async fn main() -> Result<(), Error> {
    // Initialize tracing for Lambda
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("🚀 Starting GPT-OSS MCP Server on AWS Lambda");
    tracing::info!("🔧 Lambda Configuration:");
    tracing::info!("  ✅ MCP protocol implementation");
    tracing::info!("  ✅ Axum web framework");
    tracing::info!("  ✅ CORS support");
    tracing::info!("  ✅ ARM64 optimized");
    tracing::info!("🧰 Available tools: search, open, find");

    // Create the Axum router
    let app = McpServer::router()
        .layer(CorsLayer::permissive())
        .layer(middleware::from_fn(mcp_session_middleware));

    // Run the Lambda function
    run(service_fn(move |event: Request| {
        let app = app.clone();
        async move { function_handler(event, app).await }
    }))
    .await
}

async fn function_handler(
    lambda_request: Request,
    app: Router,
) -> Result<lambda_http::Response<Body>, Error> {
    tracing::info!(
        "Processing Lambda request: {} {}",
        lambda_request.method(),
        lambda_request.uri()
    );

    // Convert Lambda HTTP request to Axum request
    let axum_request = convert_lambda_to_axum_request(lambda_request)?;

    // Process the request through Axum
    let axum_response = app
        .oneshot(axum_request)
        .await
        .map_err(|err| Error::from(format!("Axum processing error: {}", err)))?;

    // Convert Axum response to Lambda HTTP response
    convert_axum_to_lambda_response(axum_response).await
}

fn convert_lambda_to_axum_request(lambda_request: Request) -> Result<AxumRequest, Error> {
    let (parts, body) = lambda_request.into_parts();

    // Convert body
    let body_bytes = match body {
        Body::Text(text) => text.into_bytes(),
        Body::Binary(bytes) => bytes,
        Body::Empty => Vec::new(),
    };

    // Convert method
    let method = match parts.method.as_str() {
        "GET" => axum::http::Method::GET,
        "POST" => axum::http::Method::POST,
        "PUT" => axum::http::Method::PUT,
        "DELETE" => axum::http::Method::DELETE,
        "HEAD" => axum::http::Method::HEAD,
        "OPTIONS" => axum::http::Method::OPTIONS,
        "PATCH" => axum::http::Method::PATCH,
        _ => axum::http::Method::GET, // Default fallback
    };

    // Convert URI - extract just the path part for Axum routing
    let uri_str = parts.uri.to_string();
    let path = if uri_str.starts_with("http") {
        // Extract path from full URL
        if let Some(path_start) = uri_str.find("/prod") {
            &uri_str[path_start + 5..] // Remove "/prod" prefix
        } else if let Some(path_start) = uri_str.rfind('/') {
            &uri_str[path_start..]
        } else {
            "/"
        }
    } else {
        // Already just a path, remove /prod prefix if present
        if uri_str.starts_with("/prod") {
            &uri_str[5..]
        } else {
            &uri_str
        }
    };
    
    // Ensure we have a valid path
    let final_path = if path.is_empty() { "/" } else { path };
    
    tracing::info!("Converting URI: {} -> path: {}", uri_str, final_path);
    
    let uri = final_path.parse::<axum::http::Uri>()
        .map_err(|e| Error::from(format!("Failed to parse URI path '{}': {}", final_path, e)))?;

    // Build Axum request
    let mut builder = AxumRequest::builder()
        .method(method)
        .uri(uri);

    // Add headers - convert from lambda_http to axum
    for (name, value) in parts.headers.iter() {
        if let Ok(header_value) = value.to_str() {
            builder = builder.header(name.as_str(), header_value);
        }
    }

    let axum_request = builder
        .body(axum::body::Body::from(body_bytes))
        .map_err(|e| Error::from(format!("Failed to build Axum request: {}", e)))?;

    Ok(axum_request)
}

async fn convert_axum_to_lambda_response(
    axum_response: Response,
) -> Result<lambda_http::Response<Body>, Error> {
    let (parts, body) = axum_response.into_parts();

    // Extract body bytes
    let body_bytes = axum::body::to_bytes(body, usize::MAX)
        .await
        .map_err(|e| Error::from(format!("Failed to read response body: {}", e)))?;

    // Convert status code
    let status_code = parts.status.as_u16();

    // Build Lambda response
    let mut builder = lambda_http::Response::builder().status(status_code);

    // Add headers - convert from axum to lambda_http
    for (name, value) in parts.headers.iter() {
        if let Ok(header_value) = value.to_str() {
            builder = builder.header(name.as_str(), header_value);
        }
    }

    // Determine if body is text or binary
    let lambda_body = if is_text_content(&parts.headers) {
        Body::Text(String::from_utf8_lossy(&body_bytes).to_string())
    } else {
        Body::Binary(body_bytes.to_vec())
    };

    let lambda_response = builder
        .body(lambda_body)
        .map_err(|e| Error::from(format!("Failed to build Lambda response: {}", e)))?;

    Ok(lambda_response)
}

fn is_text_content(headers: &HeaderMap) -> bool {
    if let Some(content_type) = headers.get("content-type") {
        if let Ok(content_type_str) = content_type.to_str() {
            return content_type_str.starts_with("text/")
                || content_type_str.starts_with("application/json")
                || content_type_str.starts_with("application/javascript");
        }
    }
    true // Default to text for MCP JSON-RPC responses
}

/// Middleware to handle MCP session management
async fn mcp_session_middleware(request: AxumRequest, next: Next) -> Result<Response, StatusCode> {
    let headers = request.headers();

    // Log incoming request
    let method = request.method().clone();
    let uri = request.uri().clone();
    tracing::info!("Incoming request: {} {}", method, uri);

    // Check for MCP session header (AgentCore adds this automatically)
    if let Some(session_id) = headers.get("Mcp-Session-Id") {
        tracing::info!("MCP Session ID: {:?}", session_id);
    }

    // Check for authorization header
    if let Some(auth_header) = headers.get("authorization") {
        tracing::info!(
            "Authorization header present: {}",
            auth_header
                .to_str()
                .unwrap_or("invalid")
                .chars()
                .take(20)
                .collect::<String>()
                + "..."
        );
    }

    // Add CORS headers for MCP compliance
    let mut response = next.run(request).await;
    let response_headers = response.headers_mut();

    response_headers.insert("Access-Control-Allow-Origin", "*".parse().unwrap());
    response_headers.insert(
        "Access-Control-Allow-Methods",
        "GET, POST, OPTIONS".parse().unwrap(),
    );
    response_headers.insert(
        "Access-Control-Allow-Headers",
        "Content-Type, Authorization, Mcp-Session-Id".parse().unwrap(),
    );

    Ok(response)
}