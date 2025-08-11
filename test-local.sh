#!/bin/bash

# Local testing script for GPT-OSS MCP Server
# Tests compilation and basic functionality before Docker build

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Logging functions
log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

log_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Check prerequisites
check_prerequisites() {
    log_info "Checking prerequisites..."
    
    # Check if Rust is installed
    if ! command -v cargo &> /dev/null; then
        log_error "Rust/Cargo is not installed. Please install Rust first."
        exit 1
    fi
    
    # Check Rust version
    local rust_version
    rust_version=$(rustc --version | cut -d' ' -f2)
    log_info "Rust version: $rust_version"
    
    # Check if ARM64 target is available
    if ! rustup target list --installed | grep -q "aarch64-unknown-linux-gnu"; then
        log_info "Adding ARM64 target..."
        rustup target add aarch64-unknown-linux-gnu
    fi
    
    log_success "Prerequisites check passed"
}

# Test local compilation for x86_64
test_local_compilation() {
    log_info "Testing local compilation (x86_64)..."
    
    if cargo check --target x86_64-unknown-linux-gnu; then
        log_success "Local compilation check passed"
    else
        log_error "Local compilation failed"
        exit 1
    fi
}

# Test ARM64 cross-compilation
test_arm64_compilation() {
    log_info "Testing ARM64 cross-compilation..."
    
    # Set Graviton2 optimizations
    export RUSTFLAGS="-C target-cpu=neoverse-n1"
    
    if cargo check --target aarch64-unknown-linux-gnu; then
        log_success "ARM64 cross-compilation check passed"
    else
        log_warning "ARM64 cross-compilation failed - this might require cross-compilation tools"
        log_info "To install cross-compilation tools:"
        log_info "  Ubuntu/Debian: sudo apt-get install gcc-aarch64-linux-gnu"
        log_info "  macOS: Install via Docker or use native Apple Silicon"
    fi
}

# Test with clippy
test_clippy() {
    log_info "Running Clippy (Rust linter)..."
    
    if cargo clippy --target aarch64-unknown-linux-gnu -- -D warnings; then
        log_success "Clippy check passed"
    else
        log_warning "Clippy found issues - consider fixing them"
    fi
}

# Test formatting
test_format() {
    log_info "Checking code formatting..."
    
    if cargo fmt -- --check; then
        log_success "Code formatting check passed"
    else
        log_warning "Code formatting issues found. Run 'cargo fmt' to fix."
    fi
}

# Show build information
show_build_info() {
    log_info "Build configuration:"
    echo "===================="
    
    log_info "Default target: aarch64-unknown-linux-gnu (ARM64)"
    log_info "Graviton2 optimization: -C target-cpu=neoverse-n1"
    log_info "Binary name: bootstrap (AWS Lambda compatible)"
    log_info "Runtime: provided.al2023"
    
    # Show target info if available
    if command -v rustc &> /dev/null; then
        log_info "Available targets:"
        rustup target list --installed | grep -E "(aarch64|x86_64)" | head -5
    fi
}

# Main execution
main() {
    log_info "🧪 Starting GPT-OSS MCP Server local testing"
    log_info "============================================="
    
    check_prerequisites
    show_build_info
    test_local_compilation
    test_arm64_compilation
    test_clippy
    test_format
    
    log_success "🎉 Local testing completed!"
    log_info "📝 Next steps:"
    log_info "   1. Build Docker image: ./build-docker.sh"
    log_info "   2. Deploy to AWS: ./deploy.sh"
    log_info ""
    log_info "💡 Tips:"
    log_info "   - Use 'cargo build --release --target aarch64-unknown-linux-gnu' for release build"
    log_info "   - The binary will be optimized for AWS Graviton2 processors"
}

# Run main function
main "$@"