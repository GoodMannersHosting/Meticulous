---
name: Agent System
overview: "Detailed plan for the Meticulous agent system: agent binary, agent controller, NATS pub/sub integration, agent provisioning flow, multi-platform support, and Kubernetes operator."
todos:
  - id: agent-proto
    content: Define protobuf service definitions for agent<->controller gRPC (registration, heartbeat, job status, log streaming)
    status: completed
  - id: agent-nats-subjects
    content: Design NATS subject hierarchy and JetStream consumer configuration for job dispatch
    status: completed
  - id: agent-binary
    content: "Implement met-agent binary: CLI entrypoint, config loading, NATS connection, gRPC client, job executor loop"
    status: completed
  - id: agent-controller
    content: "Implement met-controller: gRPC server, agent registry, health tracking, join token management"
    status: completed
  - id: agent-provisioning
    content: "Implement provisioning flow: join token creation, security bundle validation, JWT issuance, agent enrollment"
    status: completed
  - id: agent-job-lifecycle
    content: "Implement job lifecycle: NATS pickup, per-job PKI handshake, step execution, status reporting, log streaming"
    status: completed
  - id: agent-env-validation
    content: Implement operating environment validation (OS, arch, network, NTP, container runtime detection)
    status: completed
  - id: agent-execution
    content: "Implement step execution backends: container (Linux), native process (macOS/Windows), workspace isolation"
    status: completed
  - id: agent-operator
    content: "Implement met-operator: Kubernetes CRDs, reconciliation loop, agent pool auto-scaling"
    status: completed
  - id: agent-db-schema
    content: Design and implement agent-related database tables (agents, join_tokens, agent_heartbeats, job_assignments)
    status: completed
  - id: agent-cross-platform
    content: Set up cross-compilation targets and platform-specific build configuration (Linux, macOS, Windows)
    status: completed
  - id: agent-integration-tests
    content: "Write integration tests: agent registration, NATS dispatch, job execution, heartbeat, revocation"
    status: completed
  - id: agent-process-watcher
    content: "Implement process watcher: track child process spawns, compute SHA256 checksums of executed binaries, log as job metadata"
    status: completed
isProject: false
---

# Meticulous -- Agent System Plan

**Phase 1 of the Meticulous build order.** This plan depends on Phase 0 (Foundation) being complete -- specifically `met-core` types, the Postgres schema foundation, and protobuf tooling.

---

## 1. Overview

The agent system is the execution backbone of Meticulous. Agents are lightweight, stateless (between jobs) binaries that run on heterogeneous infrastructure -- Linux containers, bare-metal macOS build machines, Windows CI hosts -- and execute pipeline jobs dispatched by the control plane.

The system has three primary components:


| Component      | Crate            | Role                                                                                                                                   |
| -------------- | ---------------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| **Agent**      | `met-agent`      | Binary that runs on worker machines. Subscribes to NATS for jobs, reports status via gRPC, executes steps in isolated environments.    |
| **Controller** | `met-controller` | Control-plane service. Manages agent registration, health monitoring, join tokens, and coordinates with the scheduler via NATS.        |
| **Operator**   | `met-operator`   | Kubernetes operator (using `kube-rs`) that manages ephemeral agent pods, auto-scales agent pools, and handles CRD-based configuration. |


### Design Principles

1. **Egress-only networking**: Agents never accept inbound connections. All communication is agent-initiated (gRPC to controller, NATS subscription).
2. **Zero trust by default**: Agents prove identity via join tokens and security bundles. Secrets are encrypted per-job with one-time PKI. Agents are revocable server-side at any time.
3. **Multi-platform parity**: The core agent logic is platform-agnostic. Only the step execution backend varies (containers on Linux, native processes on macOS/Windows).
4. **Ephemeral preferred, long-lived supported**: Kubernetes agents are ephemeral (pod-per-job or pod-per-pool). Bare-metal/VM agents support long-lived operation with JWT renewal and approval workflows.

---

## 2. Protobuf Service Definitions

All agent-to-controller communication uses gRPC (via `tonic`). Protobuf definitions live in `proto/agent/`.

### 2.1 `AgentService` -- Controller-side gRPC server

```protobuf
syntax = "proto3";
package meticulous.agent.v1;

import "google/protobuf/timestamp.proto";

service AgentService {
  // Agent registration (join token exchange)
  rpc Register(RegisterRequest) returns (RegisterResponse);

  // Periodic heartbeat -- agent reports health, controller confirms liveness
  rpc Heartbeat(HeartbeatRequest) returns (HeartbeatResponse);

  // Agent reports job status transitions
  rpc ReportJobStatus(JobStatusReport) returns (JobStatusAck);

  // Bidirectional log streaming for a running job
  rpc StreamLogs(stream LogChunk) returns (LogStreamAck);

  // Per-job PKI: agent sends public key, receives encrypted secrets
  rpc ExchangeJobKeys(JobKeyExchange) returns (JobSecretsPayload);

  // Agent requests graceful deregistration
  rpc Deregister(DeregisterRequest) returns (DeregisterResponse);
}
```

### 2.2 Key Message Types

```protobuf
message RegisterRequest {
  string join_token = 1;
  SecurityBundle security_bundle = 2;
  AgentCapabilities capabilities = 3;
}

message SecurityBundle {
  string hostname = 1;
  string os = 2;
  string arch = 3;
  string kernel_version = 4;
  repeated string public_ips = 5;
  repeated string private_ips = 6;
  bool ntp_synchronized = 7;
  string container_runtime = 8;      // "docker", "podman", "containerd", "none"
  string container_runtime_version = 9;
  EnvironmentType environment_type = 10;  // physical, virtual, container
  bytes agent_x509_public_key = 11;
}

enum EnvironmentType {
  ENVIRONMENT_TYPE_UNSPECIFIED = 0;
  PHYSICAL = 1;
  VIRTUAL = 2;
  CONTAINER = 3;
}

message AgentCapabilities {
  string os = 1;
  string arch = 2;
  repeated string labels = 3;       // user-defined: "gpu", "high-memory", etc.
  repeated string pool_tags = 4;    // determines NATS subject subscriptions
}

message RegisterResponse {
  string agent_id = 1;              // server-assigned UUID
  string jwt_token = 2;
  google.protobuf.Timestamp jwt_expires_at = 3;
  bool renewable = 4;
  repeated string nats_subjects = 5;  // subjects this agent should subscribe to
  NatsCredentials nats_credentials = 6;
}

message HeartbeatRequest {
  string agent_id = 1;
  AgentStatus status = 2;           // idle, busy, draining
  ResourceSnapshot resources = 3;   // CPU, memory, disk utilization
  optional string current_job_id = 4;
}

message HeartbeatResponse {
  HeartbeatAction action = 1;       // continue, drain, terminate, update_config
  optional AgentConfigPatch config_patch = 2;
}

enum HeartbeatAction {
  HEARTBEAT_ACTION_UNSPECIFIED = 0;
  CONTINUE = 1;
  DRAIN = 2;      // finish current job, then stop accepting new ones
  TERMINATE = 3;  // cancel current job and shut down
  UPDATE_CONFIG = 4;
}

message JobKeyExchange {
  string agent_id = 1;
  string job_id = 2;
  bytes one_time_x509_public_key = 3;  // per-job keypair
}

message JobSecretsPayload {
  string job_id = 1;
  repeated EncryptedSecret secrets = 2;
}

message EncryptedSecret {
  string key = 1;
  bytes encrypted_value = 2;
  string sha256_checksum = 3;  // of plaintext, for agent-side verification
}

// Job status report with execution metadata
message JobStatusReport {
  string agent_id = 1;
  string job_id = 2;
  JobStatus status = 3;
  optional string step_id = 4;
  optional int32 exit_code = 5;
  optional string failure_reason = 6;
  google.protobuf.Timestamp timestamp = 7;
  optional JobExecutionMetadata execution_metadata = 8;  // populated on job completion
}

message JobStatusAck {
  bool acknowledged = 1;
}

// Execution metadata collected during job run
message JobExecutionMetadata {
  repeated ExecutedBinary executed_binaries = 1;
  uint64 total_processes_spawned = 2;
  uint32 execution_tree_depth = 3;
}

message ExecutedBinary {
  string path = 1;
  string sha256 = 2;
  uint32 execution_count = 3;
  google.protobuf.Timestamp first_executed_at = 4;
  google.protobuf.Timestamp last_executed_at = 5;
  repeated string step_ids = 6;
}

enum JobStatus {
  JOB_STATUS_UNSPECIFIED = 0;
  ACCEPTED = 1;
  RUNNING = 2;
  SUCCEEDED = 3;
  FAILED = 4;
  CANCELLED = 5;
  TIMED_OUT = 6;
}
```

---

## 3. NATS Subject Design

NATS with JetStream provides durable, at-least-once delivery for job dispatch. The subject hierarchy is organized by tenant, project, and agent pool.

### 3.1 Subject Hierarchy

```
met.jobs.{tenant_id}.{pool_tag}          # Job dispatch to a specific pool
met.jobs.{tenant_id}._default            # Default pool (no explicit pool_tag)
met.status.{tenant_id}.{agent_id}        # Agent status updates (published by controller)
met.cancel.{tenant_id}.{job_id}          # Job cancellation signals
met.broadcast.{tenant_id}                # Broadcast messages to all agents in a tenant
```

### 3.2 JetStream Configuration


| Stream   | Subjects       | Retention | Max Age | Replicas | Notes                                      |
| -------- | -------------- | --------- | ------- | -------- | ------------------------------------------ |
| `JOBS`   | `met.jobs.>`   | WorkQueue | 24h     | 3        | Each message consumed by exactly one agent |
| `STATUS` | `met.status.>` | Limits    | 1h      | 1        | Ephemeral, controller-published            |
| `CANCEL` | `met.cancel.>` | Interest  | 1h      | 3        | Fanout to all subscribers for a job        |


### 3.3 Consumer Design

Each agent pool gets a **durable pull consumer** on the `JOBS` stream, filtered by subject. This gives us:

- **Work-queue semantics**: each job goes to exactly one agent.
- **Explicit ack**: agents ack after accepting the job (not after completion -- status is tracked via gRPC).
- **Redelivery**: if an agent crashes before acking, the message redelivers to another agent in the pool.
- **Flow control**: agents pull at their own pace (one-at-a-time for single-job agents, or batched for multi-slot agents).

```
Consumer: pool-{tenant_id}-{pool_tag}
  Filter: met.jobs.{tenant_id}.{pool_tag}
  Deliver: pull
  AckPolicy: explicit
  AckWait: 30s
  MaxDeliver: 3
  MaxAckPending: per-agent concurrency slots
```

### 3.4 Job Message Envelope

```json
{
  "job_id": "uuid",
  "run_id": "uuid",
  "pipeline_id": "uuid",
  "tenant_id": "uuid",
  "priority": 50,
  "created_at": "2026-03-30T00:00:00Z",
  "timeout_seconds": 3600,
  "steps": [
    {
      "step_id": "uuid",
      "name": "build",
      "image": "rust:1.80-bookworm",
      "commands": ["cargo build --release"],
      "env": {"CARGO_INCREMENTAL": "0"},
      "secrets_required": ["REGISTRY_TOKEN", "SIGNING_KEY"],
      "cache_keys": ["cargo-target-{hash}"],
      "timeout_seconds": 600
    }
  ],
  "artifacts": {
    "inputs": [{"name": "source", "path": "/workspace/src", "source": "s3://..."}],
    "outputs": [{"name": "binary", "path": "/workspace/target/release/app", "dest": "s3://..."}]
  },
  "required_capabilities": {
    "os": "linux",
    "arch": "amd64",
    "labels": ["docker"]
  }
}
```

---

## 4. Agent Binary (`met-agent`)

### 4.1 Architecture

```
met-agent process
├── Config Loader        ← TOML/env config (controller URL, join token, labels)
├── gRPC Client          ← tonic client to met-controller
├── NATS Client          ← async-nats with JetStream
├── Heartbeat Task       ← tokio::spawn, periodic gRPC heartbeat
├── Job Executor Loop    ← main loop: pull NATS → execute → report
│   ├── PKI Manager      ← per-job X509 keypair generation
│   ├── Execution Backend
│   │   ├── ContainerBackend (Linux)    ← OCI container via containerd/docker
│   │   └── NativeBackend (macOS/Win)   ← isolated process execution
│   ├── Log Shipper      ← streams stdout/stderr to controller
│   └── Artifact Manager ← upload/download from object storage
└── Signal Handler       ← SIGTERM/SIGINT → graceful drain
```

### 4.2 Configuration

Agent configuration is loaded from (in order of precedence): CLI flags > environment variables > config file (`/etc/meticulous/agent.toml` or `~/.config/meticulous/agent.toml`).

```toml
[agent]
controller_url = "https://controller.meticulous.internal:9443"
join_token = "met_join_xxxxxxxxxxxxxxxxxxxx"   # or via MET_JOIN_TOKEN env var
labels = ["gpu", "high-memory"]
pool_tags = ["linux-amd64", "docker"]
concurrency = 1                                 # max simultaneous jobs
workspace_dir = "/var/lib/meticulous/workspaces"
log_level = "info"

[tls]
ca_cert = "/etc/meticulous/ca.pem"              # for mTLS to controller
# client_cert and client_key populated after registration

[nats]
# populated by controller during registration; can be overridden
url = ""
credentials_file = ""
```

### 4.3 Startup Sequence

1. Load configuration (TOML + env + CLI flags).
2. If no agent identity exists locally (`/var/lib/meticulous/agent-id`):
  a. Collect `SecurityBundle` (OS detection, network info, NTP check, container runtime).
   b. Generate long-term X509 keypair for agent identity.
   c. Call `AgentService.Register()` with join token + security bundle.
   d. Receive `agent_id`, JWT, and NATS credentials. Persist to disk.
3. If agent identity exists, validate stored JWT. If expired and renewable, call `Heartbeat` which can trigger renewal. If non-renewable and expired, re-register.
4. Connect to NATS using provided credentials. Subscribe to assigned subjects.
5. Spawn heartbeat background task (interval: 15s, configurable).
6. Enter job executor loop.

### 4.4 Job Executor Loop

```
loop {
    msg = nats_consumer.pull(batch=1, timeout=30s)
    if msg is None → continue

    job = deserialize(msg.payload)
    msg.ack()  // ack receipt, not completion

    report_status(job.id, ACCEPTED)

    // Per-job PKI handshake
    (privkey, pubkey) = generate_x509_keypair()
    secrets = grpc.exchange_job_keys(job.id, pubkey)
    decrypted = decrypt_and_verify(secrets, privkey)

    // Initialize process watcher for this job
    process_watcher = ProcessWatcher::new()
    all_executed_binaries = []

    // Execute steps sequentially
    for step in job.steps {
        report_status(job.id, step.id, RUNNING)

        // Run step with process watching enabled
        result = execution_backend.run(step, decrypted, workspace, &mut process_watcher)
        stream_logs(job.id, step.id, result.stdout, result.stderr)

        // Collect executed binaries from this step
        all_executed_binaries.extend(result.executed_binaries)

        if result.exit_code != 0 {
            report_status(job.id, step.id, FAILED, result.exit_code)
            report_status(job.id, FAILED)
            break
        }
        report_status(job.id, step.id, SUCCEEDED)
    }

    upload_artifacts(job.artifacts.outputs)

    // Aggregate and deduplicate binary execution metadata
    execution_metadata = aggregate_execution_metadata(all_executed_binaries, process_watcher)

    // Report final status with execution metadata
    report_status(job.id, SUCCEEDED, execution_metadata)

    // Zeroize secrets from memory
    zeroize(decrypted)
    drop(privkey)

    cleanup_workspace(job.id)
}
```

### 4.5 Graceful Shutdown

On receiving SIGTERM or SIGINT (or `DRAIN` command via heartbeat response):

1. Stop pulling new jobs from NATS.
2. If a job is in progress, allow it to complete (up to a configurable grace period).
3. If grace period expires, send `CANCELLED` status for the in-progress job.
4. Call `AgentService.Deregister()`.
5. Disconnect from NATS and exit.

---

## 5. Agent Controller (`met-controller`)

### 5.1 Architecture

```
met-controller process
├── gRPC Server (tonic)
│   ├── Register handler       ← validates join tokens, creates agent records
│   ├── Heartbeat handler      ← updates liveness, issues commands
│   ├── ReportJobStatus        ← persists status, triggers pipeline engine callbacks
│   ├── StreamLogs             ← forwards log chunks to log storage / WebSocket fanout
│   ├── ExchangeJobKeys        ← delegates to secrets broker for encryption
│   └── Deregister handler     ← marks agent offline, cleans up
├── Agent Registry             ← in-memory + DB-backed agent state
├── Health Monitor             ← detects stale agents (missed heartbeats)
│   └── Reaper Task            ← marks unresponsive agents dead, requeues jobs
├── Join Token Manager         ← CRUD for join tokens with scope/expiry
├── NATS Publisher             ← publishes jobs to dispatch subjects
└── Metrics Exporter           ← agent pool sizes, job queue depths
```

### 5.2 Join Token Management

Join tokens are the entry point for agent enrollment. They are created by platform admins or project owners via the API.

```
Token format: met_join_{base62_random(32)}
```


| Field        | Type      | Description                                         |
| ------------ | --------- | --------------------------------------------------- |
| `id`         | UUID      | Primary key                                         |
| `token_hash` | TEXT      | bcrypt hash of the token (plaintext never stored)   |
| `scope`      | ENUM      | `platform`, `tenant`, `project`, `pipeline`         |
| `scope_id`   | UUID      | ID of the scoped entity (null for `platform`)       |
| `max_uses`   | INT       | null = unlimited                                    |
| `uses`       | INT       | current registration count                          |
| `expires_at` | TIMESTAMP | null = no expiry                                    |
| `created_by` | UUID      | user who created the token                          |
| `labels`     | TEXT[]    | forced labels applied to agents using this token    |
| `pool_tags`  | TEXT[]    | forced pool tags applied to agents using this token |
| `revoked`    | BOOL      | admin can revoke without deleting                   |


Scope hierarchy determines what jobs an agent registered with this token can execute:

- `platform`: any job across any tenant.
- `tenant`: any job within the specified tenant.
- `project`: jobs for pipelines in the specified project.
- `pipeline`: jobs for a specific pipeline only.

### 5.3 Agent Registry

The controller maintains agent state both in-memory (for fast access during heartbeat/dispatch) and in PostgreSQL (for persistence and querying).

In-memory state (refreshed from DB on controller startup):

```rust
struct AgentState {
    agent_id: Uuid,
    status: AgentStatus,       // online, busy, draining, offline, dead
    last_heartbeat: Instant,
    capabilities: AgentCapabilities,
    current_job: Option<Uuid>,
    jwt_expires_at: DateTime<Utc>,
    resource_snapshot: Option<ResourceSnapshot>,
}
```

### 5.4 Health Monitoring and Reaping

The health monitor runs as a background task:

- **Heartbeat interval**: 15s (agent-side), controller expects heartbeat within 45s (3x interval).
- **Stale threshold**: 45s without heartbeat → mark agent `offline`.
- **Dead threshold**: 120s without heartbeat → mark agent `dead`, requeue any assigned job.
- **Requeue**: When an agent dies mid-job, the job message is re-published to the NATS dispatch subject (up to the job's `MaxDeliver` limit).

### 5.5 Server-Side Agent Revocation

Admins can revoke an agent at any time via the API:

1. Agent record marked `revoked` in DB.
2. Next heartbeat response returns `TERMINATE` action.
3. If the agent doesn't respond within the dead threshold, the controller force-deregisters and requeues the job.
4. The agent's NATS credentials are revoked (NATS supports credential revocation via account JWTs).

---

## 6. Operating Environment Validation

During registration, the controller validates the agent's `SecurityBundle`. Validation rules are configurable per-tenant.

### 6.1 Default Validations


| Check             | Behavior                                                                                 |
| ----------------- | ---------------------------------------------------------------------------------------- |
| NTP sync          | **Required**. Reject agents with unsynchronized clocks.                                  |
| OS/Arch           | Must match at least one allowed OS/arch combination for the token's scope.               |
| Container runtime | If pool_tags include container-requiring tags (e.g., `docker`), runtime must be present. |
| Public IP         | Optionally checked against an allowlist (for compliance environments).                   |
| Kernel version    | Advisory only (logged/stored), not enforced by default.                                  |


### 6.2 Configurable Validation Plugins

Tenants can define custom validation rules as policy expressions (likely using a lightweight embedded policy language or simple predicate DSL):

```yaml
agent_validation:
  rules:
    - name: require-private-network
      condition: "security_bundle.private_ips | any(startswith('10.0.'))"
      action: reject_unless_match
    - name: minimum-kernel
      condition: "security_bundle.kernel_version >= '6.1'"
      action: warn
```

This is a later enhancement -- initial implementation uses the hardcoded default validations.

---

## 7. Per-Job PKI Flow

This is a critical security mechanism ensuring secrets are never in plaintext on the wire and are scoped to a single job execution.

### 7.1 Sequence

```
Agent                          Controller                    Secrets Broker
  |                                |                              |
  |--- pick up job from NATS ----->|                              |
  |                                |                              |
  |  generate one-time X509 pair   |                              |
  |--- ExchangeJobKeys(pubkey) --->|                              |
  |                                |--- resolve secrets for job ->|
  |                                |<-- plaintext secrets --------|
  |                                |                              |
  |                                |  encrypt each secret with    |
  |                                |  agent's one-time pubkey     |
  |                                |  compute sha256 of plaintext |
  |                                |                              |
  |<-- JobSecretsPayload ----------|                              |
  |                                |                              |
  |  decrypt with one-time privkey |                              |
  |  verify sha256 of each secret  |                              |
  |  inject into step environment  |                              |
  |                                |                              |
  |  [job executes]                |                              |
  |                                |                              |
  |  zeroize privkey + secrets     |                              |
```

### 7.2 Implementation Notes

- X509 keypairs generated using `rcgen` (Rust X509 certificate generation).
- Encryption uses the public key to encrypt an ephemeral AES-256-GCM symmetric key, which encrypts the actual secret values (hybrid encryption). This avoids RSA size limitations for large secret values.
- The one-time private key is held only in memory and zeroized (via the `zeroize` crate) after all secrets are decrypted.
- The sha256 checksum is computed on the **plaintext** value before encryption. The agent recomputes after decryption to verify integrity.
- If checksum verification fails for any secret, the job is immediately failed with a security error.

---

## 8. Step Execution Backends

### 8.1 Container Backend (Linux)

The primary execution backend for Linux agents. Each step runs in an OCI container.

- **Runtime abstraction**: Support `containerd` (via `containerd-client` crate) and `docker` (via Docker Engine API). Runtime auto-detected or configured.
- **Workspace isolation**: Each job gets a unique workspace directory, bind-mounted into step containers at `/workspace`.
- **Networking**: Steps run in an isolated network namespace by default. Egress can be allowed/restricted via pipeline configuration.
- **Resource limits**: CPU, memory, and PID limits applied per-step via cgroup configuration.
- **Image pulling**: Images pulled from configured registries. Image pull secrets managed by the secrets broker.
- **Cache mounts**: Cache directories (e.g., cargo target, npm cache) are bind-mounted from a shared cache volume, keyed by cache key hashes.

### 8.2 Native Backend (macOS / Windows)

For platforms where containers are not natively supported or practical.

- **Process isolation**: Each step runs as a subprocess with a restricted environment. On macOS, uses `sandbox-exec` profiles where available. On Windows, uses job objects for resource limiting.
- **Workspace**: Same directory-per-job model as container backend.
- **Tool management**: Pre-installed tools expected at known paths (build tool volumes mounted read-only are a future enhancement -- see design notes on `/buildtools/` layout).
- **Environment scrubbing**: The step process inherits only explicitly declared environment variables, not the agent's full environment.

### 8.3 Execution Trait

Both backends implement a common trait:

```rust
#[async_trait]
pub trait ExecutionBackend: Send + Sync {
    async fn prepare_workspace(&self, job: &JobSpec) -> Result<WorkspaceHandle>;

    async fn run_step(
        &self,
        step: &StepSpec,
        workspace: &WorkspaceHandle,
        env: HashMap<String, String>,
        log_sink: mpsc::Sender<LogChunk>,
    ) -> Result<StepResult>;

    async fn cleanup_workspace(&self, workspace: WorkspaceHandle) -> Result<()>;
}

pub struct StepResult {
    pub exit_code: i32,
    pub duration: Duration,
    pub resource_usage: ResourceUsage,
    pub executed_binaries: Vec<ExecutedBinary>,  // SHA256 checksums of all binaries executed
}

pub struct ExecutedBinary {
    pub path: String,           // absolute path to the binary
    pub sha256: String,         // hex-encoded SHA256 checksum
    pub pid: u32,               // process ID
    pub parent_pid: u32,        // parent process ID
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub exit_code: Option<i32>,
}
```

### 8.4 Process Watcher and Binary Checksum Tracking

For security auditing and reproducibility, the agent tracks all child processes spawned during step execution and computes SHA256 checksums of all executed binaries. This metadata is collected and logged at the end of each job.

#### 8.4.1 Design Goals

1. **Complete visibility**: Track every process spawned during step execution, including nested child processes.
2. **Binary attestation**: Compute and record SHA256 checksums of all executed binaries for supply chain security.
3. **Minimal overhead**: Process watching should not significantly impact step execution performance.
4. **Cross-platform support**: Work on Linux (primary), macOS, and Windows with platform-appropriate mechanisms.

#### 8.4.2 Linux Implementation

On Linux, the process watcher uses eBPF to trace process execution with minimal overhead:

- **eBPF tracepoint**: Attach to `tracepoint/syscalls/sys_enter_execve` to intercept all `execve` syscalls.
- **Process tree tracking**: Maintain parent-child relationships via eBPF maps.
- `**/proc/{pid}/exe`**: Resolve the executable path for each process from userspace.
- **File hashing**: Compute SHA256 of the binary file when a new process is detected.

The eBPF approach is preferred over ptrace because:

1. **Lower overhead**: No context switches per syscall; data collected in-kernel.
2. **No process slowdown**: Traced processes run at full speed.
3. **Container-aware**: Works correctly with containerized workloads from outside the namespace.

For containerized execution, the watcher operates from outside the container namespace using `/proc/{container_pid}/root/proc/{pid}/exe` path resolution.

```rust
use aya::{Bpf, maps::AsyncPerfEventArray};

pub struct ProcessWatcher {
    bpf: Bpf,
    events: AsyncPerfEventArray<ExecEvent>,
    tracked_processes: HashMap<u32, TrackedProcess>,
    binary_cache: HashMap<(PathBuf, u64, SystemTime), String>,  // (path, inode, mtime) -> sha256
}

#[repr(C)]
struct ExecEvent {
    pid: u32,
    ppid: u32,
    filename: [u8; 256],
    timestamp_ns: u64,
}

struct TrackedProcess {
    pid: u32,
    parent_pid: u32,
    exe_path: PathBuf,
    exe_sha256: String,
    started_at: Instant,
}

impl ProcessWatcher {
    /// Load eBPF program and start watching execve syscalls
    pub async fn new() -> Result<Self>;

    /// Poll for new process events (non-blocking)
    pub async fn poll(&mut self) -> Result<Vec<ProcessEvent>>;

    /// Get all tracked processes with their binary checksums
    pub fn get_executed_binaries(&self) -> Vec<ExecutedBinary>;

    /// Stop watching and cleanup eBPF resources
    pub fn stop(self);
}
```

**Kernel requirements**: eBPF tracepoints require Linux kernel 4.15+ with `CONFIG_BPF_SYSCALL=y`. The agent validates eBPF availability at startup and fails fast if unsupported.

#### 8.4.3 macOS Implementation

On macOS, use Endpoint Security framework or `dtrace` for process monitoring:

- **Endpoint Security (`es_subscribe`)**: Subscribe to `ES_EVENT_TYPE_NOTIFY_EXEC` events for process execution tracking.
- **Binary path**: Extract from the `es_message_t` event structure.
- **Fallback**: Use `dtrace` scripting if Endpoint Security entitlements are not available.

Note: Endpoint Security requires a signed binary with the appropriate entitlement (`com.apple.developer.endpoint-security.client`).

#### 8.4.4 Windows Implementation

On Windows, use ETW (Event Tracing for Windows) or Job Objects:

- **ETW Provider**: Subscribe to the Microsoft-Windows-Kernel-Process provider for process start/stop events.
- **Job Objects**: If the step runs in a Windows Job Object, use `QueryInformationJobObject` to enumerate child processes.
- **Binary path**: Extract from `EVENT_RECORD` or `GetProcessImageFileName`.

#### 8.4.5 Checksum Computation

SHA256 checksums are computed efficiently with streaming hashing:

```rust
use sha2::{Sha256, Digest};
use tokio::fs::File;
use tokio::io::AsyncReadExt;

async fn compute_binary_sha256(path: &Path) -> Result<String> {
    let mut file = File::open(path).await?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 65536];  // 64KB buffer

    loop {
        let n = file.read(&mut buffer).await?;
        if n == 0 { break; }
        hasher.update(&buffer[..n]);
    }

    Ok(hex::encode(hasher.finalize()))
}
```

**Caching**: Binary checksums are cached by (path, inode, mtime) tuple to avoid re-hashing the same binary multiple times within a job.

#### 8.4.6 Metadata Collection and Reporting

At the end of each job, the collected binary execution metadata is:

1. **Aggregated**: Combined from all steps into a single job-level report.
2. **Deduplicated**: Same binary executed multiple times is reported once with execution count.
3. **Reported**: Sent to the controller via the job status report.
4. **Stored**: Persisted in the database for audit queries.

```rust
pub struct JobExecutionMetadata {
    pub job_id: Uuid,
    pub executed_binaries: Vec<ExecutedBinaryRecord>,
    pub total_processes_spawned: u64,
    pub execution_tree_depth: u32,  // max depth of process tree
}

pub struct ExecutedBinaryRecord {
    pub path: String,
    pub sha256: String,
    pub execution_count: u32,
    pub first_executed_at: DateTime<Utc>,
    pub last_executed_at: DateTime<Utc>,
    pub step_ids: Vec<Uuid>,  // which steps executed this binary
}
```

#### 8.4.7 Security Considerations

- **Checksum verification**: The controller can optionally validate checksums against an allowlist of known-good binaries.
- **Anomaly detection**: Unexpected binaries (e.g., not in the base image or build outputs) can trigger alerts.
- **SBOM integration**: Binary checksums can be cross-referenced with Software Bill of Materials for supply chain validation.
- **Tamper detection**: If a binary's checksum changes between executions within the same job, flag as potential compromise.

---

## 9. Kubernetes Operator (`met-operator`)

### 9.1 Purpose

The operator manages ephemeral agent pools on Kubernetes, similar in concept to GitHub Actions Runner Controller (ARC). It watches custom resources and reconciles the desired state by creating/destroying agent pods.

### 9.2 Custom Resource Definitions

#### `AgentPool` CRD

```yaml
apiVersion: meticulous.dev/v1alpha1
kind: AgentPool
metadata:
  name: linux-amd64-docker
  namespace: meticulous
spec:
  replicas:
    min: 1
    max: 20
    idle: 2                          # keep 2 idle agents warm
  selector:
    os: linux
    arch: amd64
    labels:
      - docker
  poolTags:
    - linux-amd64
    - docker
  template:
    spec:
      containers:
        - name: agent
          image: ghcr.io/meticulous/agent:latest
          resources:
            requests:
              cpu: "500m"
              memory: "512Mi"
            limits:
              cpu: "4"
              memory: "8Gi"
          volumeMounts:
            - name: workspace
              mountPath: /var/lib/meticulous/workspaces
            - name: docker-socket
              mountPath: /var/run/docker.sock
      volumes:
        - name: workspace
          emptyDir:
            sizeLimit: 50Gi
        - name: docker-socket
          hostPath:
            path: /var/run/docker.sock
  controllerUrl: https://controller.meticulous.internal:9443
  joinTokenSecretRef:
    name: agent-join-token
    key: token
status:
  ready: 5
  busy: 3
  idle: 2
  totalJobsCompleted: 1247
```

#### `AgentPoolAutoscaler` CRD (optional, can be inline in AgentPool)

```yaml
apiVersion: meticulous.dev/v1alpha1
kind: AgentPoolAutoscaler
metadata:
  name: linux-amd64-scaler
spec:
  poolRef:
    name: linux-amd64-docker
  metrics:
    - type: QueueDepth
      target:
        subject: "met.jobs.*.linux-amd64"
        threshold: 5              # scale up when >5 pending jobs
    - type: IdleAgents
      target:
        min: 2                    # always keep 2 idle
  behavior:
    scaleUp:
      stabilizationWindowSeconds: 30
      policies:
        - type: Pods
          value: 5
          periodSeconds: 60
    scaleDown:
      stabilizationWindowSeconds: 300
      policies:
        - type: Pods
          value: 2
          periodSeconds: 120
```

### 9.3 Reconciliation Loop

The operator's reconciler watches `AgentPool` resources and:

1. **Desired state**: Compute desired replica count based on autoscaler metrics (NATS queue depth, current idle count, min/max bounds).
2. **Current state**: List agent pods matching the pool's label selector.
3. **Scale up**: If desired > current, create new agent pods from the template. Each pod gets a fresh join token (or uses the shared one from the secret ref).
4. **Scale down**: If desired < current, select idle agents (prefer longest-idle), send `DRAIN` via heartbeat, wait for graceful shutdown, then delete pod.
5. **Health**: If a pod is in `CrashLoopBackOff` or the agent has been `dead` in the controller for >60s, delete and replace the pod.

### 9.4 Docker-in-Docker vs Docker Socket

The operator supports two modes for container step execution:

- **Docker-in-Docker (DinD)** (default for Kubernetes): Sidecar `dind` container with its own Docker daemon. Provides strong isolation between jobs and prevents container escape attacks. Slightly higher overhead but essential for multi-tenant security.
- **Docker Socket mount**: Host Docker socket mounted into agent pod. Better performance but less isolated. Opt-in via `spec.template.socketMount: true` -- only recommended for single-tenant clusters with trusted workloads.

For **non-Kubernetes agents** (bare-metal, VMs), the default is reversed: socket mount (or direct containerd/docker access) is the default since the agent typically has dedicated access to the container runtime.

The choice is made per-pool via the `AgentPool` CRD.

---

## 10. Database Schema (Agent Tables)

These tables extend the foundation schema from Phase 0.

```sql
-- Agent join tokens for enrollment
CREATE TABLE join_tokens (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    token_hash      TEXT NOT NULL,
    scope           TEXT NOT NULL CHECK (scope IN ('platform', 'tenant', 'project', 'pipeline')),
    scope_id        UUID,                             -- NULL for platform scope
    max_uses        INT,                              -- NULL = unlimited
    current_uses    INT NOT NULL DEFAULT 0,
    labels          TEXT[] NOT NULL DEFAULT '{}',
    pool_tags       TEXT[] NOT NULL DEFAULT '{}',
    expires_at      TIMESTAMPTZ,
    revoked         BOOLEAN NOT NULL DEFAULT FALSE,
    created_by      UUID NOT NULL REFERENCES users(id),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Registered agents
CREATE TABLE agents (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id           UUID NOT NULL REFERENCES tenants(id),
    hostname            TEXT NOT NULL,
    os                  TEXT NOT NULL,
    arch                TEXT NOT NULL,
    labels              TEXT[] NOT NULL DEFAULT '{}',
    pool_tags           TEXT[] NOT NULL DEFAULT '{}',
    environment_type    TEXT NOT NULL CHECK (environment_type IN ('physical', 'virtual', 'container')),
    container_runtime   TEXT,
    kernel_version      TEXT,
    public_ips          INET[] NOT NULL DEFAULT '{}',
    private_ips         INET[] NOT NULL DEFAULT '{}',
    status              TEXT NOT NULL DEFAULT 'online'
                        CHECK (status IN ('online', 'busy', 'draining', 'offline', 'dead', 'revoked')),
    x509_public_key     BYTEA NOT NULL,               -- agent's long-term identity key
    join_token_id       UUID NOT NULL REFERENCES join_tokens(id),
    jwt_expires_at      TIMESTAMPTZ NOT NULL,
    jwt_renewable       BOOLEAN NOT NULL DEFAULT TRUE,
    last_heartbeat_at   TIMESTAMPTZ,
    registered_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    deregistered_at     TIMESTAMPTZ,
    version             TEXT                           -- agent binary version
);

CREATE INDEX idx_agents_tenant_status ON agents(tenant_id, status);
CREATE INDEX idx_agents_pool_tags ON agents USING GIN(pool_tags);
CREATE INDEX idx_agents_last_heartbeat ON agents(last_heartbeat_at) WHERE status IN ('online', 'busy');

-- Agent heartbeat history (for diagnostics, short retention)
CREATE TABLE agent_heartbeats (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id        UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    status          TEXT NOT NULL,
    cpu_percent     REAL,
    memory_percent  REAL,
    disk_percent    REAL,
    current_job_id  UUID,
    recorded_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_heartbeats_agent_time ON agent_heartbeats(agent_id, recorded_at DESC);

-- Job assignments (which agent picked up which job)
CREATE TABLE job_assignments (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    job_id          UUID NOT NULL,                    -- references jobs table from engine schema
    agent_id        UUID NOT NULL REFERENCES agents(id),
    status          TEXT NOT NULL DEFAULT 'accepted'
                    CHECK (status IN ('accepted', 'running', 'succeeded', 'failed', 'cancelled', 'timed_out')),
    accepted_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    started_at      TIMESTAMPTZ,
    completed_at    TIMESTAMPTZ,
    exit_code       INT,
    failure_reason  TEXT,
    attempt         INT NOT NULL DEFAULT 1            -- retry attempt number
);

CREATE INDEX idx_job_assignments_job ON job_assignments(job_id);
CREATE INDEX idx_job_assignments_agent ON job_assignments(agent_id) WHERE status IN ('accepted', 'running');

-- Executed binaries tracked during job execution (for security audit)
CREATE TABLE job_executed_binaries (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    job_id              UUID NOT NULL,                    -- references jobs table from engine schema
    job_assignment_id   UUID NOT NULL REFERENCES job_assignments(id) ON DELETE CASCADE,
    binary_path         TEXT NOT NULL,
    sha256_checksum     TEXT NOT NULL,                    -- hex-encoded SHA256
    execution_count     INT NOT NULL DEFAULT 1,
    first_executed_at   TIMESTAMPTZ NOT NULL,
    last_executed_at    TIMESTAMPTZ NOT NULL,
    step_ids            UUID[] NOT NULL DEFAULT '{}',     -- which steps executed this binary
    UNIQUE(job_assignment_id, binary_path, sha256_checksum)
);

CREATE INDEX idx_job_executed_binaries_job ON job_executed_binaries(job_id);
CREATE INDEX idx_job_executed_binaries_sha256 ON job_executed_binaries(sha256_checksum);

-- Job execution metadata summary
CREATE TABLE job_execution_metadata (
    id                      UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    job_id                  UUID NOT NULL,
    job_assignment_id       UUID NOT NULL REFERENCES job_assignments(id) ON DELETE CASCADE,
    total_processes_spawned BIGINT NOT NULL DEFAULT 0,
    execution_tree_depth    INT NOT NULL DEFAULT 0,
    unique_binaries_count   INT NOT NULL DEFAULT 0,
    recorded_at             TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(job_assignment_id)
);

CREATE INDEX idx_job_execution_metadata_job ON job_execution_metadata(job_id);
```

---

## 11. Cross-Platform Build Strategy

### 11.1 Supported Targets


| Platform      | Target Triple                | Container Support         | Priority |
| ------------- | ---------------------------- | ------------------------- | -------- |
| Linux AMD64   | `x86_64-unknown-linux-musl`  | Full (containerd, docker) | P0       |
| Linux ARM64   | `aarch64-unknown-linux-musl` | Full (containerd, docker) | P0       |
| macOS ARM64   | `aarch64-apple-darwin`       | None (native backend)     | P1       |
| Windows AMD64 | `x86_64-pc-windows-msvc`     | None (native backend)     | P2       |


Using `musl` for Linux targets produces fully static binaries with zero runtime dependencies -- critical for deploying agents into minimal container images or bare-metal hosts.

### 11.2 CI Cross-Compilation

- Use `cross` (cross-rs) for building non-native targets in CI.
- macOS builds require a macOS runner (GitHub Actions or self-hosted).
- Windows builds use the MSVC toolchain on a Windows runner.
- Release artifacts: single static binary per platform + Docker image for Linux targets.

### 11.3 Platform Abstraction

Platform-specific code is isolated behind feature flags and compile-time `#[cfg]` blocks:

```rust
// In met-agent/src/backend/mod.rs
#[cfg(target_os = "linux")]
mod container;
#[cfg(target_os = "linux")]
pub use container::ContainerBackend;

#[cfg(any(target_os = "macos", target_os = "windows"))]
mod native;
#[cfg(any(target_os = "macos", target_os = "windows"))]
pub use native::NativeBackend;

pub fn default_backend() -> Box<dyn ExecutionBackend> {
    #[cfg(target_os = "linux")]
    { Box::new(ContainerBackend::new()) }

    #[cfg(target_os = "macos")]
    { Box::new(NativeBackend::new_macos()) }

    #[cfg(target_os = "windows")]
    { Box::new(NativeBackend::new_windows()) }
}
```

---

## 12. Integration Test Strategy

### 12.1 Test Infrastructure

- **NATS**: Use `nats-server` binary in tests (download during CI, or use testcontainers).
- **PostgreSQL**: Use `testcontainers` crate for ephemeral Postgres instances.
- **gRPC**: In-process `tonic` server/client for unit tests; separate processes for integration tests.

### 12.2 Key Test Scenarios


| Test                                  | Description                                                                                            |
| ------------------------------------- | ------------------------------------------------------------------------------------------------------ |
| `test_agent_registration`             | Agent registers with valid join token, receives agent_id and JWT.                                      |
| `test_registration_invalid_token`     | Registration rejected for expired/revoked/invalid join token.                                          |
| `test_registration_scope_enforcement` | Agent with project-scoped token cannot subscribe to other projects' subjects.                          |
| `test_heartbeat_liveness`             | Agent sends heartbeats; controller tracks liveness correctly.                                          |
| `test_heartbeat_timeout_reaper`       | Agent stops heartbeating; controller marks dead and requeues job.                                      |
| `test_job_dispatch_nats`              | Scheduler publishes job to NATS; agent picks it up and acks.                                           |
| `test_job_pki_exchange`               | Agent generates keypair, exchanges with controller, receives encrypted secrets, decrypts and verifies. |
| `test_job_execution_container`        | (Linux only) Step executes in a container, stdout/stderr captured.                                     |
| `test_job_execution_native`           | Step executes as a native process.                                                                     |
| `test_job_cancellation`               | Cancel signal via NATS; agent receives and aborts in-progress job.                                     |
| `test_agent_drain`                    | Controller sends DRAIN; agent finishes current job and stops accepting new ones.                       |
| `test_agent_revocation`               | Admin revokes agent; next heartbeat returns TERMINATE.                                                 |
| `test_concurrent_agents`              | Multiple agents in same pool; each gets distinct jobs (no double-dispatch).                            |
| `test_agent_reconnect`                | Agent loses NATS connection, reconnects, and resumes pulling jobs.                                     |
| `test_process_watcher_basic`          | Process watcher tracks direct child process and computes correct SHA256 checksum.                      |
| `test_process_watcher_nested`         | Process watcher tracks nested child processes (grandchildren, etc.) in process tree.                   |
| `test_process_watcher_binary_caching` | Same binary executed multiple times is only hashed once; execution count incremented correctly.        |
| `test_execution_metadata_reported`    | Job completion report includes accurate execution metadata with all executed binaries.                 |
| `test_execution_metadata_persisted`   | Execution metadata is correctly persisted to `job_executed_binaries` and `job_execution_metadata`.     |
| `test_container_process_watching`     | (Linux only) Process watcher correctly tracks processes inside containers from outside namespace.      |


### 12.3 Property-Based Tests

Use `proptest` for:

- Secret encryption/decryption round-trip with random payloads.
- Job message serialization/deserialization fuzz.
- NATS subject construction from arbitrary tenant/pool combinations.

---

## 13. Crate Dependency Map

```
met-agent
  ├── met-core          (shared types, errors, config)
  ├── met-secrets       (per-job PKI, zeroize)
  ├── met-telemetry     (tracing, metrics)
  ├── met-logging       (log shipping)
  ├── met-objstore      (artifact upload/download)
  ├── tonic             (gRPC client)
  ├── async-nats        (NATS client)
  ├── rcgen             (X509 keypair generation)
  ├── sha2              (SHA256 binary checksums)
  ├── hex               (hex encoding for checksums)
  ├── aya               (eBPF for Linux process watching)
  ├── tokio             (async runtime)
  ├── clap              (CLI)
  └── serde / serde_json

met-controller
  ├── met-core
  ├── met-store         (database access)
  ├── met-secrets       (secret encryption with agent pubkeys)
  ├── met-telemetry
  ├── tonic             (gRPC server)
  ├── async-nats        (NATS publisher)
  ├── jsonwebtoken      (JWT issuance/validation)
  ├── bcrypt            (join token hashing)
  ├── tokio
  └── serde / serde_json

met-operator
  ├── met-core
  ├── met-telemetry
  ├── kube / kube-derive / kube-runtime  (K8s operator framework)
  ├── k8s-openapi       (K8s API types)
  ├── async-nats        (queue depth metrics)
  ├── tokio
  └── serde / serde_json / serde_yaml
```

---

## 14. Build Order (Within Phase 1)


| Step | Work                                                                                                                       | Depends On            |
| ---- | -------------------------------------------------------------------------------------------------------------------------- | --------------------- |
| 1.0  | Protobuf definitions (`proto/agent/v1/*.proto`)                                                                            | Phase 0 proto tooling |
| 1.1  | Agent DB schema (migrations for `join_tokens`, `agents`, `agent_heartbeats`, `job_assignments`)                            | Phase 0 DB foundation |
| 1.2  | `met-controller` scaffolding: gRPC server skeleton, join token CRUD, agent registry                                        | 1.0, 1.1              |
| 1.3  | `met-agent` scaffolding: config loader, gRPC client, registration flow                                                     | 1.0                   |
| 1.4  | NATS integration: subject design, JetStream stream/consumer setup, job dispatch publisher (controller), subscriber (agent) | 1.2, 1.3              |
| 1.5  | Heartbeat loop (agent + controller), health monitor, reaper                                                                | 1.4                   |
| 1.6  | Per-job PKI: keypair generation (agent), encryption (controller), exchange RPC                                             | 1.4                   |
| 1.7  | Container execution backend (Linux): workspace setup, container lifecycle, log capture                                     | 1.4                   |
| 1.8  | Native execution backend (macOS/Windows): process runner, environment scrubbing                                            | 1.4                   |
| 1.9  | Agent drain / revocation / graceful shutdown                                                                               | 1.5                   |
| 1.10 | Operating environment validation                                                                                           | 1.2                   |
| 1.11 | Process watcher: eBPF (Linux), Endpoint Security (macOS), ETW (Windows), binary SHA256 hashing                             | 1.7, 1.8              |
| 1.12 | `met-operator`: CRDs, reconciler, auto-scaler                                                                              | 1.5, 1.4              |
| 1.13 | Integration tests                                                                                                          | 1.0 -- 1.12           |
| 1.14 | Cross-platform CI build targets                                                                                            | 1.3                   |


---

## 15. Open Questions and Decisions


| Question                                                                  | Current Leaning                                                                                                                | Notes                                                                                                                           |
| ------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------- |
| Should agents support running multiple concurrent jobs?                   | Yes, configurable `concurrency` setting (default 1).                                                                           | Multi-slot agents reduce pod overhead in K8s.                                                                                   |
| NATS auth: per-agent credentials or shared per-pool?                      | Per-agent (issued during registration).                                                                                        | More secure, enables per-agent revocation. Adds complexity to credential management.                                            |
| Agent update mechanism?                                                   | Out of scope for Phase 1. Agents report their version; operator can roll new image tags. Bare-metal agents updated externally. | Revisit in Phase 4 (CLI could support `met agent update`).                                                                      |
| Should the controller be a separate binary or embedded in the API server? | Separate binary.                                                                                                               | Allows independent scaling. Controller needs to be highly available; could run multiple replicas behind the same gRPC endpoint. |
| DinD vs socket mount as default for K8s?                                  | DinD as default for Kubernetes (security); socket mount as default for bare-metal/VM (performance).                            | Multi-tenant K8s clusters require isolation. Socket mount is opt-in for trusted single-tenant clusters.                         |
| Process watcher implementation?                                           | eBPF only. Require kernel 4.15+ for Linux agents.                                                                              | eBPF provides low-overhead process tracing. Older kernels are not supported for process watching.                               |
| Should binary checksums be validated against an allowlist?                | Optional per-tenant policy, not enforced by default.                                                                           | Allowlist management adds significant operational complexity. Start with logging-only, add enforcement as opt-in later.         |


