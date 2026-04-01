---
name: meticulous-rust-workspace
description: Use when navigating the Rust workspace, locating migrations or protobuf definitions, choosing a crate for a change, or running build, test, check, or frontend commands for Meticulous.
---

# Meticulous Rust workspace

## Overview

The repo is a **Cargo workspace** (`crates/*`). SQL migrations and protobuf sources have **fixed locations**; the web UI is a separate **Node** app under `frontend/`.

## Workspace members

| Crate | Role |
| --- | --- |
| `met-core` | Shared types, errors, configuration |
| `met-api` | Axum REST API server |
| `met-engine` | Pipeline engine: DAG, scheduler, caching, execution |
| `met-agent` | Worker agent binary |
| `met-controller` | Agent controller: registration, health, dispatch |
| `met-operator` | Kubernetes operator for agent provisioning |
| `met-secrets` | Secrets broker and external provider integrations |
| `met-secret-resolve` | Pipeline secret validation and resolution |
| `met-parser` | Pipeline definition parsing (YAML, schema, DAG construction) |
| `met-cli` | Developer CLI |
| `met-store` | Database layer (sqlx); **hosts SQL migrations** |
| `met-objstore` | S3-compatible object storage abstraction |
| `met-logging` | Log shipping, streaming, aggregation |
| `met-telemetry` | OpenTelemetry metrics and tracing |
| `met-proto` | Protobuf definitions and generated gRPC code |

Descriptions align with each crate’s `Cargo.toml`.

## Migrations

SQL migrations live in **`crates/met-store/migrations/`** (numbered `*.sql` files). There is no top-level `migrations/` directory for schema.

## Protobuf

gRPC and shared message definitions are under **`proto/`**; generated code is wired through **`met-proto`**.

## Common commands

From repository root:

```bash
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets
```

Frontend (**SvelteKit 2**, **Svelte 5**, Vite):

```bash
cd frontend && npm ci && npm run build
npm run check    # svelte-check + sync
npm run dev      # local dev server
```

## Keywords

cargo workspace, met-store, migrations, sqlx, proto, met-proto, SvelteKit, vite, crates.
