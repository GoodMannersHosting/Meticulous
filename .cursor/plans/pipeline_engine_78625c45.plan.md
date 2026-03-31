---
name: Pipeline Engine
overview: "Detailed plan for the Meticulous pipeline engine: YAML parser with source-span errors, pipeline IR, reusable workflow resolution with semver versioning, DAG execution with concurrent branch scheduling, NATS-based job dispatch, multi-layer caching, artifact passing, conditional execution (CEL), retry/timeout, and an event broadcast system."
todos:
  - id: parser-source-spans
    content: Add source span tracking to YAML parser using serde_yaml's Location API for line-numbered error messages
    status: completed
  - id: parser-workflow-fetch-db
    content: Implement DatabaseWorkflowProvider to fetch global workflows from PostgreSQL (requires met-store integration)
    status: completed
  - id: parser-workflow-fetch-git
    content: Implement GitWorkflowProvider to fetch project workflows from .stable/workflows/ in git repos
    status: completed
  - id: parser-semver-resolution
    content: Implement semver version resolution for workflow references (parse version constraints, find best match)
    status: completed
  - id: parser-hash-files
    content: Implement hashFiles() helper function for cache key templates (SHA-256 of glob-matched file contents)
    status: completed
  - id: parser-tests
    content: "Add comprehensive parser tests: edge cases, error messages, workflow resolution, variable interpolation"
    status: completed
  - id: engine-db-persistence
    content: "Persist pipeline runs to database: create runs/job_runs/step_runs records, update status transitions"
    status: completed
  - id: engine-retry-logic
    content: "Implement retry policy execution: exponential backoff, max attempts, re-queue failed jobs"
    status: completed
  - id: engine-artifact-upload
    content: Implement artifact upload to object storage after job completion (tarball + metadata)
    status: completed
  - id: engine-artifact-download
    content: Include presigned artifact download URLs in JobPayload for dependent jobs
    status: completed
  - id: engine-cache-objstore
    content: "Complete ObjectStoreCache implementation: S3 upload/download, zstd compression, metadata tracking"
    status: completed
  - id: engine-cache-eviction
    content: "Implement cache eviction: LRU by last_hit_at, per-project storage quotas, garbage collection"
    status: completed
  - id: engine-secret-encryption
    content: Integrate with met-secrets for per-job secret encryption (PKI handshake with agent pubkey)
    status: completed
  - id: engine-completion-listener
    content: Wire up CompletionListener to receive agent job completions and update run state
    status: completed
  - id: engine-log-streaming
    content: "Implement log streaming relay: receive from gRPC, store in object storage, emit WebSocket events"
    status: completed
  - id: db-pipeline-tables
    content: Write migration for pipeline execution tables (pipeline_runs, job_runs, step_runs, cache_entries, artifacts)
    status: completed
  - id: db-run-queries
    content: "Implement met-store queries: create_run, update_run_status, list_runs_by_pipeline, get_run_with_jobs"
    status: completed
  - id: proto-job-payload
    content: Define JobPayload protobuf (steps, secrets, cache, artifacts, pool selector) in proto/job.proto
    status: completed
  - id: proto-completion
    content: Define JobCompletion protobuf (status, outputs, execution metadata) for agent->controller reporting
    status: completed
  - id: api-trigger-run
    content: Implement POST /api/pipelines/{id}/runs endpoint to trigger pipeline execution
    status: completed
  - id: api-run-status
    content: Implement GET /api/runs/{id} endpoint with job/step status and log URLs
    status: completed
  - id: api-run-cancel
    content: Implement POST /api/runs/{id}/cancel endpoint to request run cancellation
    status: completed
  - id: api-websocket-events
    content: Expose run/job/step events over WebSocket for real-time UI updates
    status: completed
  - id: test-parser-e2e
    content: "End-to-end parser test: parse example pipeline YAML, verify IR structure"
    status: completed
  - id: test-engine-simple
    content: "Integration test: execute simple pipeline with single job (mock agent completion)"
    status: completed
  - id: test-engine-dag
    content: "Integration test: execute diamond DAG, verify correct execution order and concurrency"
    status: completed
  - id: test-engine-cache
    content: "Integration test: verify cache hit skips job, cache miss executes job"
    status: completed
  - id: test-engine-retry
    content: "Integration test: job fails, retry executes, succeeds on retry"
    status: completed
  - id: test-engine-cancel
    content: "Integration test: cancel mid-run, verify jobs marked cancelled"
    status: completed
isProject: false
---

# Meticulous -- Pipeline Engine Plan

**Phase 2 of the Meticulous build order.** This plan depends on Phase 0 (Foundation) for `met-core` types, `met-store` database layer, and protobuf tooling, and on Phase 1 (Agent System) for `met-agent`, `met-controller`, and NATS infrastructure.

**Crates**: `met-parser`, `met-engine`
**Upstream crate dependencies**: `met-core`, `met-store`, `met-agent`, `met-controller`, `met-secrets`, `met-objstore`

---

## 1. Overview

The pipeline engine owns the full lifecycle from "YAML on disk" to "jobs running on agents." It encompasses:

- **Parsing** pipeline definitions (YAML primary; TypeScript and Python deferred)
- **Resolving** reusable workflows at two scopes (global, project)
- **Building and validating** the execution DAG
- **Scheduling** jobs onto agents via NATS pub/sub
- **Orchestrating** step execution, variable/secret injection, caching, and artifact passing
- **Tracking** run state through the database and emitting events for the API/UI layer

The engine runs server-side in the control plane. Agents are dumb executors -- they receive a fully-resolved job payload and run steps in sequence, reporting status back via gRPC. The engine reacts to completion events and advances the DAG.

---

## 2. Pipeline Definition Format

### 2.1 File Convention

Pipeline definitions live in `.stable/` at the repository root (configurable per-project). Each `.yaml` file in that directory is a pipeline. The existing example in [design/notes/pipelines.md](design/notes/pipelines.md) serves as the reference.

### 2.2 YAML Schema

```yaml
name: string                       # Human-readable pipeline name
triggers:
  manual: {}                       # Always available
  webhook:                         # SCM push/PR events
    events: [push, pull_request]
    branches: [main, "release/*"]
  tag:
    patterns: ["v*", "semver bump"]
  schedule:
    cron: "0 2 * * 1-5"           # UTC cron expression

runs-on:                           # Agent pool selector
  tags:
    - amd64: true
    - gpu: false

secrets:                           # Secret references (never inline values)
  SECRET_NAME:
    aws:
      arn: string
    vault:
      path: string
      key: string
    builtin:
      name: string                 # Discouraged; UX warns

vars:                              # Plain-text variables
  VAR_NAME: value

workflows:                         # Ordered list of workflow invocations
  - name: string                   # Display name
    id: string                     # Unique within this pipeline, used for depends-on
    workflow: string               # "global/<name>" or "project/<name>"
    version: string                # Semver or tag (resolved at parse time)
    inputs:                        # Key-value, supports ${VAR} and ${SECRET} interpolation
      key: value
    depends-on: [id, ...]          # DAG edges
    condition: string              # Optional: CEL expression for conditional execution
    timeout: duration              # Optional: max wall-clock time (default from global config)
    retry:                         # Optional: retry policy
      max_attempts: int
      backoff: duration
    cache:                         # Optional: per-workflow cache config
      key: string                  # Template, e.g. "${workflow}-${hashFiles('**/Cargo.lock')}"
      paths: [string]              # Paths to cache
      restore-keys: [string]       # Fallback keys for partial match
```

### 2.3 Reusable Workflow Definition

Reusable workflows are YAML files stored at two scopes:

- **Global**: managed by platform admins, stored in the database / a dedicated git repo, referenced as `global/<name>`
- **Project**: stored in the project repo under `.stable/workflows/`, referenced as `project/<name>`

```yaml
name: string
description: string
version: string                    # Semver

inputs:                            # Declared inputs with types and defaults
  input_name:
    type: string | int | bool | secret
    required: bool
    default: value

outputs:                           # Values exported to downstream dependents
  output_name:
    value: string                  # Expression referencing step outputs

jobs:                              # One or more jobs (most workflows have one)
  - name: string
    id: string
    runs-on:                       # Can override pipeline-level pool selector
      tags: [...]
    steps:
      - name: string
        id: string
        run: string                # Shell command (bash/powershell)
        shell: bash | powershell | python
        env:                       # Step-scoped env vars
          KEY: value
        working-directory: string
        timeout: duration
        continue-on-error: bool
      - name: string
        uses: action/name@version  # Built-in action (git-clone, docker-build, etc.)
        with:
          key: value
    services:                      # Sidecar containers for the job
      - name: string
        image: string
        ports: [int]
        env: {}
```

### 2.4 Variable Interpolation

Interpolation uses `${...}` syntax. Resolution order (last wins):

1. Built-in context variables (`MET_RUN_ID`, `MET_PIPELINE_NAME`, `MET_COMMIT_SHA`, etc.)
2. Pipeline-level `vars`
3. Workflow `inputs`
4. Step `env`
5. Secret references (resolved at execution time, never interpolated during YAML parsing)

Secrets are **never** embedded in the resolved job payload. The payload contains secret *references* that the secrets broker resolves and encrypts per-job using the agent's ephemeral public key (see Security plan and [design/notes/agents.md](design/notes/agents.md) PKI process).

---

## 3. Crate Design: `met-parser`

### 3.1 Responsibilities

- Deserialize `.yaml` pipeline files into strongly-typed Rust structs
- Validate schema: required fields, type correctness, no unknown keys
- Resolve reusable workflow references (fetch global workflows from DB, project workflows from repo)
- Flatten workflows into a fully-resolved pipeline IR (intermediate representation)
- Validate the DAG: cycle detection, missing dependency references, unreachable nodes
- Validate variable/secret references: all `${...}` tokens resolve to declared vars/secrets/inputs/context
- Produce detailed, line-numbered error messages for user-facing feedback

### 3.2 Internal Pipeline IR

The parser produces an IR that the engine consumes. The IR is fully resolved -- no external references remain.

```rust
pub struct PipelineIR {
    pub name: String,
    pub triggers: Vec<Trigger>,
    pub variables: HashMap<String, String>,
    pub secret_refs: HashMap<String, SecretRef>,
    pub jobs: Vec<JobIR>,
}

pub struct JobIR {
    pub id: JobId,
    pub name: String,
    pub depends_on: Vec<JobId>,
    pub pool_selector: PoolSelector,
    pub steps: Vec<StepIR>,
    pub services: Vec<ServiceDef>,
    pub timeout: Duration,
    pub retry_policy: Option<RetryPolicy>,
    pub cache_config: Option<CacheConfig>,
    pub condition: Option<String>,       // CEL expression
    pub source_workflow: WorkflowRef,    // Traceability back to the reusable workflow
}

pub struct StepIR {
    pub id: StepId,
    pub name: String,
    pub command: StepCommand,            // Run(shell, script) | Action(name, version, inputs)
    pub env: HashMap<String, EnvValue>,  // EnvValue: Literal(String) | SecretRef(name) | Expr(String)
    pub working_directory: Option<String>,
    pub timeout: Duration,
    pub continue_on_error: bool,
}
```

### 3.3 Parser Pipeline (6 Stages)

```
Raw YAML text
  |
  v
[Stage 1: Deserialize]        serde_yaml into raw AST structs (permissive, captures source spans)
  |
  v
[Stage 2: Schema Validate]    check required fields, types, unknown keys; collect errors
  |
  v
[Stage 3: Workflow Resolution] fetch referenced workflows, inline their jobs/steps
  |                            (recursive: workflows can nest, depth-limited to 5)
  v
[Stage 4: Variable Resolution] resolve ${...} tokens, flag unresolved references
  |
  v
[Stage 5: DAG Construction]   build adjacency list from depends-on, validate acyclicity (Kahn's algo)
  |
  v
[Stage 6: Emit IR]            produce PipelineIR or Vec<ParseError> with source spans
```

### 3.4 Error Reporting

Errors carry source location (file, line, column) and a human-readable message. Multiple errors are collected per stage rather than failing on the first. Format designed to render well in both CLI and web UI.

```rust
pub struct ParseError {
    pub severity: Severity,       // Error | Warning | Info
    pub message: String,
    pub source: SourceLocation,   // file, line, col, span
    pub hint: Option<String>,     // Suggestion for fixing
    pub code: ErrorCode,          // Machine-readable error code (e.g. E1001)
}
```

### 3.5 Future: TypeScript and Python Parsers

Phase 2 is YAML-only. TS/Python support added later via:

- A well-defined JSON schema that TS/Python SDKs generate against
- TS/Python programs output JSON conforming to the same schema
- `met-parser` has a JSON ingestion path that feeds into Stage 2+ of the same pipeline
- The IR is the stable contract; the front-end language is interchangeable

---

## 4. Crate Design: `met-engine`

### 4.1 Responsibilities

- Accept a `PipelineIR` and create a **pipeline run**
- Manage run lifecycle: `Pending -> Running -> Succeeded / Failed / Cancelled`
- Execute the DAG: schedule ready jobs, react to completions, propagate failures
- Interface with the scheduler to dispatch jobs to agents via NATS
- Handle cancellation, timeouts, retries
- Manage caching (lookup and store)
- Coordinate artifact passing between jobs
- Persist all state transitions to the database via `met-store`
- Emit events (via a broadcast channel) for the API layer to fan out over WebSocket

### 4.2 Core State Machines

**Pipeline Run:**

- `Pending -> Running -> Succeeded`
- `Running -> Failed`
- `Running -> Cancelled`

**Job Run:**

- `Pending -> Waiting` (depends-on not yet met)
- `Waiting -> Queued` (dispatched to NATS)
- `Queued -> Running` (agent picked up)
- `Running -> Succeeded`
- `Running -> Failed -> Retrying -> Queued` (if retries remain)
- `Running -> Cancelled`
- `Running -> TimedOut`
- `Pending -> Skipped` (upstream failed, transitive skip)

**Step Run:**

- `Pending -> Running -> Succeeded`
- `Running -> Failed` (exit code != 0)
- `Pending -> Skipped` (continue-on-error from prior step, or job skipped)
- `Running -> TimedOut`

### 4.3 DAG Executor

The DAG executor is the core loop -- an async Tokio task that:

1. Initializes all jobs as `Pending`
2. Evaluates which jobs have all dependencies satisfied (or no dependencies)
3. For satisfied jobs, evaluates the `condition` (CEL) if present; skip if false
4. Checks the cache for cache-eligible jobs; if hit, marks as `Succeeded` with cached outputs
5. Dispatches ready jobs to the scheduler
6. Waits for job completion events (via a channel from the controller)
7. On completion: updates state, marks downstream jobs as ready, loops
8. On failure: depending on pipeline failure strategy (fail-fast vs. continue), either cancels remaining jobs or continues independent branches
9. On cancellation: sends cancel signals to all running jobs, waits for acknowledgment

```rust
pub struct DagExecutor {
    run_id: RunId,
    ir: PipelineIR,
    job_states: HashMap<JobId, JobRunState>,
    scheduler: Arc<dyn JobScheduler>,
    cache: Arc<dyn CacheService>,
    store: Arc<dyn RunStore>,
    event_tx: broadcast::Sender<RunEvent>,
}

impl DagExecutor {
    pub async fn execute(&mut self) -> RunOutcome { ... }
}
```

**Concurrency**: Independent DAG branches execute concurrently. The executor does not serialize jobs that have no dependency relationship.

**Failure propagation**: When a job fails, all transitive dependents are marked `Skipped` (not `Failed`) to distinguish "did not run" from "ran and failed."

### 4.4 Job Scheduler

The scheduler bridges the engine and the agent layer:

1. Receives a `JobIR` + run context from the DAG executor
2. Builds a `JobPayload` protobuf message (step definitions, secret refs, cache/artifact instructions, pool selector)
3. Requests secret encryption from the secrets broker (per-job PKI)
4. Publishes the payload to the appropriate NATS subject (`met.jobs.<tenant_id>.<pool_tag>`)
5. Records the dispatch in the database with a timeout deadline
6. Monitors for agent acknowledgment; if no ack within threshold, re-publishes or marks failed

```rust
#[async_trait]
pub trait JobScheduler: Send + Sync {
    async fn dispatch(&self, job: &JobIR, run_ctx: &RunContext) -> Result<DispatchReceipt>;
    async fn cancel(&self, job_id: &JobId, run_id: &RunId) -> Result<()>;
}
```

Default implementation (`NatsJobScheduler`) uses JetStream with:

- **Subject**: `met.jobs.<tenant_id>.<pool_hash>` where `pool_hash` is derived from the sorted tag set
- **Consumer**: Each agent pool has a pull-based consumer, ensuring exactly-once delivery
- **Ack policy**: Explicit ack. Agent doesn't ack within `ack_wait` -> NATS redelivers (bounded by `max_deliver`)

### 4.5 Job Payload (Protobuf)

Defined in `proto/job.proto`, the payload is what the agent receives:

```protobuf
message JobPayload {
  string run_id = 1;
  string job_id = 2;
  string pipeline_name = 3;
  repeated StepPayload steps = 4;
  map<string, EncryptedSecret> secrets = 5;
  map<string, string> variables = 6;
  CacheRestore cache_restore = 7;
  repeated ArtifactRef input_artifacts = 8;
  PoolSelector pool_selector = 9;
  google.protobuf.Duration timeout = 10;
  RetryPolicy retry_policy = 11;
  repeated ServiceDef services = 12;
}

message StepPayload {
  string step_id = 1;
  string name = 2;
  oneof command {
    ShellCommand shell = 3;
    ActionRef action = 4;
  }
  map<string, string> env = 5;
  string working_directory = 6;
  google.protobuf.Duration timeout = 7;
  bool continue_on_error = 8;
}
```

### 4.6 Execution Contract (Agent Side)

While `met-agent` owns this code, the engine defines the contract:

1. Agent receives `JobPayload` from NATS
2. Sends ack to NATS + gRPC `JobStarted` to controller
3. Generates ephemeral X509 keypair, sends pubkey to controller via `ExchangeJobKeys`
4. Receives encrypted secrets, decrypts and verifies SHA-256 checksums
5. Restores cache (if `cache_restore` present) from object storage
6. Starts sidecar services (if any)
7. Executes steps sequentially:
  - `ShellCommand`: spawn process with env vars, stream stdout/stderr to controller via gRPC
  - `ActionRef`: invoke built-in action handler
  - Capture exit code, timing, binary SHAs (for supply chain tracking per [design/notes/security.md](design/notes/security.md))
8. On completion: upload artifacts to object storage, upload cache (if changed), send `JobCompleted`
9. Controller relays `JobCompleted` to the engine via internal channel

### 4.7 Caching

**Cache key model**: Content-addressed strings built from user-defined templates. Helper functions:

- `hashFiles(glob)` -- SHA-256 of sorted, concatenated contents of matching files
- `hashEnv(var)` -- SHA-256 of an env var value
- Literal strings and variable interpolation

Example key: `docker-build-${hashFiles('**/Cargo.lock')}-${MET_PIPELINE_NAME}`

**Cache storage** via `met-objstore` (S3-compatible):

- Path: `caches/<project_id>/<cache_key>.tar.zst` (zstd compressed tarball)
- Metadata in Postgres: key, size, created_at, last_hit_at, hit_count, ttl
- Eviction: LRU by `last_hit_at`, bounded by per-project storage quota

**Cache lifecycle**:

1. **Restore**: Engine checks if `cache_key` exists. If so, includes a presigned download URL in `JobPayload.cache_restore`. Agent downloads and extracts.
2. **Save**: After successful job, agent tarballs cache paths, uploads to object storage, reports key + size. Engine updates cache metadata.
3. **Fallback keys**: If primary key misses, `restore-keys` are tried in order (prefix match) for partial cache hits.

**Multi-layer caching** (per [design/notes/features.md](design/notes/features.md)):

- **Layer 1 -- Agent-local**: LRU disk cache. Best-effort, no network round-trip.
- **Layer 2 -- Object storage**: Durable, shared across all agents in a pool.

**Cache immutability**: Once written, a cache entry for a given key is immutable. If inputs change, the key changes. Prevents cache poisoning.

### 4.8 Artifact Passing

Jobs produce artifacts that downstream jobs consume.

**Upload**: After job completion, agent uploads declared artifacts to object storage:

- Path: `artifacts/<run_id>/<job_id>/<artifact_name>.tar.zst`
- Controller records metadata (path, size, SHA-256) in database

**Download**: Downstream job's `JobPayload` includes `ArtifactRef` entries with presigned download URLs. Agent downloads and extracts before step execution.

**Retention**: Per project retention policy (default 30 days). Pinned/tagged run artifacts retained indefinitely. Garbage collection runs on a schedule.

### 4.9 Conditional Execution

The `condition` field supports CEL (Common Expression Language) expressions. CEL environment includes:

- `vars` -- all pipeline variables
- `trigger` -- trigger metadata (event type, branch, tag, etc.)
- `jobs` -- map of completed job IDs to their outcome/outputs
- `env` -- built-in context variables

Examples:

- `trigger.branch == 'main'`
- `jobs.build.outcome == 'success' && vars.DEPLOY_TARGET != ''`
- `trigger.event == 'tag' && trigger.tag matches 'v[0-9]+.*'`

Evaluated server-side by the DAG executor before dispatching each job.

### 4.10 Timeout and Retry

**Timeouts**: Every job and step has a timeout (explicit or inherited from global/project defaults). The engine starts a timer when dispatching a job:

- If the agent doesn't report completion within the timeout, the job is marked `TimedOut`
- The agent also enforces timeouts locally and will kill long-running steps
- Double-enforcement ensures timeouts are respected even if agent-controller communication is delayed

**Retries**: Jobs can specify a retry policy:

```rust
pub struct RetryPolicy {
    pub max_attempts: u32,      // Including the initial attempt
    pub backoff: Duration,      // Initial backoff duration
    pub backoff_multiplier: f64, // Exponential backoff factor (default 2.0)
    pub max_backoff: Duration,  // Cap on backoff duration
}
```

When a job fails and has retries remaining:

1. Engine increments the attempt counter
2. Computes backoff: `min(backoff * multiplier^attempt, max_backoff)`
3. Waits for backoff duration
4. Re-dispatches the job to NATS
5. Agent receives as a new job (unaware of retry status)

Retry state is tracked in the database (`job_runs.attempt` column) and surfaced in the UI.

### 4.11 Event Broadcasting

The engine emits events for all state transitions, enabling real-time UI updates and external integrations:

```rust
pub enum RunEvent {
    RunQueued { run_id: RunId, pipeline_id: PipelineId, triggered_by: String },
    RunStarted { run_id: RunId, pipeline_id: PipelineId },
    RunCompleted { run_id: RunId, pipeline_id: PipelineId, success: bool, duration_ms: u64 },
    RunCancelled { run_id: RunId, pipeline_id: PipelineId },
    JobQueued { job_run_id: JobRunId, run_id: RunId, job_name: String },
    JobStarted { job_run_id: JobRunId, run_id: RunId, agent_id: AgentId },
    JobCompleted { job_run_id: JobRunId, run_id: RunId, success: bool, duration_ms: u64 },
    JobCancelled { job_run_id: JobRunId, run_id: RunId },
    StepStarted { step_run_id: StepRunId, job_run_id: JobRunId, step_name: String },
    StepCompleted { step_run_id: StepRunId, job_run_id: JobRunId, exit_code: i32 },
    LogChunk { job_run_id: JobRunId, step_run_id: StepRunId, chunk: String },
}
```

Events are published to NATS subjects for fan-out:

- `met.events.<org_id>.runs.<run_id>` -- all events for a specific run
- `met.events.<org_id>.pipelines.<pipeline_id>` -- all events for a pipeline

The API layer subscribes to these subjects and relays to WebSocket connections.

---

## 5. Database Schema (Pipeline Tables)

These tables track pipeline definitions and execution history.

```sql
-- Pipeline definitions (parsed from YAML, stored for fast access)
CREATE TABLE pipelines (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id      UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name            TEXT NOT NULL,
    slug            TEXT NOT NULL,
    definition      JSONB NOT NULL,              -- Serialized PipelineIR
    source_file     TEXT,                        -- Path in repo
    source_sha      TEXT,                        -- Git commit SHA when parsed
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(project_id, slug)
);

-- Pipeline runs (one per execution)
CREATE TABLE pipeline_runs (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    pipeline_id     UUID NOT NULL REFERENCES pipelines(id) ON DELETE CASCADE,
    org_id          UUID NOT NULL REFERENCES organizations(id),
    status          run_status NOT NULL DEFAULT 'pending',
    triggered_by    TEXT NOT NULL,               -- 'manual', 'webhook', 'schedule', etc.
    trigger_data    JSONB,                       -- Event payload that triggered the run
    trace_id        UUID NOT NULL,               -- For distributed tracing
    started_at      TIMESTAMPTZ,
    finished_at     TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_pipeline_runs_pipeline ON pipeline_runs(pipeline_id, created_at DESC);
CREATE INDEX idx_pipeline_runs_status ON pipeline_runs(status) WHERE status IN ('pending', 'queued', 'running');

-- Job runs (one per job per pipeline run)
CREATE TABLE job_runs (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    run_id          UUID NOT NULL REFERENCES pipeline_runs(id) ON DELETE CASCADE,
    job_id          UUID NOT NULL,               -- From PipelineIR, not a foreign key
    job_name        TEXT NOT NULL,
    status          run_status NOT NULL DEFAULT 'pending',
    agent_id        UUID REFERENCES agents(id),
    attempt         INT NOT NULL DEFAULT 1,
    exit_code       INT,
    error_message   TEXT,
    cache_hit       BOOLEAN NOT NULL DEFAULT FALSE,
    cache_key       TEXT,
    started_at      TIMESTAMPTZ,
    finished_at     TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_job_runs_run ON job_runs(run_id);
CREATE INDEX idx_job_runs_agent ON job_runs(agent_id) WHERE status = 'running';

-- Step runs (one per step per job run)
CREATE TABLE step_runs (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    job_run_id      UUID NOT NULL REFERENCES job_runs(id) ON DELETE CASCADE,
    step_id         UUID NOT NULL,               -- From PipelineIR
    step_name       TEXT NOT NULL,
    status          run_status NOT NULL DEFAULT 'pending',
    exit_code       INT,
    log_path        TEXT,                        -- Object storage path
    started_at      TIMESTAMPTZ,
    finished_at     TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_step_runs_job ON step_runs(job_run_id);

-- Cache entries
CREATE TABLE cache_entries (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id      UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    cache_key       TEXT NOT NULL,
    storage_path    TEXT NOT NULL,               -- Object storage path
    size_bytes      BIGINT NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_hit_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    hit_count       INT NOT NULL DEFAULT 0,
    expires_at      TIMESTAMPTZ,
    UNIQUE(project_id, cache_key)
);

CREATE INDEX idx_cache_entries_project ON cache_entries(project_id);
CREATE INDEX idx_cache_entries_lru ON cache_entries(project_id, last_hit_at);

-- Artifacts
CREATE TABLE artifacts (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    run_id          UUID NOT NULL REFERENCES pipeline_runs(id) ON DELETE CASCADE,
    job_run_id      UUID NOT NULL REFERENCES job_runs(id) ON DELETE CASCADE,
    name            TEXT NOT NULL,
    storage_path    TEXT NOT NULL,
    content_type    TEXT,
    size_bytes      BIGINT NOT NULL,
    sha256          TEXT NOT NULL,
    pinned          BOOLEAN NOT NULL DEFAULT FALSE,
    expires_at      TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_artifacts_run ON artifacts(run_id);
CREATE INDEX idx_artifacts_job ON artifacts(job_run_id);

-- Reusable workflows (global scope)
CREATE TABLE reusable_workflows (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id          UUID REFERENCES organizations(id),  -- NULL for platform-global
    scope           TEXT NOT NULL CHECK (scope IN ('platform', 'organization')),
    name            TEXT NOT NULL,
    version         TEXT NOT NULL,
    description     TEXT,
    definition      JSONB NOT NULL,              -- Serialized RawWorkflowDef
    deprecated      BOOLEAN NOT NULL DEFAULT FALSE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(org_id, name, version)
);

CREATE INDEX idx_workflows_name ON reusable_workflows(name, version);
```

---

## 6. API Endpoints

### 6.1 Pipeline Execution

```
POST /api/projects/{project_id}/pipelines/{pipeline_id}/runs
  Request: { trigger: "manual", inputs: { ... } }
  Response: { run_id: "...", status: "queued" }

GET /api/runs/{run_id}
  Response: { id, pipeline_id, status, jobs: [...], started_at, finished_at }

GET /api/runs/{run_id}/jobs
  Response: [{ id, name, status, agent_id, steps: [...] }, ...]

GET /api/runs/{run_id}/jobs/{job_run_id}/logs
  Response: Streaming log content (text/plain or JSON lines)

POST /api/runs/{run_id}/cancel
  Response: { status: "cancelling" }

GET /api/runs/{run_id}/artifacts
  Response: [{ id, name, size_bytes, download_url }, ...]
```

### 6.2 WebSocket Events

```
WS /api/ws/runs/{run_id}
  Subscribes to real-time events for a run
  Messages: RunEvent JSON

WS /api/ws/pipelines/{pipeline_id}
  Subscribes to all runs for a pipeline
  Messages: RunEvent JSON
```

### 6.3 Cache Management

```
GET /api/projects/{project_id}/cache
  Response: [{ key, size_bytes, last_hit_at, hit_count }, ...]

DELETE /api/projects/{project_id}/cache/{key}
  Evict a specific cache entry

POST /api/projects/{project_id}/cache/purge
  Request: { older_than: "30d" }
  Purge cache entries older than threshold
```

---

## 7. Crate Dependency Map

```
met-parser
  ├── met-core          (shared types, IDs)
  ├── serde / serde_yaml (YAML parsing)
  ├── indexmap          (ordered maps)
  ├── regex             (pattern matching)
  ├── humantime         (duration parsing)
  ├── thiserror         (error types)
  └── tracing           (instrumentation)

met-engine
  ├── met-core          (shared types, IDs)
  ├── met-store         (database access)
  ├── met-parser        (PipelineIR)
  ├── met-secrets       (secret encryption)
  ├── met-proto         (protobuf messages)
  ├── met-objstore      (object storage) -- deferred
  ├── async-nats        (NATS JetStream)
  ├── cel-interpreter   (CEL conditions)
  ├── sqlx              (database queries)
  ├── tokio             (async runtime)
  ├── sha2              (cache key hashing)
  ├── futures           (async utilities)
  ├── indexmap          (ordered maps)
  ├── prost             (protobuf encoding)
  └── tracing           (instrumentation)
```

---

## 8. Build Order (Within Phase 2)


| Step | Work                                                | Depends On            |
| ---- | --------------------------------------------------- | --------------------- |
| 2.0  | Parser source span tracking                         | Phase 0               |
| 2.1  | Database migrations (pipeline_runs, job_runs, etc.) | Phase 0 DB            |
| 2.2  | DatabaseWorkflowProvider implementation             | 2.1, met-store        |
| 2.3  | hashFiles() and cache key template helpers          | -                     |
| 2.4  | JobPayload protobuf definition                      | Phase 0 proto tooling |
| 2.5  | Engine database persistence (create/update runs)    | 2.1                   |
| 2.6  | Engine retry logic                                  | 2.5                   |
| 2.7  | Secret encryption integration                       | Phase 1 met-secrets   |
| 2.8  | Object storage cache backend                        | Phase 0 met-objstore  |
| 2.9  | Artifact upload/download                            | 2.8                   |
| 2.10 | Completion listener (agent -> engine)               | Phase 1 controller    |
| 2.11 | Log streaming relay                                 | 2.10                  |
| 2.12 | API endpoints (trigger, status, cancel)             | 2.5                   |
| 2.13 | WebSocket event relay                               | 2.12                  |
| 2.14 | Integration tests                                   | 2.0 -- 2.13           |


---

## 9. Current Implementation Status

### 9.1 `met-parser` (Substantially Complete)

The parser crate has a working 6-stage pipeline:

- **Stage 1 (Deserialize)**: ✅ Using serde_yaml
- **Stage 2 (Schema Validate)**: ✅ Required fields, types, duplicate IDs
- **Stage 3 (Workflow Resolution)**: ✅ MockWorkflowProvider works; needs DB/git providers
- **Stage 4 (Variable Resolution)**: ✅ ${...} validation implemented
- **Stage 5 (DAG Construction)**: ✅ Cycle detection with Kahn's algorithm
- **Stage 6 (Emit IR)**: ✅ Full PipelineIR generation

**Remaining work**:

- Source span tracking for line-numbered errors
- DatabaseWorkflowProvider for global workflows
- GitWorkflowProvider for project workflows
- Semver version resolution
- hashFiles() helper function

### 9.2 `met-engine` (Core Loop Complete)

The engine has a working execution loop:

- **DAG Executor**: ✅ Topological ordering, dependency tracking, concurrent dispatch
- **Job Scheduler**: ✅ NATS dispatch, timeout tracking, cancellation
- **Cache Manager**: ✅ MemoryCache works; ObjectStoreCache stubbed
- **CEL Conditions**: ✅ Basic condition evaluation
- **Event Broadcasting**: ✅ NATS event publishing

**Remaining work**:

- Database persistence of run state
- Retry policy execution
- Artifact passing between jobs
- ObjectStoreCache completion
- Secret encryption integration
- Completion listener wiring
- Log streaming relay

---

## 10. Open Questions and Decisions


| Question                                                | Current Leaning                    | Notes                                                  |
| ------------------------------------------------------- | ---------------------------------- | ------------------------------------------------------ |
| Should cache keys be content-addressed or user-defined? | User-defined with helper functions | Matches GitHub Actions model, more predictable         |
| How to handle workflow version conflicts?               | Semver resolution with lockfile    | Lockfile optional, default to latest matching          |
| Should artifacts be typed (binary, log, report)?        | Yes, with content_type             | Enables UI rendering (logs as text, reports as HTML)   |
| Maximum workflow nesting depth?                         | 5 levels                           | Prevents infinite recursion, keeps debugging tractable |
| Retry scope: job-level only or step-level too?          | Job-level only for now             | Step-level retries add complexity, defer to later      |
| Cache isolation: per-branch or shared?                  | Shared with branch in key          | User controls isolation via cache key template         |


