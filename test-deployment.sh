#!/bin/bash

set -e

# Configuration
REGION="${AWS_REGION:-ap-southeast-1}"
ENVIRONMENT="${ENVIRONMENT:-prod}"
STACK_NAME="gpt-oss-mcp-server-${ENVIRONMENT}"

echo "🧪 Testing GPT-OSS MCP Server Deployment"
echo "========================================"

# Get MCP endpoint from CloudFormation
echo "🔍 Getting MCP endpoint..."
MCP_ENDPOINT=$(aws cloudformation describe-stacks \
    --stack-name "$STACK_NAME" \
    --region "$REGION" \
    --query 'Stacks[0].Outputs[?OutputKey==`McpEndpoint`].OutputValue' \
    --output text 2>/dev/null || echo "")

if [[ -z "$MCP_ENDPOINT" ]]; then
    echo "❌ Could not find MCP endpoint. Is the stack deployed?"
    exit 1
fi

echo "🔌 MCP Endpoint: $MCP_ENDPOINT"
echo ""

# Test 1: Initialize
echo "🧪 Test 1: MCP Initialize"
echo "-------------------------"
INIT_RESPONSE=$(curl -s -X POST "$MCP_ENDPOINT" \
    -H "Content-Type: application/json" \
    -d '{"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": {"name": "test", "version": "1.0"}}}' \
    2>/dev/null || echo "")

if echo "$INIT_RESPONSE" | grep -q '"result"'; then
    echo "✅ Initialize: SUCCESS"
    SERVER_NAME=$(echo "$INIT_RESPONSE" | grep -o '"name":"[^"]*"' | cut -d'"' -f4)
    SERVER_VERSION=$(echo "$INIT_RESPONSE" | grep -o '"version":"[^"]*"' | cut -d'"' -f4)
    echo "   Server: $SERVER_NAME v$SERVER_VERSION"
else
    echo "❌ Initialize: FAILED"
    echo "   Response: $INIT_RESPONSE"
fi

echo ""

# Test 2: List Tools
echo "🧪 Test 2: List Tools"
echo "---------------------"
TOOLS_RESPONSE=$(curl -s -X POST "$MCP_ENDPOINT" \
    -H "Content-Type: application/json" \
    -d '{"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}}' \
    2>/dev/null || echo "")

if echo "$TOOLS_RESPONSE" | grep -q '"tools"'; then
    echo "✅ Tools List: SUCCESS"
    TOOL_COUNT=$(echo "$TOOLS_RESPONSE" | grep -o '"name"' | wc -l)
    echo "   Available tools: $TOOL_COUNT"
    
    # Extract tool names
    TOOLS=$(echo "$TOOLS_RESPONSE" | grep -o '"name":"[^"]*"' | cut -d'"' -f4 | tr '\n' ', ' | sed 's/,$//')
    echo "   Tools: $TOOLS"
else
    echo "❌ Tools List: FAILED"
    echo "   Response: $TOOLS_RESPONSE"
fi

echo ""

# Test 3: Test a simple search tool
echo "🧪 Test 3: Search Tool"
echo "----------------------"
SEARCH_RESPONSE=$(curl -s -X POST "$MCP_ENDPOINT" \
    -H "Content-Type: application/json" \
    -d '{"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "search", "arguments": {"query": "rust programming language", "topn": 3}}}' \
    2>/dev/null || echo "")

if echo "$SEARCH_RESPONSE" | grep -q '"result"'; then
    echo "✅ Search Tool: SUCCESS"
    # Check if we got search results
    if echo "$SEARCH_RESPONSE" | grep -q "rust"; then
        echo "   Search results contain relevant content"
    else
        echo "   Search completed but results may be limited"
    fi
else
    echo "❌ Search Tool: FAILED"
    echo "   Response: $SEARCH_RESPONSE"
fi

echo ""

# Test 4: Lambda Function Details
echo "🧪 Test 4: Lambda Function Status"
echo "---------------------------------"
FUNCTION_NAME="gpt-oss-mcp-${ENVIRONMENT}"
FUNCTION_STATUS=$(aws lambda get-function \
    --function-name "$FUNCTION_NAME" \
    --region "$REGION" \
    --query 'Configuration.State' \
    --output text 2>/dev/null || echo "NOT_FOUND")

if [[ "$FUNCTION_STATUS" == "Active" ]]; then
    echo "✅ Lambda Function: ACTIVE"
    
    # Get function details
    ARCHITECTURE=$(aws lambda get-function \
        --function-name "$FUNCTION_NAME" \
        --region "$REGION" \
        --query 'Configuration.Architectures[0]' \
        --output text)
    
    RUNTIME=$(aws lambda get-function \
        --function-name "$FUNCTION_NAME" \
        --region "$REGION" \
        --query 'Configuration.Runtime' \
        --output text)
    
    MEMORY=$(aws lambda get-function \
        --function-name "$FUNCTION_NAME" \
        --region "$REGION" \
        --query 'Configuration.MemorySize' \
        --output text)
    
    echo "   Architecture: $ARCHITECTURE"
    echo "   Runtime: $RUNTIME"
    echo "   Memory: ${MEMORY}MB"
else
    echo "❌ Lambda Function: $FUNCTION_STATUS"
fi

echo ""

# Summary
echo "📊 Test Summary"
echo "==============="
echo "Stack: $STACK_NAME"
echo "Region: $REGION"
echo "Endpoint: $MCP_ENDPOINT"
echo ""

# Performance test
echo "🚀 Performance Test"
echo "-------------------"
echo "Testing response time..."
START_TIME=$(date +%s%N)
curl -s -X POST "$MCP_ENDPOINT" \
    -H "Content-Type: application/json" \
    -d '{"jsonrpc": "2.0", "id": 99, "method": "initialize", "params": {"protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": {"name": "perf-test", "version": "1.0"}}}' \
    > /dev/null
END_TIME=$(date +%s%N)
RESPONSE_TIME=$(( (END_TIME - START_TIME) / 1000000 ))
echo "✅ Response time: ${RESPONSE_TIME}ms"

echo ""
echo "🎉 Deployment test completed!"
echo ""
echo "💡 Usage Examples:"
echo "   # Initialize MCP session"
echo "   curl -X POST '$MCP_ENDPOINT' \\"
echo "     -H 'Content-Type: application/json' \\"
echo "     -d '{\"jsonrpc\": \"2.0\", \"id\": 1, \"method\": \"initialize\", \"params\": {\"protocolVersion\": \"2024-11-05\", \"capabilities\": {}, \"clientInfo\": {\"name\": \"test\", \"version\": \"1.0\"}}}'"
echo ""
echo "   # Search the web"
echo "   curl -X POST '$MCP_ENDPOINT' \\"
echo "     -H 'Content-Type: application/json' \\"
echo "     -d '{\"jsonrpc\": \"2.0\", \"id\": 2, \"method\": \"tools/call\", \"params\": {\"name\": \"search\", \"arguments\": {\"query\": \"your search query\", \"topn\": 5}}}'"
echo ""
echo "   # Open a webpage"
echo "   curl -X POST '$MCP_ENDPOINT' \\"
echo "     -H 'Content-Type: application/json' \\"
echo "     -d '{\"jsonrpc\": \"2.0\", \"id\": 3, \"method\": \"tools/call\", \"params\": {\"name\": \"open\", \"arguments\": {\"url\": \"https://example.com\"}}}'"
