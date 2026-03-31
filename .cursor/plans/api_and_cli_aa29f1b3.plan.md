---
name: API and CLI
overview: "Phase 4 detailed plan for the Meticulous API server (met-api, Axum) and developer CLI (met-cli, clap): REST endpoints, WebSocket log streaming, OIDC/JWT + API token auth, RBAC, webhook ingestion, CLI command tree, debug mode, and OpenAPI generation."
todos: []
isProject: false
---

# Meticulous -- API and CLI Detailed Plan

Parent: [Master Architecture](master_architecture_4bf1d365.plan.md)

This plan covers **Phase 4** of the Meticulous build: the two user-facing surface areas of the control plane -- the **REST/WebSocket API server** (`met-api`) and the **developer CLI** (`met-cli`). Together they provide programmatic and interactive access to all platform capabilities.

## Dependencies on Prior Phases

- **Phase 0 (Foundation)**: `met-core` types, `met-store` database layer, Postgres schema
- **Phase 1 (Agent System)**: `met-controller` for agent status queries, NATS subjects for job dispatch
- **Phase 2 (Pipeline Engine)**: `met-engine` for triggering runs, DAG resolution, `met-parser` for validation
- **Phase 3 (Security)**: OIDC/JWT token validation, per-job PKI, secrets broker integration

---

## 1. API Server (`met-api`)

### 1.1 Server Foundation

**Framework**: Axum on Tokio, with `tower` middleware layers.

**Startup sequence**:

1. Load configuration (env vars, config file, CLI flags via `met-core::Config`)
2. Initialize `met-store` connection pool (sqlx PgPool)
3. Initialize NATS JetStream client
4. Initialize object store client (`met-objstore`)
5. Build `AppState` (shared via `Arc`) containing all clients and config
6. Register middleware stack
7. Mount route groups
8. Bind listener, serve with graceful shutdown on SIGTERM/SIGINT

**AppState struct**:

```rust
pub struct AppState {
    pub db: PgPool,
    pub nats: async_nats::jetstream::Context,
    pub objstore: Arc<dyn ObjectStore>,
    pub engine: Arc<PipelineEngine>,
    pub secrets: Arc<SecretsBroker>,
    pub config: Arc<ApiConfig>,
    pub jwt_keys: Arc<JwtKeySet>,
}
```

### 1.2 REST API Design

**Versioning**: Path-prefix (`/api/v1/...`). Breaking changes get a new version prefix.

**Pagination**: Cursor-based (keyset on `(created_at, id)`):

```json
{ "data": [...], "next_cursor": "opaque-or-null", "has_more": true }
```

**Error format** (consistent across all endpoints):

```json
{ "error": { "code": "PIPELINE_NOT_FOUND", "message": "...", "request_id": "req_xxxxx" } }
```

#### Resource Endpoints (all prefixed `/api/v1`)

**Organizations**: `GET/POST /orgs`, `GET/PATCH/DELETE /orgs/:org_id`

**Projects**: `GET/POST /orgs/:org_id/projects`, `GET/PATCH/DELETE /projects/:project_id`

**Pipelines**: `GET/POST /projects/:project_id/pipelines`, `GET/PATCH/DELETE /pipelines/:pipeline_id`, `POST /pipelines/:pipeline_id/validate`

**Runs**: `POST /pipelines/:pipeline_id/runs` (trigger), `GET /pipelines/:pipeline_id/runs` (list), `GET /runs/:run_id`, `POST /runs/:run_id/cancel`, `POST /runs/:run_id/retry`, `GET /runs/:run_id/dag`

**Jobs**: `GET /runs/:run_id/jobs`, `GET /jobs/:job_id`, `GET /jobs/:job_id/logs`, `WS /jobs/:job_id/logs/stream`

**Steps**: `GET /jobs/:job_id/steps`, `GET /steps/:step_id`, `GET /steps/:step_id/logs`

**Secrets** (metadata only, never values): `GET/POST /projects/:project_id/secrets`, `PATCH/DELETE /secrets/:secret_id`, `GET/POST /orgs/:org_id/secrets`

**Variables**: `GET/POST /projects/:project_id/variables`, `PATCH/DELETE /variables/:var_id`

**Reusable Workflows**: `GET/POST /workflows/global`, `GET/POST /projects/:project_id/workflows`, `GET /workflows/:workflow_id`, `GET /workflows/:workflow_id/versions`

**Agents**: `GET /agents`, `GET /agents/:agent_id`, `POST /agents/:agent_id/revoke`, `POST/GET /agents/join-tokens`, `DELETE /agents/join-tokens/:token_id`

**Users and Groups**: `GET /orgs/:org_id/users`, `POST /orgs/:org_id/users/invite`, `GET/PATCH /users/me`, `GET/POST /orgs/:org_id/groups`, `PATCH /groups/:group_id`

**API Tokens**: `GET/POST /users/me/tokens`, `DELETE /tokens/:token_id`, `GET/POST /orgs/:org_id/tokens`

**SCM Webhooks**: `POST /webhooks/github`, `POST /webhooks/gitlab`, `POST /webhooks/bitbucket`, `POST /projects/:project_id/scm/setup`

**Artifacts**: `GET /runs/:run_id/artifacts`, `GET /artifacts/:artifact_id`, `GET /runs/:run_id/sbom`, `GET /runs/:run_id/attestation`

### 1.3 Middleware Stack

Layered via `tower::ServiceBuilder`, outermost first:

1. **Request ID** -- UUID v7 `X-Request-Id` header, propagated to all logs
2. **Request logging** -- Structured tracing (method, path, status, duration)
3. **CORS** -- Configurable origins (same-origin prod, permissive dev)
4. **Rate limiting** -- Token bucket per API token/IP, per-route overrides, `429` with `Retry-After`
5. **Authentication** -- Extract/validate credentials, populate `CurrentUser` in request extensions
6. **Authorization** -- RBAC check against the target resource
7. **Compression** -- `tower-http` gzip/zstd for responses > 1KB

### 1.4 WebSocket Log Streaming

**Endpoint**: `GET /api/v1/jobs/:job_id/logs/stream` -- upgrades to WebSocket.

**Protocol**:

- Auth via query param `?token=<jwt>` (WebSocket doesn't support custom headers reliably)
- Server subscribes to NATS subject `logs.job.<job_id>`, relays log frames:

```json
{ "ts": "2026-03-30T12:00:00.123Z", "step_id": "step_abc", "stream": "stdout", "line": "Building...", "seq": 42 }
```

- Client control messages: `set_filter` (by step), `pause`, `resume`
- **Backpressure**: Buffer up to 10,000 messages per connection; beyond that, send `{ "type": "gap", "missed": N }` and the client fetches the gap from the REST endpoint
- **Reconnection**: `?last_seq=N` on reconnect resumes from JetStream sequence N+1
- **SSE fallback**: `GET /api/v1/jobs/:job_id/logs/sse` for environments where WebSocket is blocked

### 1.5 Authentication and Authorization

#### Authentication Methods (checked in order)

1. **Bearer JWT** (`Authorization: Bearer <token>`) -- Short-lived, from OIDC login. Claims: `sub`, `org_id`, `roles[]`, `exp`
2. **API Token** (`Authorization: Token met_<token>`) -- Long-lived, scoped. Stored hashed (Argon2id). Each has a scope (personal, project, org)
3. **Agent mTLS/JWT** -- Agents auth via gRPC to `met-controller`, not the REST API

#### OIDC Login Flow

1. Client hits `GET /api/v1/auth/login?provider=github` -- gets redirect URL
2. User authenticates with IdP
3. IdP redirects to `/api/v1/auth/callback` with authorization code
4. Server exchanges code for ID token, validates claims, upserts user
5. Server issues Meticulous JWT (short-lived access + longer-lived refresh)
6. CLI receives tokens via localhost callback server

Supported providers: GitHub, GitLab, Google, generic OIDC (configured per-org).

#### RBAC

- `platform_admin` (global) -- Full platform control, global workflows, agent management
- `org_admin` (organization) -- Org settings, user/group management, org-level secrets
- `project_admin` (project) -- Project settings, pipelines, project secrets, variables
- `developer` (project) -- Trigger runs, view logs, read pipelines, manage own tokens
- `viewer` (project) -- Read-only access to runs, logs, pipeline definitions

Enforced via Axum extractor:

```rust
async fn cancel_run(
    State(state): State<AppState>,
    Auth(user, Permission::RunCancel): Auth,
    Path(run_id): Path<Uuid>,
) -> Result<Json<Run>, ApiError> { ... }
```

### 1.6 Webhook Ingestion

**GitHub**: Verify `X-Hub-Signature-256`, handle `push`, `pull_request`, `release`, `workflow_dispatch`.

**GitLab**: Verify `X-Gitlab-Token`, handle `push`, `merge_request`, `tag_push`.

**Bitbucket**: Verify HMAC, handle `repo:push`, `pullrequest:created/updated`.

**Common flow**: Validate signature -> parse into normalized `ScmEvent` -> match against pipeline triggers -> enqueue run via `met-engine` -> return `202 Accepted`.

### 1.7 OpenAPI

Auto-generate OpenAPI 3.1 via `utoipa`. Serve at `/api/v1/openapi.json` and Swagger UI at `/api/docs` (dev/staging). CI validates spec stays in sync with handler code.

---

## 2. Developer CLI (`met-cli`)

### 2.1 Design Principles

- **Fast feedback**: Output within 1s or show progress indicator
- **Scriptable**: `--output json` on all commands for machine consumption
- **Minimal config**: Works with zero config for common cases
- **Offline-capable where possible**: Pipeline validation and YAML linting work without server

### 2.2 Command Structure

```
met
├── auth
│   ├── login              # OIDC browser login
│   ├── logout             # Clear stored credentials
│   ├── status             # Current auth state
│   └── token {create,list,revoke}
├── org {list,switch,info}
├── project {list,create,info,switch}
├── pipeline {list,show,validate,trigger,diff}
├── run
│   ├── list               # Filterable by status, pipeline, date
│   ├── status             # DAG progress view
│   ├── logs               # Stream or fetch (--follow, --job, --step, --tail)
│   ├── cancel / retry
│   └── artifacts          # List/download
├── secret {list,set,delete}
├── variable {list,set,delete}
├── workflow {list,show,versions}
├── agent
│   ├── list / info / revoke
│   └── join-token {create,list,revoke}
├── debug
│   ├── run                # Local pipeline execution
│   ├── shell              # Interactive shell in job environment
│   └── replay             # Replay failed run locally
└── config {show,set,init}
```

### 2.3 Configuration

**Config file** (`~/.config/meticulous/config.toml`):

```toml
[server]
url = "https://meticulous.example.com"

[context]
org = "my-org"
project = "my-project"

[output]
format = "text"   # text | json | yaml
color = "auto"    # auto | always | never
```

**Credential storage**: OS keyring via `keyring` crate (macOS/Linux/Windows). Falls back to encrypted file if no keyring available.

**Context resolution** (highest to lowest priority):

1. CLI flags (`--org`, `--project`, `--server`)
2. Env vars (`MET_ORG`, `MET_PROJECT`, `MET_SERVER_URL`)
3. Project-local `.meticulous.toml`
4. Global config `~/.config/meticulous/config.toml`

### 2.4 CLI Auth Flow

1. `met auth login` starts a temp HTTP server on random localhost port
2. Opens browser to `<server>/api/v1/auth/login?provider=github&redirect_uri=http://localhost:<port>/callback`
3. User authenticates with IdP
4. Server redirects back to `localhost:<port>/callback?code=<auth_code>`
5. CLI exchanges code for access + refresh tokens, stores in OS keyring
6. Transparent token refresh before each request; re-auth prompt if refresh token expired

**Non-interactive**: `met auth login --token met_<api_token>` for CI/headless environments.

### 2.5 Log Streaming

```bash
met run logs <run_id> --follow                          # All jobs interleaved
met run logs <run_id> --job <name> --follow             # Specific job
met run logs <run_id> --job <name> --step <name> --follow  # Specific step
met run logs <run_id> --tail 100                        # Last N lines
```

- WebSocket to `/api/v1/jobs/:job_id/logs/stream`
- Multi-job: one WebSocket per job, merged by timestamp
- Colorized by job name, step boundaries shown with separators
- `--output json` emits one JSON object per line for piping

### 2.6 Debug Mode

Addresses the open question: *"Can we have a reasonably secure debug CLI where developers could have a good user experience WITHOUT the ability to easily exfiltrate secrets?"*

#### `met debug run`

Local pipeline execution in containers on the developer's machine:

```bash
met debug run                                    # .stable/ definitions
met debug run --file .stable/build.yaml          # Specific file
met debug run --set IMAGE_TAG=dev-test           # Override variables
met debug run --dry                              # Parse/resolve DAG only
```

**How it works**: Parse YAML locally (`met-parser`) -> resolve DAG, fetch reusable workflow defs from server -> execute steps in containers -> output logs in real time.

**Secret handling**: Secrets are **never** downloaded to the developer's machine in plaintext:

1. CLI creates a **debug session** (`POST /api/v1/debug/sessions`)
2. Server provisions a short-lived (15min), single-use secrets proxy endpoint
3. Container env gets `MET_SECRETS_PROXY=https://server/api/v1/debug/sessions/<sid>/secrets` + single-use token
4. Steps call the proxy at runtime; proxy returns decrypted values over TLS, logged server-side for audit
5. Proxy auto-expires after session ends; requires `debug` permission on the project

#### `met debug shell`

Interactive shell in the execution environment of a specific step (same container image, env vars, proxied secrets):

```bash
met debug shell --pipeline build --job compile --step build-binary
```

#### `met debug replay`

Re-run a failed pipeline run locally with the same inputs, optionally with overrides:

```bash
met debug replay <run_id>
met debug replay <run_id> --set TIMEOUT=120
```

Server provides the original run's resolved inputs so the failure can be reproduced exactly.

### 2.7 Additional CLI Features

- **Progress display**: `indicatif` spinners/progress bars for long operations
- **Shell completions**: `met completions <shell>` for bash, zsh, fish, PowerShell
- **Version check**: `met version` shows version and checks for updates (`MET_NO_UPDATE_CHECK=1` to opt out)
- **Request debugging**: `--verbose` prints underlying HTTP request/response

---

## 3. Crate Architecture

### 3.1 `met-api` Internal Structure

```
crates/met-api/src/
├── main.rs               # Binary entrypoint, server startup
├── lib.rs                # Library root (for testing)
├── config.rs             # API-specific config
├── state.rs              # AppState definition
├── error.rs              # ApiError type, JSON error responses
├── extractors/
│   ├── auth.rs           # Auth extractor (JWT + API token)
│   ├── pagination.rs     # Cursor pagination extractor
│   └── request_id.rs     # Request ID extractor
├── middleware/
│   ├── rate_limit.rs     # Token bucket rate limiter
│   ├── cors.rs           # CORS config
│   └── logging.rs        # Request/response logging
├── routes/
│   ├── mod.rs            # Router assembly
│   ├── orgs.rs, projects.rs, pipelines.rs, runs.rs, jobs.rs
│   ├── secrets.rs, variables.rs, workflows.rs, agents.rs
│   ├── users.rs, auth.rs, tokens.rs, webhooks.rs
│   ├── artifacts.rs, debug.rs
└── ws/
    ├── logs.rs           # WebSocket log streaming
    └── protocol.rs       # WS message types
```

### 3.2 `met-cli` Internal Structure

```
crates/met-cli/src/
├── main.rs               # Binary entrypoint
├── cli.rs                # Top-level clap App definition
├── config.rs             # Config file loading/merging
├── context.rs            # Org/project context resolution
├── client.rs             # HTTP client wrapper (auth, retries)
├── auth/
│   ├── login.rs          # OIDC browser login flow
│   ├── keyring.rs        # OS keyring storage
│   └── refresh.rs        # Token refresh logic
├── commands/
│   ├── auth.rs, org.rs, project.rs, pipeline.rs, run.rs
│   ├── secret.rs, variable.rs, workflow.rs, agent.rs
│   ├── debug.rs, config_cmd.rs
├── output/
│   ├── table.rs          # Human-readable tables
│   ├── json.rs           # JSON output
│   └── color.rs          # Terminal color utilities
└── ws/
    └── log_stream.rs     # WebSocket log stream client
```

### 3.3 Key Dependencies

`**met-api**`: `axum` + `axum-extra`, `tower` + `tower-http`, `sqlx` (via `met-store`), `jsonwebtoken`, `utoipa`, `async-nats`, `serde`/`serde_json`, `uuid`, `tracing`

`**met-cli**`: `clap` + `clap_complete`, `reqwest` (rustls), `tokio-tungstenite`, `keyring`, `indicatif`, `comfy-table`, `colored`, `toml`, `open`, `dialoguer`

---

## 4. Database Schema Additions

```sql
-- API tokens (personal and org-level service tokens)
CREATE TABLE api_tokens (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    prefix      VARCHAR(8) NOT NULL UNIQUE,
    hash        BYTEA NOT NULL,               -- Argon2id
    name        VARCHAR(255) NOT NULL,
    scope_type  VARCHAR(20) NOT NULL,         -- 'personal', 'project', 'org'
    scope_id    UUID,
    user_id     UUID NOT NULL REFERENCES users(id),
    permissions TEXT[] NOT NULL DEFAULT '{}',
    expires_at  TIMESTAMPTZ,
    last_used   TIMESTAMPTZ,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    revoked_at  TIMESTAMPTZ
);
CREATE INDEX idx_api_tokens_prefix ON api_tokens(prefix);
CREATE INDEX idx_api_tokens_user ON api_tokens(user_id) WHERE revoked_at IS NULL;

-- OIDC provider configurations (per-org)
CREATE TABLE oidc_providers (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id          UUID NOT NULL REFERENCES organizations(id),
    provider_type   VARCHAR(50) NOT NULL,
    client_id       VARCHAR(255) NOT NULL,
    client_secret   BYTEA NOT NULL,           -- Encrypted at rest
    issuer_url      VARCHAR(512),
    enabled         BOOLEAN NOT NULL DEFAULT true,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(org_id, provider_type)
);

-- User sessions (refresh tokens)
CREATE TABLE user_sessions (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id         UUID NOT NULL REFERENCES users(id),
    refresh_hash    BYTEA NOT NULL,
    user_agent      VARCHAR(512),
    ip_address      INET,
    expires_at      TIMESTAMPTZ NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    revoked_at      TIMESTAMPTZ
);
CREATE INDEX idx_sessions_user ON user_sessions(user_id) WHERE revoked_at IS NULL;

-- SCM webhook registrations
CREATE TABLE webhook_registrations (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id      UUID NOT NULL REFERENCES projects(id),
    provider        VARCHAR(50) NOT NULL,
    external_id     VARCHAR(255),
    secret_hash     BYTEA NOT NULL,
    events          TEXT[] NOT NULL,
    active          BOOLEAN NOT NULL DEFAULT true,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Debug sessions
CREATE TABLE debug_sessions (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id         UUID NOT NULL REFERENCES users(id),
    project_id      UUID NOT NULL REFERENCES projects(id),
    pipeline_id     UUID REFERENCES pipelines(id),
    token_hash      BYTEA NOT NULL,
    expires_at      TIMESTAMPTZ NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    closed_at       TIMESTAMPTZ
);

-- Rate limit tracking
CREATE TABLE rate_limit_counters (
    key             VARCHAR(255) NOT NULL,
    window_start    TIMESTAMPTZ NOT NULL,
    count           INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (key, window_start)
);
```

---

## 5. Build Order (Within Phase 4)

```
4a  API server skeleton (Axum boilerplate, AppState, middleware stack, ApiError, pagination, health check, test harness)
4b  Auth and RBAC (JWT validation, API token auth, Auth extractor, RBAC resolution, OIDC flow, token refresh, DB migration)
4c  Core resource endpoints (Org, Project, Pipeline, Run, Job, Step, Variable, Secret, Workflow CRUD)
4d  WebSocket log streaming (WS upgrade, NATS subscription, backpressure, reconnection, SSE fallback)
4e  Webhook ingestion (GitHub/GitLab/Bitbucket receivers, ScmEvent normalization, trigger matching, GitHub App setup)
4f  Agent and token management (agent list/detail/revoke, join token CRUD, API token CRUD)
4g  CLI foundation (clap tree, config loading, context resolution, HTTP client, auth login, keyring, output formatting, completions)
4h  CLI core commands (org, project, pipeline, run, secret, variable, agent commands + log streaming)
4i  Debug mode (met debug run/shell/replay, debug session API, secrets proxy, audit logging)
4j  Polish and OpenAPI (utoipa annotations, Swagger UI, CI spec validation, rate limiting, size limits, versioning)
```

---

## 6. Open Questions

- **Rate limit storage**: Postgres sliding window (simple) vs Redis (fast, adds dependency). Leaning Postgres for now.
- **API token format**: `met_` prefix + random bytes (like GitHub's `ghp_` tokens) -- identifiable and greppable.
- **WebSocket scaling**: Multiple API replicas each subscribe to NATS independently (JetStream handles this), but client reconnect must be seamless.
- **Debug mode container runtime**: `bollard` (Docker API) vs shelling out to `docker`/`podman` CLI for simplicity + Podman support.
- **CLI plugin system**: Defer to post-MVP, but design command tree to not preclude `met <plugin> <command>`.

