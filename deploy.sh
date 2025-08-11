#!/bin/bash

set -e

echo "🚀 Deploying GPT-OSS MCP Server to AWS Lambda (ARM64)"

# Configuration
STACK_NAME="gpt-oss-mcp-server-rust"
TEMPLATE_FILE="template.yaml"
REGION="${AWS_REGION:-us-east-1}"
ENVIRONMENT="${ENVIRONMENT:-prod}"

# Parse command line arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        -e|--environment)
            ENVIRONMENT="$2"
            shift 2
            ;;
        -r|--region)
            REGION="$2"
            shift 2
            ;;
        -s|--stack-name)
            STACK_NAME="$2"
            shift 2
            ;;
        -h|--help)
            echo "Usage: $0 [OPTIONS]"
            echo "Options:"
            echo "  -e, --environment ENV    Deployment environment (dev|staging|prod) [default: prod]"
            echo "  -r, --region REGION      AWS region [default: us-east-1]"
            echo "  -s, --stack-name NAME    CloudFormation stack name [default: gpt-oss-mcp-server]"
            echo "  -h, --help               Show this help message"
            exit 0
            ;;
        *)
            echo "❌ Unknown option: $1"
            exit 1
            ;;
    esac
done

# Validate environment
if [[ ! "$ENVIRONMENT" =~ ^(dev|staging|prod)$ ]]; then
    echo "❌ Invalid environment: $ENVIRONMENT. Must be one of: dev, staging, prod"
    exit 1
fi

# Check prerequisites
echo "🔍 Checking prerequisites..."

if ! command -v sam &> /dev/null; then
    echo "❌ AWS SAM CLI not found. Please install it first:"
    echo "   https://docs.aws.amazon.com/serverless-application-model/latest/developerguide/install-sam-cli.html"
    exit 1
fi

if ! command -v docker &> /dev/null; then
    echo "❌ Docker not found. Please install Docker first."
    exit 1
fi

if ! docker info &> /dev/null; then
    echo "❌ Docker daemon is not running. Please start Docker."
    exit 1
fi

if ! aws sts get-caller-identity &> /dev/null; then
    echo "❌ AWS credentials not configured. Please run 'aws configure'."
    exit 1
fi

echo "✅ Prerequisites check passed"

# Build the application using Docker
echo "🔨 Building application with Docker..."
./build-docker.sh

# Create the expected directory structure for SAM
echo "📁 Setting up deployment structure..."
mkdir -p target/lambda/bootstrap
cp /tmp/cargo_build/target/lambda/bootstrap/bootstrap target/lambda/bootstrap/

# Validate SAM template
echo "🔍 Validating SAM template..."
sam validate --template-file "$TEMPLATE_FILE" --region "$REGION"

# Check if stack exists and is in a failed state
FULL_STACK_NAME="${STACK_NAME}-${ENVIRONMENT}"
STACK_STATUS=$(aws cloudformation describe-stacks --stack-name "$FULL_STACK_NAME" --region "$REGION" --query 'Stacks[0].StackStatus' --output text 2>/dev/null || echo "DOES_NOT_EXIST")

if [[ "$STACK_STATUS" == "ROLLBACK_COMPLETE" ]]; then
    echo "⚠️  Stack is in ROLLBACK_COMPLETE state. Deleting and recreating..."
    aws cloudformation delete-stack --stack-name "$FULL_STACK_NAME" --region "$REGION"
    echo "⏳ Waiting for stack deletion..."
    aws cloudformation wait stack-delete-complete --stack-name "$FULL_STACK_NAME" --region "$REGION"
    echo "✅ Stack deleted successfully"
fi

# Deploy with SAM
echo "📦 Deploying with AWS SAM..."
echo "   Stack Name: $FULL_STACK_NAME"
echo "   Environment: $ENVIRONMENT"
echo "   Region: $REGION"

sam deploy \
    --template-file "$TEMPLATE_FILE" \
    --stack-name "$FULL_STACK_NAME" \
    --capabilities CAPABILITY_NAMED_IAM \
    --region "$REGION" \
    --parameter-overrides \
        Environment="$ENVIRONMENT" \
    --tags \
        Environment="$ENVIRONMENT" \
        Project="gpt-oss-mcp" \
        Architecture="arm64" \
    --no-confirm-changeset \
    --no-fail-on-empty-changeset \
    --resolve-s3

# Get deployment outputs
echo ""
echo "✅ Deployment complete!"
echo ""
echo "📊 Deployment Information:"
echo "=========================="
echo "Stack Name: $FULL_STACK_NAME"
echo "Environment: $ENVIRONMENT"
echo "Region: $REGION"
echo ""

# Get stack outputs
echo "🔗 Stack Outputs:"
aws cloudformation describe-stacks \
    --stack-name "$FULL_STACK_NAME" \
    --region "$REGION" \
    --query 'Stacks[0].Outputs[*].[OutputKey,OutputValue,Description]' \
    --output table

# Get specific endpoints
API_ENDPOINT=$(aws cloudformation describe-stacks \
    --stack-name "$FULL_STACK_NAME" \
    --region "$REGION" \
    --query 'Stacks[0].Outputs[?OutputKey==`GptOssMcpApi`].OutputValue' \
    --output text 2>/dev/null || echo "")

MCP_ENDPOINT=$(aws cloudformation describe-stacks \
    --stack-name "$FULL_STACK_NAME" \
    --region "$REGION" \
    --query 'Stacks[0].Outputs[?OutputKey==`McpEndpoint`].OutputValue' \
    --output text 2>/dev/null || echo "")

FUNCTION_ARN=$(aws cloudformation describe-stacks \
    --stack-name "$FULL_STACK_NAME" \
    --region "$REGION" \
    --query 'Stacks[0].Outputs[?OutputKey==`GptOssMcpFunction`].OutputValue' \
    --output text 2>/dev/null || echo "")

if [[ -n "$API_ENDPOINT" ]]; then
    echo ""
    echo "🌐 API Gateway Endpoint: $API_ENDPOINT"
fi

if [[ -n "$MCP_ENDPOINT" ]]; then
    echo "🔌 MCP Endpoint: $MCP_ENDPOINT"
fi

if [[ -n "$FUNCTION_ARN" ]]; then
    echo "⚡ Lambda Function: $FUNCTION_ARN"
fi

# Test the MCP endpoint
if [[ -n "$MCP_ENDPOINT" ]]; then
    echo ""
    echo "🧪 Testing MCP endpoint..."
    
    # Test initialize
    INIT_RESPONSE=$(curl -s -X POST "$MCP_ENDPOINT" \
        -H "Content-Type: application/json" \
        -d '{"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": {"name": "test", "version": "1.0"}}}' \
        2>/dev/null || echo "")
    
    if echo "$INIT_RESPONSE" | grep -q '"result"'; then
        echo "✅ MCP initialization successful"
        
        # Test tools list
        TOOLS_RESPONSE=$(curl -s -X POST "$MCP_ENDPOINT" \
            -H "Content-Type: application/json" \
            -d '{"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}}' \
            2>/dev/null || echo "")
        
        if echo "$TOOLS_RESPONSE" | grep -q '"tools"'; then
            TOOL_COUNT=$(echo "$TOOLS_RESPONSE" | grep -o '"name"' | wc -l)
            echo "✅ Tools endpoint working ($TOOL_COUNT tools available)"
        else
            echo "⚠️  Tools endpoint test failed"
        fi
    else
        echo "⚠️  MCP initialization test failed"
    fi
fi

# Get Lambda function details
if [[ -n "$FUNCTION_ARN" ]]; then
    echo ""
    echo "📋 Lambda Function Details:"
    FUNCTION_NAME=$(echo "$FUNCTION_ARN" | cut -d':' -f7)
    aws lambda get-function \
        --function-name "$FUNCTION_NAME" \
        --region "$REGION" \
        --query '{FunctionName: Configuration.FunctionName, Runtime: Configuration.Runtime, Architecture: Configuration.Architectures[0], MemorySize: Configuration.MemorySize, Timeout: Configuration.Timeout, CodeSize: Configuration.CodeSize}' \
        --output table
fi

echo ""
echo "🎉 Deployment completed successfully!"
echo ""
echo "📝 Next steps:"
echo "  1. Configure your MCP client with the endpoint URL:"
echo "     $MCP_ENDPOINT"
echo ""
echo "  2. Test the MCP tools using curl:"
echo "     curl -X POST \"$MCP_ENDPOINT\" \\"
echo "       -H \"Content-Type: application/json\" \\"
echo "       -d '{\"jsonrpc\": \"2.0\", \"id\": 1, \"method\": \"initialize\", \"params\": {\"protocolVersion\": \"2024-11-05\", \"capabilities\": {}, \"clientInfo\": {\"name\": \"test\", \"version\": \"1.0\"}}}'"
echo ""
echo "  3. Available MCP tools:"
echo "     - search: Search for content on web pages"
echo "     - open: Open and read web page content"
echo "     - find: Find specific content within pages"
echo ""
echo "  4. Monitor CloudWatch logs:"
echo "     aws logs tail /aws/lambda/gpt-oss-mcp-${ENVIRONMENT} --follow --region ${REGION}"
echo ""
echo "🏗️  Architecture: ARM64 (native compilation)"
echo "🚀 Runtime: provided.al2023"
echo "💰 Optimized for AWS Graviton processors"