# ADR-006: Execution telemetry (binaries and network metadata)

**Status:** Proposed  
**Date:** 2026-03-31  
**PRDs:** [070](../prd/070-execution-telemetry-and-audit.md), [090](../prd/090-supply-chain-sbom-tools-blast-radius.md)

## Context

The agent already collects **executed binaries** with path and SHA-256 via [crates/met-agent/src/process_watcher.rs](../../crates/met-agent/src/process_watcher.rs) and attaches them to step reports in [executor.rs](../../crates/met-agent/src/executor.rs). Network flow metadata and a durable **tool index** for blast radius are not fully wired through the API/DB.

## Decision

1. **Canonical source for “what ran”** — Step completion reports carry `executed_binaries[]` with at least: `path`, `sha256`, `first_executed_at`, `last_executed_at`, `step_ids` (or equivalent proto fields). This is the **authoritative** feed for PRD 070 and for PRD 090’s tool database **ingestion**.

2. **Storage** — Persist per step run or job run in Postgres as JSONB or normalized child table (`executed_binary_events`). Retention and PII policy (paths may contain usernames) are deployment-configurable.

3. **Collection approach for executed binaries:**
   - **Primary:** `seccomp(SECCOMP_RET_USER_NOTIF)` — requires no elevated capabilities (`no_new_privs` + `prctl(PR_SET_SECCOMP, SECCOMP_MODE_FILTER, ...)`), cannot miss execs, synchronous per-exec notification. **Minimum kernel: Linux 5.0 (2019).** Use the `seccompiler` crate for filter compilation.
   - **eBPF tracepoint path:** `sys_enter_execve` tracepoint via eBPF requires `CAP_BPF + CAP_PERFMON` (Linux 5.8+). Use only when the agent runs with those capabilities (e.g. privileged K8s pods).
   - **Fallback:** `/proc/<pid>/task/<tid>/children` polling at 100ms intervals (current implementation). Documents the gap: processes completing in < 100ms are not captured. Acceptable only on platforms where seccomp-notif is unavailable.
   Capability detection must be at agent startup; log which mode is active.

4. **Network metadata (phase 2)** — Record `src_ip`, `dst_ip`, `dst_port`, `protocol`, `direction`, `step_id`, `timestamp` only—**no payloads**. Collection via `/proc/net/tcp` polling at 1-second intervals (acceptable; sub-second connections are not captured but the metadata-only requirement relaxes precision). eBPF-based collection (via `CAP_NET_ADMIN` + netfilter) is an upgrade path. Schema reserved in proto/DB before implementation.

5. **Tool database / blast radius (PRD 090)** — Derived tables or search index keyed by `sha256` + optional `(name, version)` from SBOM or package managers; **reindex** from stored executed-binary rows and SBOM blobs.

6. **RBAC** — Read telemetry with same roles as run logs (PRD 010); no cross-org queries.

## Consequences

- Proto may need stable field names for executed binaries if not already aligned with `process_watcher` types.
- High-cardinality explosion if storing every TCP connection; default sample or allowlist ports in phase 2.
- `PathRedactor` must be implemented in the agent before step completion reports are sent — redacted data must never transit the gRPC wire.
- `run_network_connections` table needs `retention_days` column at org or project level; a background sweeper enforces it.
- `sbom_reports` Postgres rows must be retained permanently (or until explicit deletion); only `sbom_components` rows rotate. Separate `met-sboms` bucket with a 3-year minimum lifecycle rule, independent of the `met-artifacts` bucket.

## Threat model

- **Assets:** Execution history reveals internal paths and dependency footprint.
- **Adversaries:** Tenant A reading tenant B’s telemetry via API bugs.
- **Mitigations:** Org scoping on all queries; audit read access for sensitive deployments.
- **Residual risk:** Sensitive paths in binaries list; consider path redaction rules.

## References

- [ADR-001](001-run-and-job-lifecycle.md) job/step identity
- [ADR-003](003-grpc-agent-control-plane.md) log and status streams
