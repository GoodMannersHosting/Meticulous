# ADR-014: Workspace snapshots and soft affinity

**Status:** Accepted — passive affinity snapshots + explicit per-invocation `workspace:` (`from` / `outputs`) implemented in parser, engine dispatch, and agent packing (2026-04-13).  
**Date:** 2026-04-11  
**PRDs:** [030](../prd/030-pipeline-authoring-dag-workflows.md), [040](../prd/040-scheduling-and-nats-dispatch.md)  
**Authoring guide:** [Pipeline authoring](../pipeline-authoring.md) (workflows, workspaces, global vs project scope).

## Context

Cross-workflow jobs today require hard agent affinity (`share-workspace: true`) to share filesystem state. This forces all related jobs onto the same agent, preventing horizontal scaling and creating a single point of failure. If the pinned agent goes offline, the entire pipeline stalls.

The plan calls for **workspace snapshots**: upload workspace state to S3 after a producing job completes, then download and restore it on any agent before a consuming job starts. Agent affinity becomes a soft scheduling preference rather than a hard requirement.

## Decision

### YAML surface

Add an optional `workspace:` block to workflow invocations:

```yaml
workflows:
  - id: build
    workflow: project/build
  - id: test
    workflow: project/test
    depends-on: [build]
    workspace:
      from: build              # restore snapshot from this invocation's terminal job
      outputs:                 # optional: only these paths are packed when this invocation uploads
        - target/
        - .cache/
```

`from` references a prior **`workflows[].id`** within the same pipeline. The parser resolves it to that invocation’s **unique terminal job** (the job in that invocation that no other job in the same invocation depends on). If that invocation has **multiple** terminal jobs (ambiguous DAG), validation fails with **`E5006`**. The consumer invocation must **`depends-on`** the producer workflow so the terminal job appears in `depends-on` after expansion (otherwise **`E5006`**).

`outputs` lists paths **relative to the workspace root** to include in the tarball when this job run uploads a snapshot. When **empty**, packing uses the full tree (same rules as passive mode: gitignore-aware walk, `.git/` excluded). Non-empty `outputs` restricts packing to files under those prefixes (agent-side).

### Passive mode vs explicit `workspace:` (summary)

| | **Passive (affinity + S3)** | **Explicit `workspace:`** |
|--|-----------------------------|---------------------------|
| **Trigger** | `share_workspace: true` + explicit `affinity-group` + engine snapshot config + S3 | Optional `workspace:` on an invocation; works together with passive mode |
| **What is archived** | Entire workspace tree (see exclusions below) unless `outputs` lists a non-empty subset | Same: empty `outputs` → full tree; non-empty → subset only |
| **Consumer restore source** | Engine picks **maximal in-group predecessor** from `depends_on` (same affinity group), if unique | If `workspace.from` is set, restore uses that invocation’s **terminal job**’s snapshot; otherwise passive rule applies |
| **Object key** | `…/workspace-snapshots/{run_id}/{producer_job_run_id}.tar.zst` | Same object layout; explicit `from` only changes **which** producer job id is used for restore |
| **Per-job workspace dir** | Yes (`workspace_root_id` cleared; each job run gets its own directory, restored from blob) | Same |

**Fan-in:** If a job depends on several in-group predecessors and passive selection would be ambiguous, there is **no** passive restore. Set **`workspace.from`** to the invocation whose tree you need (you still depend on that workflow in `depends-on`). Parallel branches that both mutate the workspace are not merged; you choose **one** snapshot source.

**Parallel jobs in one affinity group** are rejected by the parser unless **`agent-affinity.allow-parallel-shared-workspace-jobs: true`** (opt-in for S3-isolated per-`job_run_id` workspaces). See [Pipeline authoring](../pipeline-authoring.md).

Passive mode does **not** require `workspace:` on the YAML; explicit fields refine restore source and upload size.

### Passive mode (no extra YAML): `share_workspace` + object storage

**Gating (met-api):** Passive snapshots are **on** when an object-store client is configured **and** `MET_WORKSPACE_SNAPSHOTS_DISABLED` is **not** set to a truthy value (`1`, `true`, `yes`). If snapshots are off (no S3 or disabled flag), the engine keeps **shared-directory** semantics: `workspace_root_id` + stable directory name per affinity group.

When passive snapshots are **on**, jobs with **`share_workspace: true`** and an explicit **`affinity-group`** automatically:

1. Use a **per-`job_run_id` workspace directory** (engine omits `workspace_root_id` for these jobs).
2. **Restore:** If a **workspace snapshot predecessor** exists (see below), `JobDispatch.workspace_restore` carries a presigned **GET** URL, `expected_sha256`, and provenance (`producing_job_run_id`, `producing_workflow_invocation_id`, `archive_sha256`, `snapshot_generation`). The agent downloads, verifies the digest, and extracts into the workspace root before steps.
3. **Upload:** `JobDispatch.workspace_snapshot_upload` carries a presigned **PUT** URL, `object_key`, and `max_bytes` (compressed cap, default 10 GiB in `WorkspaceSnapshotConfig`). After **successful** steps, the agent packs the workspace, uploads, and reports **`WorkspaceSnapshotUploadResult`** on the terminal status update. Upload failure fails the job; the engine does not register a snapshot, so consumers that need that predecessor are not dispatched with a stale assumption.
4. **Registry:** The engine keeps a **run-scoped** map from producer **job id** → `{ object_key, sha256, size_bytes, producer_job_run_id, workflow_invocation_id, generation }`. A monotonic **`snapshot_generation`** is assigned per registration to help consumers detect unexpected replays.

**Predecessor selection:** Among `depends_on` edges to jobs in the **same** `(share_workspace, affinity_group)` class, the engine chooses the **maximal** predecessor (the one that runs “latest” among those dependencies—formally: the candidate reachable from every other candidate via the DAG). If there is no in-group predecessor, the job starts from an **empty** workspace (after normal checkout/bootstrap steps).

**Scheduling note:** Affinity groups are still **pinned** to the first agent that ran a job in that group; later jobs target that agent. If the pinned agent is unavailable or no longer matches pool/tags, dispatch fails with an affinity error—**passive snapshots do not, by themselves, reroute work to another agent** in the current scheduler. Snapshots ensure **filesystem continuity** (checkout → build) using object storage + per-job directories, and they preserve the option to relax pinning in a future change.

**Configuration (operators):**

| Variable | Where | Meaning |
|----------|--------|---------|
| `MET_WORKSPACE_SNAPSHOTS_DISABLED` | met-api | If `1` / `true` / `yes`, passive snapshots off; shared disk path used when applicable. |
| `MET_WORKSPACE_SNAPSHOT_TTL_HOURS` | met-api | Hint for lifecycle design: default **24**, clamped **1–168** (7d). Stored in engine `WorkspaceSnapshotConfig.object_ttl_hours`; **S3 does not read this**—configure lifecycle on the bucket prefix to match. |

Presign TTLs default to **1 hour** each for GET and PUT (`WorkspaceSnapshotConfig`); if a job waits longer than that in a queue, the engine must **re-dispatch** with fresh URLs (normal dispatch path).

### Snapshot format and storage

1. **Archive:** `tar` stream compressed with **zstd** (level **3** in the agent). The agent walks the workspace with the Rust **`ignore`** crate (`WalkBuilder::standard_filters(true)`), which honors **`.gitignore`** (and related standard ignore rules) from the workspace root. Entries under **`.git/`** are **never** included (large, not needed for typical build trees). **Non-file** entries (directories as separate tar members, devices, etc.) are skipped as appropriate; symlinks are **not** followed when packing.
2. **Extract:** The consumer uses `tar` **`unpack_in`** with path checks: **absolute** paths and **`..`** components in archive members are rejected.
3. **Object key (passive):** `<org[/project] prefix>/workspace-snapshots/{run_id}/{producer_job_run_id}.tar.zst` via `ObjectKeyBuilder::workspace_snapshot_job_run` (`crates/met-objstore/src/paths.rs`). Using **`job_run_id`** avoids collisions when a logical job is retried with a new run id.
4. **Size limits:** `WorkspaceSnapshotUploadSpec.max_bytes` caps the **compressed** archive; the agent also tracks **uncompressed** bytes while tarring and can fail early if an uncompressed cap is exceeded (implementation uses the same budget for streaming totals). Exceeding the cap fails the upload path with a clear error.
5. **Download:** Presigned GET on `JobDispatch.workspace_restore`, TTL from `WorkspaceSnapshotConfig.presign_get_ttl` (default 1h).

**Repositories without `.git`:** The same walker still applies ignore files present in the tree; there is no separate documented deny list beyond ignore rules and the hard `.git/` exclusion. Teams should rely on `.gitignore` or keep large artifacts outside the workspace. (Org-level exclude globs are a possible future extension.)

### Proto surface (`controller.proto` / agent types)

- **`WorkspaceSnapshot`** on dispatch: download URL, expected SHA-256, optional restore path list, provenance fields (`producing_job_run_id`, `producing_workflow_invocation_id`, `archive_sha256`, `snapshot_generation`).
- **`WorkspaceSnapshotUploadSpec`** on `JobDispatch`: presigned PUT URL, `object_key`, `max_bytes`, optional **`include_paths`** (subset pack; mirrors YAML `workspace.outputs`).
- **`WorkspaceSnapshotUploadResult`** on **`JobCompletion`** and **`JobStatusUpdate`**: `uploaded`, `sha256`, `size_bytes`, `object_key`, `skipped`, `error_message`.

### Digest verification

The producing agent computes a SHA-256 digest of the archive before upload and reports it to the engine via the job completion message. The engine stores this digest and includes it as `expected_sha256` in the consuming job's `WorkspaceSnapshot`. The consuming agent verifies the digest after download before extraction. A mismatch aborts the job with a clear error referencing the producing invocation.

### Scheduling and affinity

1. **Affinity pin:** For jobs with `affinity-group`, the first dispatched job picks an available agent; the run state **pins** that group to that agent. Later jobs in the group must use the pinned agent or dispatch fails (see `scheduler.rs`).
2. **Passive snapshots** complement that model by **restoring** the previous job’s workspace into each new **`job_run_id`** directory so serial jobs see the same tree without relying on a shared path on disk.
3. **Explicit `workspace:`** is **opt-in** for subset uploads (`outputs`), unambiguous restore (`from`) when passive predecessor selection is insufficient, or both; it does not replace passive mode for simple linear affinity chains.

### Agent behavior

**Passive producer** (`workspace_snapshot_upload` present and not skipped):

1. Run steps in the (possibly restored) workspace.
2. On success, pack workspace (`tar` + zstd), compute SHA-256, `PUT` to presigned URL.
3. Emit `WorkspaceSnapshotUploadResult` on the terminal update so the controller/engine can register the snapshot.

**Passive consumer** (`workspace_restore` populated):

1. `GET` blob, verify **`expected_sha256`**, extract with traversal guards, then run steps.

**Explicit YAML producer** (invocation declares `workspace.outputs` non-empty):

1. After steps, archive **only** declared path prefixes into `tar.zst`, then upload and report digest (same as passive otherwise).

**Explicit YAML consumer** (invocation declares `workspace.from`):

1. Engine selects the registered snapshot for the **resolved terminal job** of that invocation; agent downloads, verifies digest, extracts, runs steps.

### Engine orchestration

**Passive mode** (`crates/met-engine/src/workspace_snapshots.rs`, `scheduler.rs`, run state):

- Compute `workspace_snapshot_predecessor` from `PipelineIR` for each `share_workspace` job.
- On dispatch: if predecessor exists, require a **registered** snapshot for that predecessor’s job id (or fail with a workspace-snapshot error if the predecessor succeeded but registration is missing). Presign GET and fill `WorkspaceSnapshot` provenance.
- Always attach presign **PUT** for passive snapshot producers (same chain).
- On completion: parse `workspace_snapshot_result`; if upload succeeded, **register** `WorkspaceSnapshotRecord` under the **producer job id** for this run.

**Explicit `workspace:` mode:** snapshot registry remains keyed by **producer job id** (per run). The parser fills `restore_from_job_id` from `workspace.from`. Dispatch uses **`restore_from_job_id` when set**, otherwise **`workspace_snapshot_predecessor`** (passive).

**Retries:** A new **`job_run_id`** implies a new object key (`workspace_snapshot_job_run`); consumers must not read an object keyed for a **previous** attempt once the engine registers the new blob. Partial uploads should not be registered; lifecycle rules eventually expire abandoned objects.

## Consequences

### Positive

- Jobs can run on any agent, improving fleet utilization and resilience.
- No changes to the per-job PKI or secret delivery model.
- Snapshot archives are naturally auditable (S3 access logs, object versioning).

### Negative

- Additional S3 bandwidth and storage costs proportional to workspace size.
- Snapshot upload/download adds latency compared to same-agent workspace reuse (mitigated by soft preference).
- Requires S3-compatible storage to be configured; agents without S3 access cannot participate in snapshot transfers.

### Migration

- Passive mode requires **current agents** that implement `workspace_restore`, `workspace_snapshot_upload`, and `workspace_snapshot_result` reporting; older agents will not populate snapshots correctly.
- No DB migration required; snapshot metadata is transient per-run.
- `ObjectKeyBuilder` gains `workspace_snapshot_job_run`; the earlier invocation-scoped key remains for explicit YAML-mode.

### Operations (S3 lifecycle and cost)

- **Prefix:** `{org base}/workspace-snapshots/{run_id}/` (see `ObjectKeyBuilder`).
- **Retention:** Add a bucket **lifecycle rule** that expires objects under `workspace-snapshots/` after **N days** (or transitions to Glacier if desired). Match **N** to org policy: default narrative is **24 hours**; **7 days** is the documented upper bound for TTL hint (`MET_WORKSPACE_SNAPSHOT_TTL_HOURS` ≤ 168).
- **Metadata:** Operators *may* set object tags or `x-amz-meta-*` in future for auditing; the engine’s `object_ttl_hours` is for **documentation and future hooks**, not automatic S3 expiry.
- **Presigned URLs:** Short-lived (default 1h); independent of object lifetime. Long-queued jobs need **redispatch** with fresh URLs.
- **Cost:** Charged for storage, PUT/GET, and egress; gitignore-aware packing and excluding `.git/` reduce average object size.

## Threat model

- **Assets:** Workspace file contents (may include build artifacts, source code, test data); presigned URLs.
- **Adversaries:** Compromised agent tampering with snapshot contents; network observer intercepting presigned URLs; replay of stale snapshots.
- **Mitigations:**
  - SHA-256 digest verified by consumer agent before extraction; dispatch carries provenance (`producing_job_run_id`, generation) so the consumer can reject mismatched metadata.
  - Presigned URLs are short-lived (1 hour) and scoped to the specific object key.
  - Snapshots are scoped to `{org_id}/{run_id}` — cross-org access requires guessing a UUID + valid presigned signature.
  - `tar` extraction rejects absolute paths and path traversal (`../`).
  - Snapshot size limits prevent storage exhaustion.
- **Residual risk:** A compromised producing agent can inject malicious files into the snapshot. Mitigation requires signing snapshots with the per-job key (deferred to a future enhancement).

**Certificates:** Not directly applicable. If S3 uses custom TLS trust stores, verify certificates per workspace rules.

## References

- [ADR-004](004-secrets-and-per-job-pki.md) — per-job PKI (snapshot transfer does not carry secrets)
- [ADR-002](002-nats-subjects-and-envelopes.md) — NATS dispatch; completion messages carry digest
- [`crates/met-objstore/src/paths.rs`](../../crates/met-objstore/src/paths.rs) — object key conventions
- [`crates/met-engine/src/scheduler.rs`](../../crates/met-engine/src/scheduler.rs) — dispatch orchestration
- [`crates/met-agent/src/executor.rs`](../../crates/met-agent/src/executor.rs) — agent-side job execution
- [Platform evolution plan](../../.cursor/plans/platform_evolution_plan_a66c44a0.plan.md) — Feature 3
