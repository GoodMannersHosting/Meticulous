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
# CI Simulation
# ============================================================================

# Run the full CI pipeline locally
ci: check fmt-check clippy test

# Full CI with database tests
ci-full: up db-migrate ci sqlx-check
