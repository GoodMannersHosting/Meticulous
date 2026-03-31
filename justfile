# Meticulous CI/CD - Development Commands
# Run `just --list` to see all available commands

set dotenv-load := true

# Default recipe - show available commands
default:
    @just --list

# ============================================================================
# Build Host Setup
# ============================================================================

# Check if all build requirements are installed (works on macOS and Linux)
check-requirements:
    #!/usr/bin/env bash
    set -euo pipefail
    
    echo "Checking build requirements..."
    echo ""
    
    missing=()
    
    # Check Rust
    if command -v rustc &> /dev/null; then
        version=$(rustc --version)
        echo "✓ Rust: $version"
    else
        echo "✗ Rust: not installed"
        missing+=("rust")
    fi
    
    # Check Cargo
    if command -v cargo &> /dev/null; then
        version=$(cargo --version)
        echo "✓ Cargo: $version"
    else
        echo "✗ Cargo: not installed"
        missing+=("cargo")
    fi
    
    # Check rustfmt
    if rustup component list 2>/dev/null | grep -q "rustfmt.*installed"; then
        echo "✓ rustfmt: installed"
    else
        echo "✗ rustfmt: not installed"
        missing+=("rustfmt")
    fi
    
    # Check clippy
    if rustup component list 2>/dev/null | grep -q "clippy.*installed"; then
        echo "✓ clippy: installed"
    else
        echo "✗ clippy: not installed"
        missing+=("clippy")
    fi
    
    # Check just
    if command -v just &> /dev/null; then
        version=$(just --version)
        echo "✓ just: $version"
    else
        echo "✗ just: not installed"
        missing+=("just")
    fi
    
    # Check protoc (optional but recommended)
    if command -v protoc &> /dev/null; then
        version=$(protoc --version)
        echo "✓ protoc: $version"
    else
        echo "○ protoc: not installed (optional - prost-build bundles protoc)"
    fi
    
    # Check buf (for proto linting)
    if command -v buf &> /dev/null; then
        version=$(buf --version 2>&1 || echo "unknown")
        echo "✓ buf: $version"
    else
        echo "○ buf: not installed (needed for proto-lint, proto-breaking)"
        missing+=("buf")
    fi
    
    # Check sqlx-cli
    if command -v sqlx &> /dev/null; then
        version=$(sqlx --version)
        echo "✓ sqlx-cli: $version"
    else
        echo "○ sqlx-cli: not installed (needed for db-migrate, sqlx-prepare)"
        missing+=("sqlx-cli")
    fi
    
    # Check cargo-watch (optional for dev)
    if cargo install --list 2>/dev/null | grep -q "^cargo-watch"; then
        echo "✓ cargo-watch: installed"
    else
        echo "○ cargo-watch: not installed (needed for dev, watch commands)"
        missing+=("cargo-watch")
    fi
    
    # Check cross (optional for cross-compilation)
    if cargo install --list 2>/dev/null | grep -q "^cross"; then
        echo "✓ cross: installed"
    else
        echo "○ cross: not installed (needed for cross-platform builds)"
        missing+=("cross")
    fi
    
    # Check Node.js (for frontend)
    if command -v node &> /dev/null; then
        version=$(node --version)
        echo "✓ Node.js: $version"
    else
        echo "○ Node.js: not installed (needed for frontend development)"
        missing+=("node")
    fi
    
    # Check npm
    if command -v npm &> /dev/null; then
        version=$(npm --version)
        echo "✓ npm: $version"
    else
        echo "○ npm: not installed (needed for frontend development)"
        missing+=("npm")
    fi
    
    # macOS-specific checks
    if [[ "$(uname)" == "Darwin" ]]; then
        echo ""
        echo "macOS-specific requirements:"
        
        # Check Xcode Command Line Tools
        if xcode-select -p &> /dev/null; then
            echo "✓ Xcode Command Line Tools: installed"
        else
            echo "✗ Xcode Command Line Tools: not installed"
            missing+=("xcode-cli")
        fi
        
        # Check Homebrew
        if command -v brew &> /dev/null; then
            echo "✓ Homebrew: installed"
        else
            echo "○ Homebrew: not installed (recommended for installing dependencies)"
            missing+=("homebrew")
        fi
        
        # Check macOS targets
        echo ""
        echo "Rust targets for macOS builds:"
        if rustup target list 2>/dev/null | grep -q "x86_64-apple-darwin (installed)"; then
            echo "✓ x86_64-apple-darwin: installed"
        else
            echo "○ x86_64-apple-darwin: not installed"
            missing+=("target-x86_64-apple-darwin")
        fi
        if rustup target list 2>/dev/null | grep -q "aarch64-apple-darwin (installed)"; then
            echo "✓ aarch64-apple-darwin: installed"
        else
            echo "○ aarch64-apple-darwin: not installed"
            missing+=("target-aarch64-apple-darwin")
        fi
    fi
    
    echo ""
    if [ ${#missing[@]} -eq 0 ]; then
        echo "All requirements satisfied!"
    else
        echo "Missing or optional components: ${missing[*]}"
        echo "Run 'just setup-macos' on macOS or 'just setup-linux' on Linux to install."
    fi

# Install all build requirements on macOS
setup-macos:
    #!/usr/bin/env bash
    set -euo pipefail
    
    if [[ "$(uname)" != "Darwin" ]]; then
        echo "Error: This recipe is for macOS only"
        exit 1
    fi
    
    echo "Setting up macOS build environment..."
    echo ""
    
    # Install Xcode Command Line Tools if not present
    if ! xcode-select -p &> /dev/null; then
        echo "Installing Xcode Command Line Tools..."
        xcode-select --install
        echo "Please complete the Xcode CLI installation and re-run this command."
        exit 0
    fi
    echo "✓ Xcode Command Line Tools"
    
    # Install Homebrew if not present
    if ! command -v brew &> /dev/null; then
        echo "Installing Homebrew..."
        /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
        # Add to path for this session
        eval "$(/opt/homebrew/bin/brew shellenv 2>/dev/null || /usr/local/bin/brew shellenv)"
    fi
    echo "✓ Homebrew"
    
    # Install Rust via rustup if not present
    if ! command -v rustup &> /dev/null; then
        echo "Installing Rust via rustup..."
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
        source "$HOME/.cargo/env"
    fi
    echo "✓ Rust"
    
    # Ensure stable toolchain and components
    echo "Configuring Rust toolchain..."
    rustup default stable
    rustup component add rustfmt clippy
    echo "✓ rustfmt and clippy"
    
    # Add macOS targets for cross-compilation (both Intel and Apple Silicon)
    echo "Adding macOS build targets..."
    rustup target add x86_64-apple-darwin
    rustup target add aarch64-apple-darwin
    echo "✓ macOS targets (x86_64 and aarch64)"
    
    # Install just
    if ! command -v just &> /dev/null; then
        echo "Installing just..."
        brew install just
    fi
    echo "✓ just"
    
    # Install protobuf (for proto tooling)
    if ! command -v protoc &> /dev/null; then
        echo "Installing protobuf..."
        brew install protobuf
    fi
    echo "✓ protobuf"
    
    # Install buf (for proto linting)
    if ! command -v buf &> /dev/null; then
        echo "Installing buf..."
        brew install bufbuild/buf/buf
    fi
    echo "✓ buf"
    
    # Install sqlx-cli
    if ! command -v sqlx &> /dev/null; then
        echo "Installing sqlx-cli..."
        cargo install sqlx-cli --no-default-features --features rustls,postgres
    fi
    echo "✓ sqlx-cli"
    
    # Install cargo-watch
    if ! cargo install --list 2>/dev/null | grep -q "^cargo-watch"; then
        echo "Installing cargo-watch..."
        cargo install cargo-watch
    fi
    echo "✓ cargo-watch"
    
    # Install Node.js (for frontend)
    if ! command -v node &> /dev/null; then
        echo "Installing Node.js..."
        brew install node
    fi
    echo "✓ Node.js"
    
    echo ""
    echo "macOS build environment setup complete!"
    echo "Run 'just check-requirements' to verify installation."

# Install all build requirements on Linux (Debian/Ubuntu)
setup-linux:
    #!/usr/bin/env bash
    set -euo pipefail
    
    if [[ "$(uname)" != "Linux" ]]; then
        echo "Error: This recipe is for Linux only"
        exit 1
    fi
    
    echo "Setting up Linux build environment..."
    echo ""
    
    # Install system dependencies
    echo "Installing system dependencies..."
    if command -v apt-get &> /dev/null; then
        sudo apt-get update
        sudo apt-get install -y build-essential pkg-config libssl-dev protobuf-compiler
    elif command -v dnf &> /dev/null; then
        sudo dnf install -y gcc gcc-c++ make openssl-devel protobuf-compiler
    elif command -v pacman &> /dev/null; then
        sudo pacman -S --needed base-devel openssl protobuf
    else
        echo "Warning: Unknown package manager. Please install build-essential, pkg-config, libssl-dev, and protobuf manually."
    fi
    echo "✓ System dependencies"
    
    # Install Rust via rustup if not present
    if ! command -v rustup &> /dev/null; then
        echo "Installing Rust via rustup..."
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
        source "$HOME/.cargo/env"
    fi
    echo "✓ Rust"
    
    # Ensure stable toolchain and components
    echo "Configuring Rust toolchain..."
    rustup default stable
    rustup component add rustfmt clippy
    echo "✓ rustfmt and clippy"
    
    # Install just
    if ! command -v just &> /dev/null; then
        echo "Installing just..."
        cargo install just
    fi
    echo "✓ just"
    
    # Install buf (for proto linting)
    if ! command -v buf &> /dev/null; then
        echo "Installing buf..."
        BUF_VERSION="1.47.2"
        curl -sSL "https://github.com/bufbuild/buf/releases/download/v${BUF_VERSION}/buf-$(uname -s)-$(uname -m)" -o /tmp/buf
        chmod +x /tmp/buf
        sudo mv /tmp/buf /usr/local/bin/buf
    fi
    echo "✓ buf"
    
    # Install sqlx-cli
    if ! command -v sqlx &> /dev/null; then
        echo "Installing sqlx-cli..."
        cargo install sqlx-cli --no-default-features --features rustls,postgres
    fi
    echo "✓ sqlx-cli"
    
    # Install cargo-watch
    if ! cargo install --list 2>/dev/null | grep -q "^cargo-watch"; then
        echo "Installing cargo-watch..."
        cargo install cargo-watch
    fi
    echo "✓ cargo-watch"
    
    echo ""
    echo "Linux build environment setup complete!"
    echo "Run 'just check-requirements' to verify installation."
    echo ""
    echo "For frontend development, also install Node.js:"
    echo "  curl -fsSL https://deb.nodesource.com/setup_lts.x | sudo -E bash -"
    echo "  sudo apt-get install -y nodejs"

# ============================================================================
# Development Environment
# ============================================================================

# Start all development services (Postgres, NATS, SeaweedFS)
up:
    sudo podman compose up -d

# Stop all development services
down:
    sudo podman compose down

# Stop and remove all data volumes
clean:
    sudo podman compose down -v

# Show service logs
logs *args:
    sudo podman compose logs {{ args }}

# ============================================================================
# Database
# ============================================================================

# Start only the database
db-up:
    sudo podman compose up -d postgres

# Run database migrations
db-migrate:
    cargo sqlx migrate run --source crates/met-store/migrations

# Reset database (drop and recreate)
db-reset:
    cargo sqlx database reset --source crates/met-store/migrations -y

# Create a new migration
db-new name:
    cargo sqlx migrate add -r {{ name }} --source crates/met-store/migrations

# Prepare sqlx offline cache
sqlx-prepare:
    cargo sqlx prepare --workspace

# Check sqlx offline cache is up to date
sqlx-check:
    cargo sqlx prepare --workspace --check

# ============================================================================
# Build & Test
# ============================================================================

# Build all crates
build:
    cargo build --workspace

# Build in release mode
build-release:
    cargo build --workspace --release

# Run all tests
test:
    cargo test --workspace

# Run tests with output
test-verbose:
    cargo test --workspace -- --nocapture

# Run a specific test
test-one name:
    cargo test --workspace {{ name }} -- --nocapture

# ============================================================================
# Code Quality
# ============================================================================

# Check code compiles
check:
    cargo check --workspace

# Format code
fmt:
    cargo fmt --all

# Check formatting
fmt-check:
    cargo fmt --all -- --check

# Run clippy lints
clippy:
    cargo clippy --workspace -- -D warnings

# Run all lints (fmt + clippy)
lint: fmt-check clippy

# Fix clippy warnings automatically
fix:
    cargo clippy --workspace --fix --allow-dirty

# ============================================================================
# Proto
# ============================================================================

# Lint protobuf files (requires buf)
proto-lint:
    buf lint proto/

# Check protobuf breaking changes (requires buf)
proto-breaking:
    buf breaking proto/ --against '.git#branch=main'

# Generate protobuf code
proto-gen:
    buf generate proto/

# ============================================================================
# Running Services
# ============================================================================

# Run the API server
run-api:
    cargo run --bin met-api

# Run the API server in development mode with auto-reload
dev: db-migrate
    cargo watch -x 'run --bin met-api'

# Run the agent
run-agent:
    cargo run --bin met-agent

# Run the CLI
run-cli *args:
    cargo run --bin met -- {{ args }}

# Watch and rebuild on changes (requires cargo-watch)
watch:
    cargo watch -x 'build --workspace'

# Watch and run tests on changes
watch-test:
    cargo watch -x 'test --workspace'

# ============================================================================
# Frontend Development
# ============================================================================

# Install frontend dependencies
frontend-install:
    cd frontend && npm install

# Run frontend dev server
frontend-dev:
    cd frontend && npm run dev

# Build frontend for production
frontend-build:
    cd frontend && npm run build

# ============================================================================
# Full Stack Development
# ============================================================================

# Start full dev environment (containers + API + frontend)
dev-all: up db-migrate
    @echo "Starting API server and frontend..."
    @echo "API will be at http://localhost:8080"
    @echo "Frontend will be at http://localhost:5173"
    @just dev &
    @just frontend-dev

# ============================================================================
# OpenAPI
# ============================================================================

# Generate the OpenAPI spec JSON
openapi-generate:
    cargo run --bin met-api -- --dump-openapi > openapi.json

# Check that the committed OpenAPI spec is up to date
openapi-check:
    cargo run --bin met-api -- --dump-openapi > /tmp/openapi-check.json
    diff -u openapi.json /tmp/openapi-check.json || (echo "OpenAPI spec is out of date. Run 'just openapi-generate' to update." && exit 1)

# ============================================================================
# CI Simulation
# ============================================================================

# Run the full CI pipeline locally
ci: check fmt-check clippy test

# Full CI with database tests
ci-full: up db-migrate ci sqlx-check

# ============================================================================
# Agent Build & Deployment
# ============================================================================

# Build agent container image (amd64)
agent-build-container:
    podman build -t meticulous/agent:latest -f Dockerfile.agent --platform linux/amd64 .

# Build agent container with a custom tag
agent-build-container-tag tag:
    podman build -t meticulous/agent:{{ tag }} -f Dockerfile.agent --platform linux/amd64 .

# Build agent binary for the current platform
agent-build-binary:
    cargo build --release --bin met-agent

# Build agent binary for amd64 Linux (cross-compile)
agent-build-binary-amd64:
    cargo build --release --bin met-agent --target x86_64-unknown-linux-gnu

# Run agent locally (for development)
agent-run:
    cargo run --bin met-agent

# Run agent controller gRPC server (needs Postgres, NATS, MET_CONTROLLER_JWT_SECRET)
controller-run:
    cargo run --bin met-controller

# Run agent with specific controller URL (gRPC, default port 9090 — not the REST API on 8080)
agent-run-with-controller url:
    MET_CONTROLLER_URL={{ url }} cargo run --bin met-agent

# ============================================================================
# Cross-Platform Builds
# ============================================================================

# Install cross-compilation tool
cross-install:
    cargo install cross --git https://github.com/cross-rs/cross

# Build all binaries for Linux amd64 (musl - static)
build-linux-amd64-static:
    cross build --release --target x86_64-unknown-linux-musl --bin met --bin met-api --bin met-agent

# Build all binaries for Linux arm64 (musl - static)
build-linux-arm64-static:
    cross build --release --target aarch64-unknown-linux-musl --bin met --bin met-api --bin met-agent

# Build all binaries for Linux amd64 (glibc)
build-linux-amd64:
    cargo build --release --target x86_64-unknown-linux-gnu --bin met --bin met-api --bin met-agent

# Build all binaries for macOS amd64 (requires macOS host)
build-darwin-amd64:
    cargo build --release --target x86_64-apple-darwin --bin met --bin met-api --bin met-agent

# Build all binaries for macOS arm64 (requires macOS host)
build-darwin-arm64:
    cargo build --release --target aarch64-apple-darwin --bin met --bin met-api --bin met-agent

# Build agent for all Linux targets (requires cross)
agent-build-all-linux: cross-install
    @echo "Building met-agent for Linux amd64 (musl)..."
    cross build --release --target x86_64-unknown-linux-musl --bin met-agent
    @echo "Building met-agent for Linux arm64 (musl)..."
    cross build --release --target aarch64-unknown-linux-musl --bin met-agent
    @echo "Done! Binaries in target/*/release/"

# Create release tarballs for all built targets
package-release tag:
    @mkdir -p dist
    @echo "Packaging Linux amd64 static..."
    @if [ -f target/x86_64-unknown-linux-musl/release/met-agent ]; then \
        tar -czvf dist/met-agent-{{ tag }}-linux-amd64-static.tar.gz \
            -C target/x86_64-unknown-linux-musl/release met-agent; \
    fi
    @echo "Packaging Linux arm64 static..."
    @if [ -f target/aarch64-unknown-linux-musl/release/met-agent ]; then \
        tar -czvf dist/met-agent-{{ tag }}-linux-arm64-static.tar.gz \
            -C target/aarch64-unknown-linux-musl/release met-agent; \
    fi
    @echo "Release artifacts in dist/"

# ============================================================================
# Operator Build & Deploy
# ============================================================================

# Build operator container image
operator-build-container:
    podman build -t meticulous/operator:latest -f Dockerfile.operator --platform linux/amd64 .

# Install CRDs to current Kubernetes cluster
operator-install-crds:
    kubectl apply -f crates/met-operator/crds/

# Run operator locally (for development)
operator-run:
    cargo run --bin met-operator
