# Meticulous CI/CD - Development Commands
# Run `just --list` to see all available commands

set dotenv-load := true

# Default recipe - show available commands
default:
    @just --list

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

# Run agent with specific controller URL
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
