# GPT-OSS Browser MCP Server

A Rust-based MCP (Model Context Protocol) server that provides web browsing tools, optimized for ARM64 deployment on AWS Lambda.

## 🌟 Features

- **🌐 Web Browsing Tools**: Search, open, and find functionality for web content
- **🏗️ ARM64 Optimized**: Native ARM64 compilation for AWS Lambda Graviton processors
- **📋 MCP 2024-11-05 Compliant**: Full protocol compliance with modern MCP standard
- **🔄 Session Management**: Stateful browsing with in-memory session storage
- **⚡ Performance**: Optimized for serverless deployment with fast cold starts
- **📊 Monitoring**: CloudWatch integration with comprehensive logging and alarms
- **🔒 Security**: CORS-enabled with session-based authentication

## 🛠️ Available Tools

### 1. Search (`search`)
Search for information on the web using DuckDuckGo and return formatted results with citations.

**Parameters:**
- `query` (required): Search query string
- `topn` (optional): Number of results to return (default: 10, max: 50)

**Example:**
```json
{
  "query": "rust programming language",
  "topn": 5
}
```

### 2. Open (`open`)
Open a web page by URL and return its content converted to text with line numbers for citation.

**Parameters:**
- `url` (required): URL to open
- `loc` (optional): Starting line number (default: 0)
- `num_lines` (optional): Number of lines to show (-1 for all, default: -1)

**Example:**
```json
{
  "url": "https://www.rust-lang.org",
  "loc": 0,
  "num_lines": 50
}
```

### 3. Find (`find`)
Find specific text patterns in the currently opened page or a specific URL.

**Parameters:**
- `pattern` (required): Text pattern to search for (case-insensitive)
- `url` (optional): URL of the page to search in (uses current page if omitted)

**Example:**
```json
{
  "pattern": "memory safety",
  "url": "https://www.rust-lang.org"
}
```

## 🚀 Quick Start

### Prerequisites

- **Docker**: For building the Lambda image
- **AWS CLI**: Configured with appropriate permissions
- **AWS SAM CLI**: For deployment
- **Rust** (optional): For local development

### 1. Clone and Build

```bash
# Navigate to the project directory
cd gpt-oss-mcp-server-rust

# Build the Docker image
./build-docker.sh
```

### 2. Deploy to AWS

```bash
# Deploy to production (default)
./deploy.sh

# Deploy to specific environment and region
./deploy.sh -e dev -r us-west-2

# Deploy with custom stack name
./deploy.sh -s my-mcp-server -e staging

# Get help with all options
./deploy.sh --help
```

**Deploy Script Features:**
- ✅ Automated prerequisites check (AWS CLI, SAM CLI, Docker)
- ✅ Native ARM64 compilation with Docker
- ✅ Smart stack management (handles failed deployments)
- ✅ Comprehensive endpoint testing after deployment
- ✅ Detailed deployment summary with performance metrics

**Test Deployment:**
```bash
# Run comprehensive deployment validation
./test-deployment.sh
```

### 3. Configure MCP Client

After deployment, you'll receive an MCP endpoint URL. Add it to your MCP client configuration:

```json
{
	"mcpServers": {
		"gpt-oss-browser": {
			"command": "npx",
			"args": [
				"mcp-remote",
        "https://your-api-gateway-url/prod/mcp"
			]
		}
	}
}
```

## 🏗️ Architecture

### Components

- **Lambda Function**: ARM64-optimized Rust application using `lambda_runtime` and `axum`
- **API Gateway**: HTTP endpoint with CORS support and proxy integration
- **CloudWatch**: Logging and monitoring with error/duration alarms
- **Docker**: Multi-stage build for ARM64 using Amazon Linux 2023

### Session Management

The server maintains browsing sessions using in-memory storage:

- Each client gets a unique session ID via `Mcp-Session-Id` header
- Pages are cached within sessions for efficient access
- Session cleanup on termination
- Thread-safe concurrent access with `RwLock`

### ARM64 Optimization

- **Native ARM64 compilation**: Compiles directly on ARM64 architecture using AL2023 base image
- **Graviton2/3 processor optimization**: Uses `neoverse-n1` target CPU for maximum efficiency
- **Reduced memory footprint**: Optimized binary size with LTO and symbol stripping
- **Faster execution on AWS Lambda**: Native compilation eliminates cross-compilation overhead
- **Simplified build process**: Single-stage Docker build using AL2023 ARM64 base image

## 🔧 Development

### Local Development

```bash
# Test local compilation and setup
./test-local.sh

# Install dependencies and build
cargo build --release

# Run tests
cargo test

# Format code
cargo fmt

# Lint code
cargo clippy
```

### Building for ARM64

The project uses **native ARM64 compilation** for optimal performance:

```bash
# Build using Docker (recommended - native ARM64 compilation)
./build-docker.sh

# This creates a native ARM64 binary compiled directly on ARM64 architecture
# No cross-compilation tools needed!

# For local development
cargo build --release

# The .cargo/config.toml includes Graviton2 optimizations:
# RUSTFLAGS="-C target-cpu=neoverse-n1"
```

**Native vs Cross-compilation Benefits:**
- ✅ **Faster builds**: No cross-compilation overhead
- ✅ **Better optimization**: Native CPU feature detection
- ✅ **Simpler setup**: No cross-compilation toolchain required
- ✅ **Smaller binaries**: Better dead code elimination
- ✅ **Consistent results**: Same architecture for build and runtime

### Key Dependencies

- **lambda_runtime**: AWS Lambda runtime for Rust
- **axum**: Modern web framework for routing and middleware
- **reqwest**: HTTP client with rustls for better cross-compilation
- **scraper**: HTML parsing for web content extraction
- **html2text**: HTML to text conversion
- **serde**: JSON serialization/deserialization
- **tokio**: Async runtime

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `RUST_LOG` | Log level | `info` |
| `ENVIRONMENT` | Deployment environment | `prod` |
| `LAMBDA_ARCH` | Lambda architecture | `arm64` |

## 📊 Monitoring

### CloudWatch Metrics

The deployment includes CloudWatch alarms for:

- **Function Errors**: Monitors Lambda function errors (threshold: 5 errors in 10 minutes)
- **Function Duration**: Monitors execution time (threshold: 25 seconds average)
- **API Gateway**: Request/response metrics

### Logs

- **Lambda Logs**: `/aws/lambda/gpt-oss-mcp-{environment}`
- **API Gateway Logs**: Integrated with Lambda logs
- **Retention**: 14 days

### Health Checks

- **Health Endpoint**: `GET /health`
- **Root Endpoint**: `GET /` (server info)
- **MCP Endpoint**: `POST /mcp` (JSON-RPC)

## 🔒 Security

### Authentication

The server supports session-based authentication:

- Header-based session management via `Mcp-Session-Id`
- CORS configuration for cross-origin requests
- Request validation and error handling
- No sensitive information leakage in error responses

### CORS Configuration

- **Allow-Origin**: `*` (configurable)
- **Allow-Methods**: `GET, POST, OPTIONS, PUT, DELETE`
- **Allow-Headers**: `Content-Type, Authorization, Mcp-Session-Id`
- **Max-Age**: 3600 seconds

## 📈 Performance

### Optimization Features

- **Binary Size**: Optimized with LTO and size optimization (`opt-level = "z"`)
- **Cold Start**: Minimal dependencies for fast initialization
- **Memory**: 1GB Lambda allocation for optimal performance
- **Timeout**: 30-second timeout for web requests
- **Connection Pooling**: Reused HTTP connections via reqwest

### Scaling

- **Concurrent Executions**: Auto-scaling based on demand
- **Session Isolation**: Thread-safe session management
- **Stateless Design**: Each request is independent (except for session data)

## 🛠️ Deployment Options

### Environments

- **dev**: Development environment
- **staging**: Staging environment  
- **prod**: Production environment (default)

### Regions

Supports all AWS regions with ARM64 Lambda support.

### Custom Deployment

```bash
# Custom stack name and region
./deploy.sh -s my-stack -r eu-west-1 -e staging

# With custom parameters
sam deploy \
  --stack-name custom-stack \
  --parameter-overrides Environment=custom \
  --region us-west-2
```

## 🔍 Troubleshooting

### Common Issues

1. **Docker Build Fails**
   - Ensure Docker is running
   - Check ARM64 buildx support: `docker buildx ls`
   - Verify platform: `docker info | grep Architecture`

2. **Deployment Fails**
   - Verify AWS credentials: `aws sts get-caller-identity`
   - Check SAM CLI version: `sam --version`
   - Ensure sufficient IAM permissions

3. **Function Timeout**
   - Check CloudWatch logs for specific errors
   - Consider increasing timeout in template.yaml
   - Verify network connectivity for web requests

4. **Session Issues**
   - Ensure `Mcp-Session-Id` header is included
   - Check session cleanup in logs
   - Verify concurrent access patterns

### Debug Mode

Enable debug logging:

```bash
export RUST_LOG=debug
./deploy.sh
```

## 📚 API Reference

### MCP Protocol

The server implements MCP 2024-11-05 with these methods:

- `initialize`: Initialize MCP session with server capabilities
- `tools/list`: List available tools (search, open, find)
- `tools/call`: Execute a tool with parameters
- `ping`: Health check with server status
- `session/terminate`: Clean up session data
- `notifications/cancelled`: Handle cancelled requests

### HTTP Endpoints

- `POST /mcp`: MCP JSON-RPC endpoint
- `POST /`: Alternative MCP endpoint (root)
- `GET /health`: Health check with server status
- `GET /`: Server information and capabilities
- `DELETE /sessions/{id}`: Terminate specific session

### Error Codes

- `-32700`: Parse error (invalid JSON)
- `-32600`: Invalid Request (malformed JSON-RPC)
- `-32601`: Method not found
- `-32602`: Invalid params
- `-32603`: Internal error

## 🤝 Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Test thoroughly with `./test-local.sh`
5. Submit a pull request

## 📄 License

This project is part of the GPT-OSS ecosystem. See the main GPT-OSS repository for license information.

## 🔗 Related Projects

- [GPT-OSS](https://github.com/openai/gpt-oss): Main GPT-OSS project
- [MCP Specification](https://spec.modelcontextprotocol.io/): Model Context Protocol spec
- [Claude Desktop](https://claude.ai/desktop): MCP client example
