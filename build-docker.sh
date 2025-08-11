#!/bin/bash

set -e

echo "🐳 Building GPT-OSS MCP Server with Docker (AL2023 ARM64)..."

# Clean up any previous builds
rm -rf target/docker-build/
mkdir -p target/docker-build/

# Build the Docker image
echo "📦 Building Docker image..."
docker build --platform linux/arm64 -f Dockerfile.lambda -t gpt-oss-mcp-server .

# Extract the bootstrap binary from the container
echo "📤 Extracting bootstrap binary..."
docker create --platform linux/arm64 --name temp-container gpt-oss-mcp-server
docker cp temp-container:/var/runtime/bootstrap target/docker-build/
docker rm temp-container

# Create the deployment package
echo "📦 Creating deployment package..."
cd target/docker-build/
zip -r bootstrap.zip bootstrap
cd ../..

# Copy to the expected SAM location
mkdir -p /tmp/cargo_build/target/lambda/bootstrap/
cp target/docker-build/bootstrap /tmp/cargo_build/target/lambda/bootstrap/
cp target/docker-build/bootstrap.zip /tmp/cargo_build/target/lambda/bootstrap/

echo "✅ Docker build complete!"
echo "📊 Binary info:"
ls -la target/docker-build/bootstrap
file target/docker-build/bootstrap