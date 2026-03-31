---
name: Codebase Completion Roadmap
overview: Address the 15 code TODOs, complete pending plan items, and wire up integration gaps to achieve a functional self-hosting CI/CD platform.
todos:
  - id: p1-job-status
    content: Wire ReportJobStatus gRPC handler to update job_runs/step_runs tables via met-store
    status: completed
  - id: p1-engine-callbacks
    content: Trigger met-engine DAG executor callbacks on job completion to advance dependent jobs
    status: completed
  - id: p1-job-requeue
    content: Implement job requeue via NATS when agent health monitor detects dead agent
    status: completed
  - id: p2-secret-encryption
    content: Implement per-job PKI encryption in controller (X25519 + AES-256-GCM via met-secrets)
    status: completed
  - id: p2-secret-decryption
    content: Implement secret decryption in agent executor with checksum verification and zeroize
    status: completed
  - id: p2-log-forwarding
    content: "Implement log pipeline: stream to PG cache + NATS, archive to SeaweedFS on job completion, lazy reload for old logs"
    status: pending
  - id: p3-source-spans
    content: Capture actual source locations in met-parser using serde_yaml Location API
    status: completed
  - id: p3-db-provider-test
    content: Create test infrastructure for DatabaseWorkflowProvider with sqlx::test
    status: completed
  - id: p4-oidc-wiring
    content: Wire met-secrets OIDC validator into met-api with OAuth callback routes
    status: completed
  - id: p4-group-mapping
    content: Implement OIDC group claim to Meticulous group auto-mapping on login
    status: completed
  - id: p4-provider-ui
    content: Create /admin/auth page for auth provider management and group mappings
    status: completed
  - id: p5-autoscaling
    content: Query NATS queue depth in operator reconciler for pod autoscaling decisions
    status: completed
  - id: p6-macos-watcher
    content: Implement macOS process watching via Endpoint Security or dtrace
    status: completed
  - id: p6-windows-watcher
    content: Implement Windows process watching via ETW
    status: pending
  - id: p7-token-validation
    content: Implement JWT token validation in frontend hooks.server.ts
    status: completed
  - id: p7-error-telemetry
    content: Wire frontend error handling to telemetry service in hooks.client.ts
    status: completed
  - id: p8-password-config
    content: Make password_enabled configurable in auth routes
    status: completed
isProject: false
---

# Codebase Completion Roadmap

Based on the scan, there are **15 code TODOs**, **36 pending plan items**, and several integration gaps between components. This plan prioritizes work to reach self-hosting capability first, then completes remaining features.

---

## Phase 1: Controller-Engine Integration (Critical Path)

The `met-controller` gRPC handlers are stubbed - they accept agent calls but don't complete the workflow. These block self-hosting.

### 1.1 Job/Step Status Persistence

**Files**: `crates/met-controller/src/grpc.rs`

```rust
// Line 372: TODO: Update job_runs table with status
// Line 397: TODO: Update step_runs table with status
```

- Wire `ReportJobStatus` handler to call `met-store` repositories
- Update `job_runs` and `step_runs` tables on status transitions
- Emit events to NATS for real-time updates

### 1.2 Engine Callbacks

**File**: `crates/met-controller/src/grpc.rs:373`

```rust
// TODO: Trigger pipeline engine callbacks
```

- On job completion, notify `met-engine` DAG executor
- Advance dependent jobs when predecessors complete
- Handle failure propagation (skip downstream jobs)

### 1.3 Job Requeue on Agent Failure

**File**: `crates/met-controller/src/health.rs:89`

```rust
// TODO: Requeue the job via NATS
```

- When health monitor detects dead agent, republish job to NATS
- Respect `MaxDeliver` limit from JetStream config
- Mark job as `Retrying` in database

---

## Phase 2: Secrets and Logging Pipeline

### 2.1 Secret Encryption (Per-Job PKI)

**File**: `crates/met-controller/src/grpc.rs:450`

```rust
// TODO: Implement actual secret encryption
```

- Integrate with `met-secrets` broker
- Encrypt secrets with agent's ephemeral public key (X25519 + AES-256-GCM)
- Include SHA-256 checksums for verification

**File**: `crates/met-agent/src/executor.rs:403`

```rust
// TODO: Implement actual decryption
```

- Decrypt secrets using job's ephemeral private key
- Verify checksums
- Inject into step environment
- Zeroize keys after use

### 2.2 Log Forwarding (Revised Architecture)

**File**: `crates/met-controller/src/grpc.rs:414`

```rust
// TODO: Forward logs to log storage / WebSocket fanout
```

**SeaweedFS as source of truth, PostgreSQL as 24h cache:**

#### During Job Execution:

- Stream log chunks to PostgreSQL `log_cache` table (temporary)
- Publish to NATS for WebSocket streaming to live viewers

#### On Job Completion:

```rust
async fn on_job_complete(job_id: JobId) {
    // 1. Fetch all log lines from PostgreSQL
    let logs = log_repo.get_all_for_job(job_id).await?;
    
    // 2. Compress to JSONL + gzip
    let compressed = compress_logs(&logs)?;
    
    // 3. Upload to SeaweedFS (source of truth)
    objstore.put(&format!("logs/{project}/{run}/{job}.jsonl.gz"), compressed).await?;
    
    // 4. Delete from PostgreSQL immediately
    log_repo.delete_for_job(job_id).await?;
}
```

#### Lazy Reload for Old Logs:

```rust
async fn get_logs(job_id: JobId, range: LineRange) -> Result<Vec<LogLine>> {
    // 1. Check PostgreSQL cache first
    if let Some(logs) = log_repo.get_cached(job_id, range).await? {
        return Ok(logs);
    }
    
    // 2. Cache miss - fetch from SeaweedFS
    let compressed = objstore.get(&log_path(job_id)).await?;
    let logs = decompress_logs(compressed)?;
    
    // 3. Load into PostgreSQL with 24h TTL
    log_repo.cache_with_ttl(job_id, &logs, Duration::hours(24)).await?;
    
    // 4. Return requested range
    Ok(logs.filter_range(range))
}
```

#### PostgreSQL Cache Schema:

```sql
CREATE TABLE log_cache (
    job_id      UUID NOT NULL,
    line_number BIGINT NOT NULL,
    timestamp   TIMESTAMPTZ NOT NULL,
    stream      TEXT NOT NULL,  -- stdout/stderr
    content     TEXT NOT NULL,
    cached_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at  TIMESTAMPTZ NOT NULL,  -- NOW() + 24h
    PRIMARY KEY (job_id, line_number)
);

CREATE INDEX idx_log_cache_expires ON log_cache(expires_at);
```

#### Background Cleanup:

- Cron job or Tokio task: `DELETE FROM log_cache WHERE expires_at < NOW()`

---

## Phase 3: Parser Completion

### 3.1 Source Location Tracking

**File**: `crates/met-parser/src/parser.rs:155,258`

```rust
let location = SourceLocation::new(1, 1); // TODO: capture actual location
```

- Use serde_yaml's `Location` API for line/column tracking
- Propagate spans through all parser stages
- Enable precise error messages with file:line:col

### 3.2 Database Provider Test Infrastructure

**File**: `crates/met-parser/src/providers/database.rs:165`

```rust
pool: todo!("need test pool"),
```

- Create test fixture with `sqlx::test` macro
- Add integration test for `DatabaseWorkflowProvider`

---

## Phase 4: Admin Portal Completion

Four pending items from the Admin Portal plan:

### 4.1 OIDC Validation Wiring

**Todo**: `phase3-oidc`

- Wire `met-secrets` OIDC validator into `met-api`
- Add OAuth callback routes (`/auth/{provider}/login`, `/auth/{provider}/callback`)
- Map OIDC claims to user creation/lookup

### 4.2 Group Mapping

**Todo**: `phase3-group-mapping`

- Extract groups from OIDC claims (`groups`, `roles`, `cognito:groups`)
- Sync memberships on login based on `oidc_group_mappings` table
- Support additive and full-sync modes

### 4.3 Provider Management UI

**Todo**: `phase3-provider-ui`

- Create `/admin/auth` page
- List providers with enable/disable toggle
- OIDC provider config form (issuer, client ID/secret)
- Group mapping management

---

## Phase 5: Operator Autoscaling

**File**: `crates/met-operator/src/reconciler.rs:207`

```rust
// TODO: Check NATS queue depth and adjust
```

- Query NATS JetStream for pending message count on pool subjects
- Compare against `AgentPoolAutoscaler` thresholds
- Scale agent pods up/down with stabilization windows

---

## Phase 6: Cross-Platform Agent Support (Lower Priority)

**Files**: `crates/met-agent/src/process_watcher.rs:340,365`

### 6.1 macOS Process Watching

- Implement using Endpoint Security framework or dtrace
- Binary SHA256 computation for supply chain tracking

### 6.2 Windows Process Watching

- Implement using ETW (Event Tracing for Windows)
- Job Objects for process enumeration

---

## Phase 7: Frontend Integration

**File**: `frontend/src/hooks.server.ts:7`

```typescript
// TODO: Validate token with backend and extract user info
```

- Implement token validation against `met-api`
- Extract user info from JWT claims
- Populate `locals.user` for downstream routes

**File**: `frontend/src/hooks.client.ts:9`

```typescript
// TODO: Send errors to telemetry service in production
```

- Integrate with OpenTelemetry or error tracking service
- Batch and send client-side errors

---

## Phase 8: Configuration and Polish

**File**: `crates/met-api/src/routes/auth.rs:89`

```rust
password_enabled: true, // TODO: Make this configurable
```

- Read from platform settings or org config
- Allow disabling password auth when OIDC is configured

---

## Plan Hygiene

Several plans have todos marked pending despite code existing:

- **Foundation & Scaffolding**: 12 todos pending, but workspace and crates exist. Review and mark complete.
- **Self-Hosting Milestone**: 5 todos pending. Update as integration gaps are closed.
- **Observability & Storage**: No todos defined. Add todos if features are incomplete.

---

## Dependency Graph

```
Phase 1 (Controller-Engine) ─┬─> Phase 2 (Secrets/Logs)
                             │
                             └─> Phase 3 (Parser)
                                      │
                                      v
                             Phase 4 (Admin Portal)
                                      │
                                      v
                             Phase 5 (Operator)
                                      │
                                      v
                             Phase 6 (Cross-Platform) ──> Phase 7 (Frontend)
                                                                │
                                                                v
                                                         Phase 8 (Polish)
```

---

## Estimated Scope


| Phase                            | Items | Priority      |
| -------------------------------- | ----- | ------------- |
| 1. Controller-Engine Integration | 3     | P0 - Blocking |
| 2. Secrets and Logging           | 3     | P0 - Blocking |
| 3. Parser Completion             | 2     | P1            |
| 4. Admin Portal                  | 3     | P1            |
| 5. Operator Autoscaling          | 1     | P2            |
| 6. Cross-Platform Agent          | 2     | P2            |
| 7. Frontend Integration          | 2     | P2            |
| 8. Configuration Polish          | 1     | P3            |


**Total: 17 distinct work items** (consolidating the 15 code TODOs + 4 admin portal pending items, with some overlap)