use axum::{
    extract::{Request, Path},
    http::StatusCode,
    response::Json as ResponseJson,
    routing::{get, post, delete},
    Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::{info, error, debug, warn};
use std::collections::HashMap;
use scraper::{Html, Selector};
use html2text::from_read;

/// MCP Server implementation for GPT-OSS browser tools
pub struct McpServer;

#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    pub params: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct McpContent {
    #[serde(rename = "type")]
    pub content_type: String,
    pub text: String,
}

#[derive(Debug, Serialize)]
pub struct McpToolResult {
    pub content: Vec<McpContent>,
}

// Tool registry for MCP protocol compliance
lazy_static::lazy_static! {
    static ref TOOLS: HashMap<&'static str, Value> = {
        let mut tools = HashMap::new();
        
        tools.insert("search", json!({
            "name": "search",
            "description": "Search for information on the web and return formatted results with citations",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "Search query"},
                    "topn": {"type": "number", "description": "Number of results to return (default: 10)", "default": 10}
                },
                "required": ["query"]
            }
        }));
        
        tools.insert("open", json!({
            "name": "open",
            "description": "Open a web page by URL and return its content with line numbers for citation",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "url": {"type": "string", "description": "URL to open"},
                    "loc": {"type": "number", "description": "Starting line number (default: 0)", "default": 0},
                    "num_lines": {"type": "number", "description": "Number of lines to show (-1 for all)", "default": -1}
                },
                "required": ["url"]
            }
        }));
        
        tools.insert("find", json!({
            "name": "find",
            "description": "Find specific text patterns in the currently opened page",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "pattern": {"type": "string", "description": "Text pattern to search for"},
                    "url": {"type": "string", "description": "URL of the page to search in (optional if using after open)"}
                },
                "required": ["pattern"]
            }
        }));
        
        tools
    };
}

// Session state management for browser tools
#[derive(Debug, Clone)]
pub struct BrowserSession {
    pub current_url: Option<String>,
    pub current_content: Option<String>,
    pub pages: HashMap<String, String>,
}

impl Default for BrowserSession {
    fn default() -> Self {
        Self {
            current_url: None,
            current_content: None,
            pages: HashMap::new(),
        }
    }
}

// Simple in-memory session storage
lazy_static::lazy_static! {
    static ref SESSIONS: std::sync::RwLock<HashMap<String, BrowserSession>> = 
        std::sync::RwLock::new(HashMap::new());
}

impl McpServer {
    pub fn router() -> Router {
        Router::new()
            .route("/mcp", post(Self::handle_mcp_request))
            .route("/mcp/", post(Self::handle_mcp_request))
            .route("/", post(Self::handle_mcp_request)) // Handle root POST for MCP
            .route("/", get(Self::handle_root))
            .route("/health", get(Self::handle_health))
            .route("/mcp/sessions/{session_id}", delete(Self::handle_session_delete))
            .route("/sessions/{session_id}", delete(Self::handle_session_delete))
            .fallback(Self::handle_fallback) // Catch-all for debugging
    }

    async fn handle_session_delete(Path(session_id): Path<String>) -> Result<ResponseJson<Value>, StatusCode> {
        info!("🔚 HTTP DELETE session termination request for: {}", session_id);
        
        // Validate session ID format (basic validation)
        if session_id.is_empty() || session_id.len() > 100 {
            warn!("Invalid session ID format: {}", session_id);
            return Err(StatusCode::BAD_REQUEST);
        }
        
        // Clean up session data
        if let Ok(mut sessions) = SESSIONS.write() {
            sessions.remove(&session_id);
        }
        
        info!("✅ Session {} terminated successfully", session_id);
        
        Ok(ResponseJson(json!({
            "status": "terminated",
            "sessionId": session_id,
            "timestamp": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            "message": "Session terminated successfully"
        })))
    }

    async fn handle_root() -> Result<ResponseJson<Value>, StatusCode> {
        info!("Root endpoint accessed");
        
        Ok(ResponseJson(json!({
            "message": "GPT-OSS Browser MCP Server",
            "version": "1.0.0",
            "protocol": "MCP 2024-11-05",
            "mcp_endpoint": "/mcp",
            "health_endpoint": "/health",
            "transport": "stateless streamable HTTP",
            "authentication": "OAuth/JWT ready",
            "aws_agentcore_compliant": true,
            "architecture": "ARM64 optimized",
            "tools": ["search", "open", "find"],
            "tools_count": TOOLS.len(),
            "status": "ready"
        })))
    }

    async fn handle_health() -> Result<ResponseJson<Value>, StatusCode> {
        info!("Health check requested");
        
        Ok(ResponseJson(json!({
            "status": "healthy",
            "server": "GPT-OSS Browser MCP",
            "tools_loaded": TOOLS.len(),
            "version": "1.0.0",
            "architecture": "ARM64"
        })))
    }

    async fn handle_mcp_request(request: Request) -> Result<ResponseJson<JsonRpcResponse>, StatusCode> {
        // Log incoming request for debugging
        info!("MCP request received");
        debug!("Request headers: {:?}", request.headers());
        
        // Extract session ID from headers if present (before consuming request)
        let session_id = request.headers()
            .get("Mcp-Session-Id")
            .or_else(|| request.headers().get("mcp-session-id"))
            .and_then(|v| v.to_str().ok())
            .unwrap_or("default")
            .to_string();
        
        debug!("Session ID: {}", session_id);
        
        // Get request body with enhanced error handling
        let body = match axum::body::to_bytes(request.into_body(), usize::MAX).await {
            Ok(body) => body,
            Err(e) => {
                error!("Error reading request body: {}", e);
                return Ok(ResponseJson(Self::create_error_response(
                    None,
                    -32600,
                    "Invalid Request",
                    Some(format!("Failed to read request body: {}", e))
                )));
            }
        };

        if body.is_empty() {
            warn!("Empty request body received for session: {}", session_id);
            return Ok(ResponseJson(Self::create_error_response(
                None,
                -32600,
                "Invalid Request",
                Some("Empty request body".to_string())
            )));
        }

        // Parse JSON with enhanced error handling
        let json_request: JsonRpcRequest = match serde_json::from_slice(&body) {
            Ok(req) => req,
            Err(e) => {
                error!("JSON decode error for session {}: {}", session_id, e);
                return Ok(ResponseJson(Self::create_error_response(
                    None,
                    -32700,
                    "Parse error",
                    Some(format!("Invalid JSON: {}", e))
                )));
            }
        };

        info!("MCP Request: method={}, id={:?}, session={}", 
              json_request.method, json_request.id, session_id);

        // Validate JSON-RPC structure
        if json_request.jsonrpc != "2.0" {
            warn!("Invalid JSON-RPC version: {} for session: {}", json_request.jsonrpc, session_id);
            return Ok(ResponseJson(Self::create_error_response(
                json_request.id,
                -32600,
                "Invalid Request",
                Some("JSON-RPC version must be 2.0".to_string())
            )));
        }

        // Handle different MCP methods
        match json_request.method.as_str() {
            "initialize" => Self::handle_initialize(json_request).await,
            "tools/list" => Self::handle_tools_list(json_request).await,
            "tools/call" => Self::handle_tools_call(json_request, &session_id).await,
            "ping" => Self::handle_ping(json_request).await,
            "session/terminate" => Self::handle_session_terminate(json_request).await,
            "notifications/cancelled" => Self::handle_notification_cancelled(json_request).await,
            _ => {
                warn!("Unknown method: {}", json_request.method);
                Ok(ResponseJson(Self::create_error_response(
                    json_request.id,
                    -32601,
                    "Method not found",
                    Some(format!("Unknown method: {}", json_request.method))
                )))
            }
        }
    }

    async fn handle_initialize(request: JsonRpcRequest) -> Result<ResponseJson<JsonRpcResponse>, StatusCode> {
        info!("🚀 MCP initialization request received");
        
        let result = json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": {}
            },
            "serverInfo": {
                "name": "gpt-oss-browser",
                "version": "1.0.0"
            },
            "instructions": "🌐 **GPT-OSS Browser MCP Server**\n\nBrowser tools from the GPT-OSS project for web searching and content analysis.\n\n**🛠️ Available Tools:**\n- **search**: Search for information on the web with citations\n- **open**: Open web pages and view content with line numbers\n- **find**: Find text patterns in opened pages\n\n**🔧 Features:**\n- Full MCP 2024-11-05 protocol compliance\n- Session-based browsing state\n- HTML to text conversion\n- Citation support with line numbers\n- ARM64 optimized for AWS Lambda Graviton\n\n**💡 Usage Tips:**\n- Use search to find relevant web content\n- Open URLs to view full page content\n- Use find to locate specific information within pages\n- Sessions maintain browsing history for context"
        });

        info!("✅ MCP initialization successful");
        Ok(ResponseJson(Self::create_success_response(request.id, result)))
    }

    async fn handle_ping(request: JsonRpcRequest) -> Result<ResponseJson<JsonRpcResponse>, StatusCode> {
        info!("🏓 Ping request received - responding with pong");
        
        let result = json!({
            "status": "healthy",
            "timestamp": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            "server": "gpt-oss-browser",
            "version": "1.0.0",
            "uptime": "running",
            "tools_available": 3,
            "protocol": "MCP",
            "architecture": "ARM64",
            "message": "pong"
        });

        info!("✅ Ping response sent successfully");
        Ok(ResponseJson(Self::create_success_response(request.id, result)))
    }

    async fn handle_session_terminate(request: JsonRpcRequest) -> Result<ResponseJson<JsonRpcResponse>, StatusCode> {
        info!("🔚 Session termination request received");
        
        // Extract session information if provided in params
        let session_info = if let Some(params) = &request.params {
            params.get("sessionId")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
        } else {
            "unknown"
        };
        
        info!("🔚 Terminating session: {}", session_info);
        
        // Clean up session data
        if let Ok(mut sessions) = SESSIONS.write() {
            sessions.remove(session_info);
        }
        
        let result = json!({
            "status": "terminated",
            "sessionId": session_info,
            "timestamp": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            "message": "Session terminated successfully"
        });

        info!("✅ Session termination completed for: {}", session_info);
        Ok(ResponseJson(Self::create_success_response(request.id, result)))
    }

    async fn handle_notification_cancelled(request: JsonRpcRequest) -> Result<ResponseJson<JsonRpcResponse>, StatusCode> {
        info!("🚫 Notification cancelled request received");
        
        let result = json!({
            "status": "acknowledged",
            "timestamp": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        });

        info!("✅ Notification cancellation acknowledged");
        Ok(ResponseJson(Self::create_success_response(request.id, result)))
    }

    async fn handle_tools_list(request: JsonRpcRequest) -> Result<ResponseJson<JsonRpcResponse>, StatusCode> {
        info!("📋 Tools list request received");
        
        let tools_list: Vec<Value> = TOOLS.values().cloned().collect();
        let result = json!({
            "tools": tools_list
        });

        info!("✅ Returned {} tools", tools_list.len());
        Ok(ResponseJson(Self::create_success_response(request.id, result)))
    }

    async fn handle_tools_call(request: JsonRpcRequest, session_id: &str) -> Result<ResponseJson<JsonRpcResponse>, StatusCode> {
        let params = match request.params {
            Some(params) => params,
            None => {
                warn!("Tool call missing parameters");
                return Ok(ResponseJson(Self::create_error_response(
                    request.id,
                    -32602,
                    "Invalid params",
                    Some("Missing parameters".to_string())
                )));
            }
        };

        let tool_name = match params.get("name").and_then(|v| v.as_str()) {
            Some(name) => name,
            None => {
                warn!("Tool call missing tool name");
                return Ok(ResponseJson(Self::create_error_response(
                    request.id,
                    -32602,
                    "Invalid params",
                    Some("Missing tool name".to_string())
                )));
            }
        };

        let arguments = params.get("arguments").cloned().unwrap_or_else(|| json!({}));

        info!("🔧 Calling tool: {}", tool_name);
        debug!("Tool arguments: {}", arguments);

        if !TOOLS.contains_key(tool_name) {
            warn!("Unknown tool: {}", tool_name);
            return Ok(ResponseJson(Self::create_error_response(
                request.id,
                -32601,
                "Method not found",
                Some(format!("Unknown tool: {}", tool_name))
            )));
        }

        // Execute the tool
        let result = Self::execute_tool(tool_name, &arguments, session_id).await;
        
        match result {
            Ok(content) => {
                info!("✅ Tool {} executed successfully", tool_name);
                let mcp_result = McpToolResult {
                    content: vec![McpContent {
                        content_type: "text".to_string(),
                        text: content,
                    }],
                };
                Ok(ResponseJson(Self::create_success_response(request.id, json!(mcp_result))))
            }
            Err(error) => {
                error!("❌ Tool {} execution failed: {}", tool_name, error);
                Ok(ResponseJson(Self::create_error_response(
                    request.id,
                    -32603,
                    "Internal error",
                    Some(error)
                )))
            }
        }
    }

    async fn execute_tool(tool_name: &str, arguments: &Value, session_id: &str) -> Result<String, String> {
        let client = reqwest::Client::builder()
            .user_agent("Mozilla/5.0 (compatible; GPT-OSS-Browser/1.0.0)")
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

        match tool_name {
            "search" => Self::execute_search(&client, arguments).await,
            "open" => Self::execute_open(&client, arguments, session_id).await,
            "find" => Self::execute_find(arguments, session_id).await,
            _ => Err(format!("Unknown tool: {}", tool_name)),
        }
    }

    async fn execute_search(client: &reqwest::Client, arguments: &Value) -> Result<String, String> {
        let query = arguments.get("query")
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: query")?;

        if query.trim().is_empty() {
            return Err("❌ Error: Search query cannot be empty.\n\nPlease provide a search term.".to_string());
        }

        let topn = arguments.get("topn")
            .and_then(|v| v.as_u64())
            .unwrap_or(10)
            .min(50)
            .max(1);

        info!("🔍 Searching web for: '{}', limit: {}", query, topn);

        // For this implementation, we'll use DuckDuckGo's instant answer API
        // In a real implementation, you would integrate with Exa or other search APIs
        let search_url = format!(
            "https://html.duckduckgo.com/html/?q={}",
            urlencoding::encode(query)
        );

        let response = client.get(&search_url)
            .header("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8")
            .send()
            .await
            .map_err(|e| format!("Network error while searching: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("❌ Search request failed with status: {}\n\nThis might be a temporary issue. Please try again later.", response.status()));
        }

        let html = response.text().await
            .map_err(|e| format!("Error reading search response: {}", e))?;

        // Parse the search results from DuckDuckGo HTML
        let results = Self::parse_search_results(&html, topn as usize)?;

        if results.is_empty() {
            return Ok(format!("🔍 No results found for query: \"{}\"\n\n💡 **Suggestions:**\n- Try different search terms\n- Check spelling\n- Use more general terms", query));
        }

        let mut formatted_results = format!("🔍 **Search Results for \"{}\":**\n\n", query);
        
        for (index, (title, url, snippet)) in results.iter().enumerate() {
            formatted_results.push_str(&format!(
                "**{}. {}**\n",
                index + 1,
                title
            ));
            
            if !snippet.is_empty() {
                formatted_results.push_str(&format!("   {}\n", snippet));
            }
            
            formatted_results.push_str(&format!(
                "   🔗 {}\n\n",
                url
            ));
        }

        formatted_results.push_str("💡 **Next steps:**\n");
        formatted_results.push_str("- Open specific URLs to view full content\n");
        formatted_results.push_str("- Use find to search within opened pages");

        Ok(formatted_results)
    }

    fn parse_search_results(html: &str, limit: usize) -> Result<Vec<(String, String, String)>, String> {
        let document = Html::parse_document(html);
        let result_selector = Selector::parse("div.result").map_err(|e| format!("CSS selector error: {}", e))?;
        let title_selector = Selector::parse("a.result__a").map_err(|e| format!("CSS selector error: {}", e))?;
        let snippet_selector = Selector::parse("a.result__snippet").map_err(|e| format!("CSS selector error: {}", e))?;

        let mut results = Vec::new();

        for result in document.select(&result_selector).take(limit) {
            let title = result
                .select(&title_selector)
                .next()
                .map(|el| el.text().collect::<String>().trim().to_string())
                .unwrap_or_else(|| "Untitled".to_string());

            let url = result
                .select(&title_selector)
                .next()
                .and_then(|el| el.value().attr("href"))
                .unwrap_or("")
                .to_string();

            let snippet = result
                .select(&snippet_selector)
                .next()
                .map(|el| el.text().collect::<String>().trim().to_string())
                .unwrap_or_else(|| String::new());

            if !url.is_empty() {
                results.push((title, url, snippet));
            }
        }

        Ok(results)
    }

    async fn execute_open(client: &reqwest::Client, arguments: &Value, session_id: &str) -> Result<String, String> {
        let url = arguments.get("url")
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: url")?;

        if url.trim().is_empty() {
            return Err("❌ Error: URL is required.".to_string());
        }

        let loc = arguments.get("loc")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;

        let num_lines = arguments.get("num_lines")
            .and_then(|v| v.as_i64())
            .unwrap_or(-1);

        info!("📂 Opening URL: {} (loc: {}, num_lines: {})", url, loc, num_lines);

        // Check if we already have this page in our session
        let session_data = {
            let sessions = SESSIONS.read().map_err(|e| format!("Session lock error: {}", e))?;
            sessions.get(session_id).cloned()
        };

        let content = if let Some(session) = session_data {
            if let Some(cached_content) = session.pages.get(url) {
                cached_content.clone()
            } else {
                Self::fetch_page_content(client, url).await?
            }
        } else {
            Self::fetch_page_content(client, url).await?
        };

        // Update session with the new page
        {
            let mut sessions = SESSIONS.write().map_err(|e| format!("Session lock error: {}", e))?;
            let session = sessions.entry(session_id.to_string()).or_default();
            session.current_url = Some(url.to_string());
            session.current_content = Some(content.clone());
            session.pages.insert(url.to_string(), content.clone());
        }

        // Format content with line numbers
        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len();

        if loc >= total_lines {
            return Err(format!(
                "❌ Invalid location parameter: {}. Cannot exceed page maximum of {}.",
                loc, total_lines.saturating_sub(1)
            ));
        }

        let end_loc = if num_lines == -1 {
            total_lines
        } else {
            (loc + num_lines as usize).min(total_lines)
        };

        let lines_to_show = &lines[loc..end_loc];
        let mut result = format!("📄 **{}**\n\n", url);
        
        if loc > 0 {
            result.push_str(&format!("📄 [Starting from line {}]\n\n", loc));
        }

        for (i, line) in lines_to_show.iter().enumerate() {
            result.push_str(&format!("L{}: {}\n", loc + i, line));
        }

        if end_loc < total_lines {
            result.push_str(&format!(
                "\n📄 [Content truncated at line {} of {}. Use loc parameter to continue reading.]",
                end_loc.saturating_sub(1), total_lines.saturating_sub(1)
            ));
        }

        result.push_str(&format!("\n\n🔗 **URL:** {}", url));
        result.push_str(&format!("\n📊 **Stats:** {} lines total", total_lines));

        Ok(result)
    }

    async fn fetch_page_content(client: &reqwest::Client, url: &str) -> Result<String, String> {
        let response = client.get(url)
            .header("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8")
            .send()
            .await
            .map_err(|e| format!("Network error while fetching page: {}", e))?;

        if response.status().as_u16() == 404 {
            return Err(format!("📄 **Page not found:** {}\n\nThe URL may be incorrect or the page may no longer exist.", url));
        }

        if !response.status().is_success() {
            return Err(format!("❌ Failed to fetch page: HTTP {}\n\nThere may be a temporary issue with the website.", response.status()));
        }

        let html = response.text().await
            .map_err(|e| format!("Error reading page response: {}", e))?;

        // Convert HTML to readable text
        let text_content = from_read(html.as_bytes(), 80)
            .map_err(|e| format!("Error converting HTML to text: {}", e))?;
        
        Ok(text_content)
    }

    async fn execute_find(arguments: &Value, session_id: &str) -> Result<String, String> {
        let pattern = arguments.get("pattern")
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: pattern")?;

        if pattern.trim().is_empty() {
            return Err("❌ Error: Search pattern cannot be empty.".to_string());
        }

        // Get the content to search in
        let (content, url) = if let Some(url_arg) = arguments.get("url").and_then(|v| v.as_str()) {
            // Search in specific URL
            let sessions = SESSIONS.read().map_err(|e| format!("Session lock error: {}", e))?;
            if let Some(session) = sessions.get(session_id) {
                if let Some(page_content) = session.pages.get(url_arg) {
                    (page_content.clone(), url_arg.to_string())
                } else {
                    return Err(format!("❌ Page not found in session: {}\nPlease open the page first.", url_arg));
                }
            } else {
                return Err("❌ No active session found.".to_string());
            }
        } else {
            // Search in current page
            let sessions = SESSIONS.read().map_err(|e| format!("Session lock error: {}", e))?;
            if let Some(session) = sessions.get(session_id) {
                if let (Some(content), Some(url)) = (&session.current_content, &session.current_url) {
                    (content.clone(), url.clone())
                } else {
                    return Err("❌ No page is currently open.\nPlease open a page first using the 'open' tool.".to_string());
                }
            } else {
                return Err("❌ No active session found.".to_string());
            }
        };

        info!("🔎 Finding pattern '{}' in {}", pattern, url);

        // Search for pattern in content
        let lines: Vec<&str> = content.lines().collect();
        let pattern_lower = pattern.to_lowercase();
        let mut matches = Vec::new();

        for (line_num, line) in lines.iter().enumerate() {
            if line.to_lowercase().contains(&pattern_lower) {
                // Show context around the match
                let start_line = line_num.saturating_sub(2);
                let end_line = (line_num + 3).min(lines.len());
                let context_lines: Vec<String> = lines[start_line..end_line]
                    .iter()
                    .enumerate()
                    .map(|(i, line)| {
                        let actual_line_num = start_line + i;
                        if actual_line_num == line_num {
                            format!("L{}: >>> {} <<<", actual_line_num, line)
                        } else {
                            format!("L{}: {}", actual_line_num, line)
                        }
                    })
                    .collect();
                
                matches.push((line_num, context_lines));
            }
        }

        if matches.is_empty() {
            return Ok(format!("🔎 No matches found for pattern: '{}'\n\n💡 **Suggestions:**\n- Check spelling\n- Try a different search term\n- Use partial words or phrases", pattern));
        }

        let mut result = format!("🔎 **Found {} match(es) for '{}' in {}:**\n\n", matches.len(), pattern, url);

        for (i, (line_num, context)) in matches.iter().enumerate().take(10) {
            result.push_str(&format!("**Match {} at line {}:**\n", i + 1, line_num));
            for context_line in context {
                result.push_str(&format!("{}\n", context_line));
            }
            result.push_str("\n");
        }

        if matches.len() > 10 {
            result.push_str(&format!("... and {} more matches (showing first 10)\n\n", matches.len() - 10));
        }

        result.push_str("💡 Use the line numbers to navigate to specific matches.");

        Ok(result)
    }

    fn create_success_response(id: Option<Value>, result: Value) -> JsonRpcResponse {
        JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    fn create_error_response(id: Option<Value>, code: i32, message: &str, data: Option<String>) -> JsonRpcResponse {
        JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.to_string(),
                data: data.map(|d| json!(d)),
            }),
        }
    }

    async fn handle_fallback(request: Request) -> Result<ResponseJson<Value>, StatusCode> {
        let method = request.method();
        let uri = request.uri();
        let headers = request.headers();
        
        warn!("Fallback handler called for: {} {}", method, uri);
        debug!("Headers: {:?}", headers);
        
        Ok(ResponseJson(json!({
            "error": "Route not found",
            "method": method.to_string(),
            "path": uri.to_string(),
            "message": "This endpoint is not available. Use POST /mcp for MCP requests."
        })))
    }
}