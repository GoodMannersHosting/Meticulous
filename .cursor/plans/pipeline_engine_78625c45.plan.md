---
name: Pipeline Engine
overview: "Detailed plan for the Meticulous pipeline engine: YAML parser with source-span errors, pipeline IR, reusable workflow resolution with semver versioning, DAG execution with concurrent branch scheduling, NATS-based job dispatch, multi-layer caching, artifact passing, conditional execution (CEL), retry/timeout, and an event broadcast system."
todos: []
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

**Timeouts**: Every job and step has a timeout (explicit or inherited from global/project defaults). Engine starts