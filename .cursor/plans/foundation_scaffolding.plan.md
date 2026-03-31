---
name: Foundation and Scaffolding
overview: "Phase 0: Rust workspace scaffolding, met-core shared types, PostgreSQL schema, protobuf definitions, and CI bootstrap for the Meticulous CI/CD platform."
todos:
  - id: foundation-workspace
    content: Scaffold Cargo workspace root and all crate stubs with correct inter-crate dependencies
    status: pending
  - id: foundation-core-errors
    content: Implement met-core error types, result aliases, and error context helpers
    status: pending
  - id: foundation-core-ids
    content: Implement met-core ID types (typed wrappers around UUIDs for every entity)
    status: pending
  - id: foundation-core-models
    content: Implement met-core domain models (Organization, Project, Pipeline, Job, Step, Agent, Secret, Variable, Trigger, ReusableWorkflow, Run, Artifact)
    status: pending
  - id: foundation-core-config
    content: Implement met-core configuration loading (TOML/env, layered config with defaults)
    status: pending
  - id: foundation-core-events
    content: Implement met-core event envelope types for NATS messaging
    status: pending
  - id: foundation-store-schema
    content: Design and write initial PostgreSQL migration (all core tables, indexes, constraints)
    status: pending
  - id: foundation-store-crate
    content: Implement met-store crate with sqlx connection pool, migration runner, and basic query modules
    status: pending
  - id: foundation-proto
    content: Write protobuf definitions for agent-controller gRPC services
    status: pending
  - id: foundation-proto-build
    content: Set up tonic-build for proto compilation and integrate into workspace
    status: pending
  - id: foundation-ci
    content: Create GitHub Actions CI pipeline (check, test, fmt, clippy, sqlx prepare)
    status: pending
  - id: foundation-dev-env
    content: Create docker-compose.yml for local dev (Postgres, NATS, SeaweedFS) and a justfile/Makefile
    status: pending
isProject: false
---

# Foundation and Scaffolding -- Detailed Plan

Parent: [Master Architecture](master_architecture_4bf1d365.plan.md)

This plan covers **Phase 0** of the Meticulous build: everything needed to have a working Rust workspace with shared types, a database schema, gRPC contract definitions, and CI -- the foundation that all later phases build on.

---

## 1. Cargo Workspace Scaffolding

### 1.1 Workspace Root `Cargo.toml`

A virtual workspace (no root package) containing all crates under `crates/`. Shared dependency versions are centralized via `[workspace.dependencies]` to keep the lockfile sane and versions uniform.

```toml
[workspace]
resolver = "2"
members = ["crates/*"]

[workspace.package]
version = "0.1.0"
edition = "2024"
license = "UNLICENSED"
repository = "https://github.com/gmh/meticulous"

[workspace.dependencies]
# Async
tokio = { version = "1", features = ["full"] }

# Web
axum = { version = "0.8" }
tower = "0.5"
tower-http = { version = "0.6", features = ["cors", "trace"] }

# Database
sqlx = { version = "0.8", features = ["runtime-tokio", "tls-rustls", "postgres", "uuid", "chrono", "json", "migrate"] }

# gRPC
tonic = "0.13"
tonic-build = "0.13"
prost = "0.13"
prost-types = "0.13"

# NATS
async-nats = "0.39"

# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"
serde_yaml = "0.9"
toml = "0.8"

# IDs and time
uuid = { version = "1", features = ["v7", "serde"] }
chrono = { version = "0.4", features = ["serde"] }

# Observability
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
opentelemetry = "0.28"

# Crypto
rustls = "0.23"
rcgen = "0.13"

# CLI
clap = { version = "4", features = ["derive", "env"] }

# Error handling
thiserror = "2"
anyhow = "1"

# Config
config = "0.15"

# Testing
insta = { version = "1", features = ["yaml"] }
```

### 1.2 Crate Stubs

Each crate gets a minimal `Cargo.toml` and `src/lib.rs` (or `src/main.rs` for binaries). The dependency graph for Phase 0:

```
met-core          (no internal deps -- leaf crate)
met-store         (depends on met-core)
met-telemetry     (depends on met-core)
met-logging       (depends on met-core, met-telemetry)
met-secrets       (depends on met-core)
met-objstore      (depends on met-core)
met-parser        (depends on met-core)
met-engine        (depends on met-core, met-store, met-parser, met-secrets)
met-controller    (depends on met-core, met-store)
met-agent         (depends on met-core)           [binary]
met-api           (depends on met-core, met-store, met-engine, met-controller)  [binary]
met-cli           (depends on met-core)           [binary]
met-operator      (depends on met-core, met-controller)
```

Only `met-core` and `met-store` get real code in Phase 0. The rest are stubs with a `// TODO: Phase N` comment so the workspace compiles and CI works from day one.

---

## 2. `met-core` Crate

The shared foundation crate. No database, no networking -- pure types, traits, and helpers.

### 2.1 Error Types (`src/error.rs`)

A unified error enum using `thiserror`:

```rust
#[derive(Debug, thiserror::Error)]
pub enum MetError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("yaml error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("gRPC error: {0}")]
    Grpc(#[from] tonic::Status),

    #[error("NATS error: {0}")]
    Nats(#[from] async_nats::Error),

    #[error("configuration error: {0}")]
    Config(String),

    #[error("not found: {entity} with id {id}")]
    NotFound { entity: &'static str, id: String },

    #[error("unauthorized: {0}")]
    Unauthorized(String),

    #[error("forbidden: {0}")]
    Forbidden(String),

    #[error("validation error: {0}")]
    Validation(String),

    #[error("internal error: {0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, MetError>;
```

Axum integration (implementing `IntoResponse`) will live in `met-api`, not here. `met-core` stays transport-agnostic.

### 2.2 Typed ID Wrappers (`src/ids.rs`)

Every entity gets a newtype around `uuid::Uuid` to prevent mixing IDs across types at compile time. Use UUIDv7 for time-sortable, index-friendly primary keys.

```rust
macro_rules! define_id {
    ($name:ident, $prefix:literal) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, sqlx::Type)]
        #[sqlx(transparent)]
        pub struct $name(pub uuid::Uuid);

        impl $name {
            pub fn new() -> Self { Self(uuid::Uuid::now_v7()) }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}_{}", $prefix, self.0)
            }
        }
    };
}

define_id!(OrganizationId, "org");
define_id!(ProjectId, "proj");
define_id!(PipelineId, "pipe");
define_id!(JobId, "job");
define_id!(StepId, "step");
define_id!(RunId, "run");
define_id!(AgentId, "agt");
define_id!(SecretId, "sec");
define_id!(VariableId, "var");
define_id!(TriggerId, "trg");
define_id!(WorkflowId, "wf");
define_id!(ArtifactId, "art");
define_id!(UserId, "usr");
define_id!(GroupId, "grp");
define_id!(TokenId, "tok");
```

### 2.3 Domain Models (`src/models/`)

Organized into submodules matching the core hierarchy. Each model is a plain struct with `serde` derives and `sqlx::FromRow` where applicable.

**Key models:**


| Module        | Structs                                                | Notes                                                               |
| ------------- | ------------------------------------------------------ | ------------------------------------------------------------------- |
| `org.rs`      | `Organization`                                         | Tenant boundary. `id`, `name`, `slug`, `created_at`                 |
| `project.rs`  | `Project`                                              | Belongs to an org. `owner_type` (user/group), `owner_id`            |
| `pipeline.rs` | `Pipeline`, `PipelineDefinition`                       | Pipeline metadata vs parsed definition                              |
| `job.rs`      | `Job`, `JobStatus`                                     | DAG node. `depends_on: Vec<JobId>`, status enum                     |
| `step.rs`     | `Step`, `StepKind`                                     | Individual command/action. Kind: `Command`, `WorkflowRef`, `Plugin` |
| `run.rs`      | `Run`, `RunStatus`, `JobRun`, `StepRun`                | Execution records. Immutable once completed                         |
| `agent.rs`    | `Agent`, `AgentStatus`, `AgentPool`, `AgentCapability` | Agent registration and pool membership                              |
| `secret.rs`   | `SecretRef`, `SecretScope`                             | Never stores plaintext. Scope: `Global`, `Project(ProjectId)`       |
| `variable.rs` | `Variable`, `VariableScope`                            | Key-value pairs, scoped like secrets                                |
| `trigger.rs`  | `Trigger`, `TriggerKind`                               | Webhook, manual, tag-push, cron schedule                            |
| `workflow.rs` | `ReusableWorkflow`, `WorkflowScope`                    | Scope: `Global` or `Project(ProjectId)`                             |
| `artifact.rs` | `Artifact`                                             | Build output. References object storage path                        |
| `user.rs`     | `User`, `Group`, `GroupMembership`                     | Identity and RBAC                                                   |


**Status enums follow a consistent pattern:**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, sqlx::Type)]
#[sqlx(type_name = "run_status", rename_all = "snake_case")]
pub enum RunStatus {
    Pending,
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    TimedOut,
}
```

### 2.4 Configuration (`src/config.rs`)

Layered configuration using the `config` crate. Load order (later overrides earlier):

1. Compiled defaults
2. `/etc/meticulous/config.toml` (system-wide)
3. `~/.config/meticulous/config.toml` (user)
4. `./meticulous.toml` (project-local)
5. Environment variables prefixed `MET_` (e.g. `MET_DATABASE__URL`)

```rust
#[derive(Debug, Clone, serde::Deserialize)]
pub struct MetConfig {
    pub database: DatabaseConfig,
    pub nats: NatsConfig,
    pub grpc: GrpcConfig,
    pub http: HttpConfig,
    pub storage: StorageConfig,
    pub log: LogConfig,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,     // default: 10
    pub min_connections: u32,     // default: 1
    pub connect_timeout_secs: u64, // default: 5
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct NatsConfig {
    pub url: String,              // default: "nats://localhost:4222"
    pub credentials_file: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct GrpcConfig {
    pub listen_addr: String,      // default: "0.0.0.0:9090"
    pub tls_cert: Option<String>,
    pub tls_key: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct HttpConfig {
    pub listen_addr: String,      // default: "0.0.0.0:8080"
    pub cors_origins: Vec<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct StorageConfig {
    pub endpoint: String,         // default: "http://localhost:9000"
    pub bucket: String,           // default: "meticulous"
    pub access_key: Option<String>,
    pub secret_key: Option<String>,
    pub region: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct LogConfig {
    pub level: String,            // default: "info"
    pub format: LogFormat,        // default: Text
}
```

### 2.5 Event Envelope (`src/events.rs`)

Typed wrapper for NATS messages. Every event gets serialized into this envelope:

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EventEnvelope<T: serde::Serialize> {
    pub id: uuid::Uuid,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub kind: &'static str,
    pub source: String,
    pub payload: T,
}
```

Concrete event payloads defined per-domain (e.g. `JobDispatched`, `AgentRegistered`, `RunCompleted`). These start as stub enums in Phase 0 and get fleshed out in later phases.

---

## 3. PostgreSQL Schema (`met-store`)

### 3.1 Design Principles

- **UUIDv7 primary keys** everywhere (time-sortable, no sequential guessing).
- `**created_at` / `updated_at`** on all tables. `updated_at` maintained by a trigger.
- **PostgreSQL enums** for status columns (type-safe, efficient, indexable).
- **Soft-delete** via `deleted_at` for audit-sensitive tables (organizations, projects, users). Hard-delete for ephemeral data (runs older than retention).
- **Row-level security (RLS)** prepared for but not enforced in Phase 0 -- columns are structured to support it.
- **JSON columns** for extensible metadata (`agent.capabilities`, `step.environment`).

### 3.2 Initial Migration (`001_initial_schema.sql`)

**Custom types:**

```sql
CREATE TYPE run_status AS ENUM ('pending','queued','running','succeeded','failed','cancelled','timed_out');
CREATE TYPE agent_status AS ENUM ('online','offline','busy','draining','decommissioned');
CREATE TYPE trigger_kind AS ENUM ('webhook','manual','tag_push','schedule');
CREATE TYPE secret_scope AS ENUM ('global','project');
CREATE TYPE variable_scope AS ENUM ('global','project');
CREATE TYPE workflow_scope AS ENUM ('global','project');
CREATE TYPE owner_type AS ENUM ('user','group');
CREATE TYPE step_kind AS ENUM ('command','workflow_ref','plugin');
```

**Core tables:**


| Table                | Key Columns                                                                                                                                                                      | Notes                                                  |
| -------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------ |
| `organizations`      | `id`, `name`, `slug`, `created_at`, `deleted_at`                                                                                                                                 | Tenant boundary                                        |
| `users`              | `id`, `org_id FK`, `username`, `email`, `display_name`, `password_hash`, `created_at`, `deleted_at`                                                                              | Local auth; OIDC adds external identity mapping later  |
| `groups`             | `id`, `org_id FK`, `name`                                                                                                                                                        | RBAC groups                                            |
| `group_memberships`  | `group_id FK`, `user_id FK`                                                                                                                                                      | Composite PK                                           |
| `projects`           | `id`, `org_id FK`, `name`, `slug`, `description`, `owner_type`, `owner_id`, `created_at`, `deleted_at`                                                                           | Owner polymorphic (user or group)                      |
| `pipelines`          | `id`, `project_id FK`, `name`, `slug`, `definition JSONB`, `created_at`, `updated_at`                                                                                            | `definition` stores the parsed pipeline config         |
| `jobs`               | `id`, `pipeline_id FK`, `name`, `depends_on UUID[]`, `agent_tags TEXT[]`, `timeout_secs INT`, `created_at`                                                                       | DAG nodes                                              |
| `steps`              | `id`, `job_id FK`, `name`, `kind step_kind`, `command TEXT`, `workflow_ref TEXT`, `environment JSONB`, `sequence INT`                                                            | Ordered within a job                                   |
| `triggers`           | `id`, `pipeline_id FK`, `kind trigger_kind`, `config JSONB`, `enabled BOOL`, `created_at`                                                                                        | Polymorphic config per kind                            |
| `runs`               | `id`, `pipeline_id FK`, `trigger_id FK NULL`, `status run_status`, `started_at`, `finished_at`, `created_at`                                                                     | Top-level execution record                             |
| `job_runs`           | `id`, `run_id FK`, `job_id FK`, `agent_id FK NULL`, `status run_status`, `started_at`, `finished_at`, `log_path TEXT`                                                            | Per-job execution                                      |
| `step_runs`          | `id`, `job_run_id FK`, `step_id FK`, `status run_status`, `exit_code INT`, `started_at`, `finished_at`, `log_path TEXT`                                                          | Per-step execution                                     |
| `agents`             | `id`, `org_id FK`, `name`, `status agent_status`, `pool TEXT`, `tags TEXT[]`, `capabilities JSONB`, `os TEXT`, `arch TEXT`, `ip_address INET`, `last_heartbeat_at`, `created_at` | Agent registry                                         |
| `agent_tokens`       | `id`, `agent_id FK NULL`, `org_id FK`, `token_hash TEXT`, `scope JSONB`, `expires_at`, `revoked_at`, `created_at`                                                                | Join tokens and session tokens                         |
| `secrets`            | `id`, `org_id FK`, `project_id FK NULL`, `scope secret_scope`, `name`, `provider TEXT`, `provider_ref TEXT`, `created_at`, `updated_at`                                          | Never stores secret values -- only external references |
| `variables`          | `id`, `org_id FK`, `project_id FK NULL`, `scope variable_scope`, `name`, `value TEXT`, `is_sensitive BOOL`, `created_at`, `updated_at`                                           | Plaintext values (sensitive ones masked in UI)         |
| `reusable_workflows` | `id`, `org_id FK`, `project_id FK NULL`, `scope workflow_scope`, `name`, `version TEXT`, `definition JSONB`, `created_at`, `updated_at`                                          | Global or project-scoped                               |
| `artifacts`          | `id`, `run_id FK`, `job_run_id FK`, `name`, `content_type TEXT`, `size_bytes BIGINT`, `storage_path TEXT`, `sha256 TEXT`, `created_at`                                           | Object storage references                              |


**Indexes:**

```sql
-- Lookup patterns that will be hot paths
CREATE INDEX idx_projects_org ON projects(org_id);
CREATE INDEX idx_pipelines_project ON pipelines(project_id);
CREATE INDEX idx_runs_pipeline_status ON runs(pipeline_id, status);
CREATE INDEX idx_runs_created ON runs(created_at DESC);
CREATE INDEX idx_job_runs_run ON job_runs(run_id);
CREATE INDEX idx_step_runs_job_run ON step_runs(job_run_id);
CREATE INDEX idx_agents_org_status ON agents(org_id, status);
CREATE INDEX idx_agents_pool ON agents(pool);
CREATE INDEX idx_agents_tags ON agents USING gin(tags);
CREATE INDEX idx_secrets_org_scope ON secrets(org_id, scope);
CREATE INDEX idx_variables_org_scope ON variables(org_id, scope);
CREATE INDEX idx_artifacts_run ON artifacts(run_id);
```

**Auto-update trigger:**

```sql
CREATE OR REPLACE FUNCTION set_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Applied to all tables with updated_at columns
```

### 3.3 `met-store` Crate

Provides:

- **Connection pool factory**: `create_pool(config: &DatabaseConfig) -> PgPool`
- **Migration runner**: Wraps `sqlx::migrate!()` with logging
- **Query modules** (one per entity, initially just the most common): `organizations`, `projects`, `pipelines`, `runs`, `agents`
- **Repository trait pattern**: Each module exposes a struct implementing a trait, making it testable with mocks

```rust
pub struct RunRepository<'a> {
    pool: &'a PgPool,
}

impl<'a> RunRepository<'a> {
    pub async fn create(&self, pipeline_id: PipelineId, trigger_id: Option<TriggerId>) -> Result<Run> { ... }
    pub async fn get(&self, id: RunId) -> Result<Run> { ... }
    pub async fn list_by_pipeline(&self, pipeline_id: PipelineId, limit: i64, offset: i64) -> Result<Vec<Run>> { ... }
    pub async fn update_status(&self, id: RunId, status: RunStatus) -> Result<()> { ... }
}
```

All queries use `sqlx::query_as!` for compile-time checking against the actual schema.

---

## 4. Protobuf Definitions (`proto/`)

### 4.1 File Layout

```
proto/
├── meticulous/
│   ├── agent/
│   │   └── v1/
│   │       ├── agent.proto         # Agent service (registration, heartbeat)
│   │       └── types.proto         # Shared message types for agent domain
│   ├── controller/
│   │   └── v1/
│   │       └── controller.proto    # Controller service (job dispatch, status)
│   └── common/
│       └── v1/
│           └── types.proto         # Cross-cutting types (timestamps, IDs, status enums)
```

### 4.2 Service Definitions

`**common/v1/types.proto**`

```protobuf
syntax = "proto3";
package meticulous.common.v1;

message Uuid { string value = 1; }
message Timestamp { int64 seconds = 1; int32 nanos = 2; }

enum RunStatus {
    RUN_STATUS_UNSPECIFIED = 0;
    RUN_STATUS_PENDING = 1;
    RUN_STATUS_QUEUED = 2;
    RUN_STATUS_RUNNING = 3;
    RUN_STATUS_SUCCEEDED = 4;
    RUN_STATUS_FAILED = 5;
    RUN_STATUS_CANCELLED = 6;
    RUN_STATUS_TIMED_OUT = 7;
}

enum AgentStatus {
    AGENT_STATUS_UNSPECIFIED = 0;
    AGENT_STATUS_ONLINE = 1;
    AGENT_STATUS_OFFLINE = 2;
    AGENT_STATUS_BUSY = 3;
    AGENT_STATUS_DRAINING = 4;
    AGENT_STATUS_DECOMMISSIONED = 5;
}
```

`**agent/v1/agent.proto**` -- Agent-initiated RPCs (egress-only model):

```protobuf
service AgentService {
    // Agent registers itself with the controller
    rpc Register(RegisterRequest) returns (RegisterResponse);

    // Periodic heartbeat -- agent reports health + capacity
    rpc Heartbeat(HeartbeatRequest) returns (HeartbeatResponse);

    // Agent reports job execution status updates
    rpc ReportJobStatus(stream JobStatusUpdate) returns (JobStatusAck);

    // Agent streams log lines to the controller
    rpc StreamLogs(stream LogChunk) returns (LogAck);
}
```

`**controller/v1/controller.proto**` -- Controller-to-agent (via NATS, not direct gRPC):

Job dispatch happens over NATS pub/sub, not gRPC. This proto defines the message shapes serialized into NATS payloads:

```protobuf
message JobDispatch {
    string job_run_id = 1;
    string run_id = 2;
    string pipeline_name = 3;
    string job_name = 4;
    repeated StepSpec steps = 5;
    map<string, string> variables = 6;
    repeated EncryptedSecret secrets = 7;
    string agent_public_key = 8;  // For per-job PKI
    int32 timeout_secs = 9;
}

message StepSpec {
    string id = 1;
    string name = 2;
    string kind = 3;
    string command = 4;
    string image = 5;
    map<string, string> environment = 6;
    int32 sequence = 7;
}

message EncryptedSecret {
    string name = 1;
    bytes encrypted_value = 2;
    string sha256 = 3;
}
```

### 4.3 Build Integration

A `build.rs` in a `met-proto` crate (or in `met-core` itself, depending on preference) that runs `tonic-build`:

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(
            &[
                "proto/meticulous/common/v1/types.proto",
                "proto/meticulous/agent/v1/agent.proto",
                "proto/meticulous/agent/v1/types.proto",
                "proto/meticulous/controller/v1/controller.proto",
            ],
            &["proto/"],
        )?;
    Ok(())
}
```

Decision: create a thin `met-proto` crate whose sole job is proto compilation + re-export. This avoids polluting `met-core` with build-time protobuf compilation and keeps compile times down when only types change.

---

## 5. CI Bootstrap (GitHub Actions)

### 5.1 Workflow: `ci.yml`

Triggers: push to `main`, all PRs.

**Jobs:**


| Job          | What it does                                                  | Notes                                         |
| ------------ | ------------------------------------------------------------- | --------------------------------------------- |
| `check`      | `cargo check --workspace`                                     | Fast fail on type errors                      |
| `fmt`        | `cargo fmt --all -- --check`                                  | Enforce formatting                            |
| `clippy`     | `cargo clippy --workspace -- -D warnings`                     | Lint enforcement                              |
| `test`       | `cargo test --workspace`                                      | Unit tests. Uses a Postgres service container |
| `sqlx-check` | `cargo sqlx prepare --check`                                  | Verify offline query cache matches schema     |
| `proto`      | Verify proto files compile with `buf lint` and `buf breaking` | Schema evolution safety                       |


**Services:**

- PostgreSQL 16 (service container, `POSTGRES_DB=meticulous_test`)
- NATS with JetStream (service container, for integration tests)

### 5.2 Workflow: `release.yml` (stub for now)

Triggers: tag push `v`*.

Placeholder that will eventually build release binaries for all target platforms. In Phase 0 it just runs the full test suite on the tagged commit.

### 5.3 `.sqlx/` Offline Mode

For CI environments without a live database, `sqlx` supports an offline cache (`.sqlx/` directory). The `sqlx-check` CI job verifies this cache is up to date. Developers regenerate it with `cargo sqlx prepare --workspace`.

---

## 6. Local Development Environment

### 6.1 `docker-compose.yml`

```yaml
services:
  postgres:
    image: postgres:16-alpine
    environment:
      POSTGRES_DB: meticulous
      POSTGRES_USER: meticulous
      POSTGRES_PASSWORD: meticulous
    ports: ["5432:5432"]
    volumes: [pgdata:/var/lib/postgresql/data]

  nats:
    image: nats:2-alpine
    command: ["--jetstream", "--store_dir=/data"]
    ports: ["4222:4222", "8222:8222"]
    volumes: [natsdata:/data]

  seaweedfs:
    image: chrislusf/seaweedfs:latest
    command: "server -s3 -dir=/data"
    ports: ["8333:8333", "9333:9333"]
    volumes: [seaweeddata:/data]

volumes:
  pgdata:
  natsdata:
  seaweeddata:
```

### 6.2 `justfile` (Task Runner)

```just
db-up:       docker compose up -d postgres
db-migrate:  cargo sqlx migrate run --source crates/met-store/migrations
db-reset:    cargo sqlx database reset --source crates/met-store/migrations
sqlx-prepare: cargo sqlx prepare --workspace
dev:         docker compose up -d && cargo watch -x 'run --bin met-api'
test:        cargo test --workspace
lint:        cargo fmt --all -- --check && cargo clippy --workspace -- -D warnings
proto:       buf lint proto/ && buf generate proto/
```

---

## 7. Cross-Cutting Conventions

### 7.1 Crate Naming

All crates use the `met-` prefix. Binary crates produce binaries named `met-api`, `met-agent`, `met-cli`.

### 7.2 Module Layout Convention

Every crate follows:

```
src/
├── lib.rs        # Public API re-exports
├── error.rs      # Crate-specific errors (if not using met-core's)
├── config.rs     # Crate-specific config subset (if needed)
└── ...           # Domain modules
```

### 7.3 Testing Strategy for Phase 0

- **Unit tests**: Inline `#[cfg(test)]` modules for pure logic (ID generation, config parsing, model serialization).
- **Integration tests**: `tests/` directory in `met-store` for database tests. Use `sqlx::test` with automatic test database creation and migration.
- **Snapshot tests**: `insta` for testing serialization formats of domain models (catches accidental breaking changes in JSON shapes).

### 7.4 Documentation

- Each crate gets a top-level doc comment in `lib.rs` explaining its purpose.
- Public types get `///` doc comments.
- No `README.md` per crate in Phase 0 -- the plan documents serve as the reference.

---

## 8. Dependency Version Pinning Policy

- **Major + minor pinned** in `[workspace.dependencies]` (e.g. `sqlx = "0.8"` not `sqlx = "0"`).
- `Cargo.lock` committed to the repository (this is an application, not a library).
- Dependabot or Renovate configured to open PRs for patch updates weekly.

---

## 9. Risks and Open Items


| Risk                                                             | Mitigation                                                                                                                   |
| ---------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| `sqlx` compile-time checking requires a live DB or offline cache | CI uses both: service container for tests, `sqlx prepare --check` for offline validation                                     |
| Proto schema evolution could break agent compatibility           | `buf breaking` in CI catches breaking changes. Versioned proto packages (`v1`) allow parallel versions                       |
| Workspace compile times as crate count grows                     | Feature-gate heavy deps (e.g. `sqlx` only in `met-store`), use `cargo-nextest` for parallel test execution                   |
| Schema migrations in production                                  | Phase 0 schema is the "first draft" -- will be iterated before any production data exists. Add migration testing to CI early |


---

## 10. Task Breakdown and Ordering

Execution order within Phase 0 (some can be parallelized):

```
[1] Workspace scaffolding (Cargo.toml, all crate stubs)
[2] met-core: error types, Result alias          ─┐
[3] met-core: ID types (macro + all entities)     │ can be parallel
[4] met-core: config loading                      │
[5] met-core: event envelope                     ─┘
[6] met-core: domain models (depends on 2, 3)
[7] Proto definitions + met-proto crate (depends on 1)
[8] Postgres schema migration (depends on 6 for column alignment)
[9] met-store crate (depends on 6, 8)
[10] docker-compose + justfile (independent)
[11] GitHub Actions CI (depends on 1, can iterate)
[12] sqlx offline cache generation (depends on 8, 9)
```

