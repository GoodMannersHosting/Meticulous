# PRD: Agent execution, logs, and artifacts

**Status:** Draft  
**Owner:** TBD  
**Last updated:** 2026-03-31  
**Related:** [../features.md](../features.md), [../architecture.md](../architecture.md), PRD 040, PRD 060, PRD 120

## Context

**met-agent** executes steps (containers on Linux; native where required), reports status over **gRPC**, and streams **logs** to the control plane ([../architecture.md](../architecture.md)). **Artifacts** and optional log segments land in **S3-compatible** storage with deployment **retention** policies ([../features.md](../features.md)).

## Problem statement

Operators cannot debug or release without reliable **live logs**, durable **artifacts**, and clear **step outcomes**; mishandling bloated logs or artifacts breaks cost and privacy expectations.

## Goals

- Execute steps in the declared environment with exit status and timing recorded.
- **Stream logs** to API/UI with **secret redaction** ([../pipelines.md](../pipelines.md)).
- Upload **artifacts** to object storage with immutable object keys or versioning policy.
- Support **retention** configuration per deployment (and future per-tenant TBD).

## Non-goals

- Storing full network payloads (metadata only in PRD 070).
- Replacing external log SIEM (export via PRD 080).

## Users and stakeholders

| Role | Need |
| --- | --- |
| Developer | See live and historical logs; download artifacts. |
| Agent runtime | Bounded memory for log buffering; resumable uploads. |
| FinOps | Lifecycle policies for object storage. |

## Functional requirements

| ID | Requirement | Priority | Notes |
| --- | --- | --- | --- |
| FR-1 | Step start/finish events and exit codes visible in API. | P0 | |
| FR-2 | Log streaming channel (WebSocket or equivalent) from agent to API. | P0 | UI: [../user-interface.md](../user-interface.md) Build Logs. |
| FR-3 | Redact secret-shaped and base64 secret values in log pipeline. | P0 | [../pipelines.md](../pipelines.md). |
| FR-4 | Artifact upload with content-addressable or run-scoped paths; multipart upload for objects > 5 MB. | P0 | Path: `met-artifacts/{org_id}/{project_id}/{run_id}/{job_run_id}/{name}`. Content-addressed by SHA-256 at upload time; duplicates deduplicated by hash before write. |
| FR-5 | Optional archive of log segments to object storage. | P1 | |
| FR-6 | Multi-layer **remote cache** integration (keys, isolation) | P1 | **Owns** execution-plane cache behavior; YAML surface in PRD **030**. TBD: [../open-questions.md](../open-questions.md). [OVERLAP-RESOLUTION.md](OVERLAP-RESOLUTION.md). |

## Non-functional requirements

| ID | Requirement | How verified |
| --- | --- | --- |
| NFR-1 | Log pipeline handles high-volume steps without OOM (backpressure). | Load test |
| NFR-2 | Large artifact multipart upload/resume. | Integration test |

## Security and privacy

- Logs and artifacts inherit **RBAC** from PRD 010. **Artifact download:** issue short-lived presigned S3 URLs (15-minute TTL) from the API rather than proxying bytes through the control plane. The API validates RBAC before generating the presigned URL; the URL itself carries no identity — any holder can download for the TTL window. Presigned URL generation is logged as an audit event.
- **Threats:** Log injection (mitigate with ANSI escape allowlist in the UI renderer per PRD 120); artifact tampering (SHA-256 content hash verified at upload and available via `GET /artifacts/{id}/checksum`); artifact path traversal (enforce `{name}` is URL-safe and does not contain `..`).

## Dependencies and assumptions

- **Depends on:** PRD 040 dispatch; PRD 060 secret injection to steps; object storage available.
- **Assumes:** Agent can reach object storage egress policy per deployment.

## Success metrics

| Metric | Target | Measurement |
| --- | --- | --- |
| Log delivery latency (P95) | < 2 s (log line written on agent → visible in UI) | OTel |
| Failed artifact uploads retried | Configurable success rate | Metrics |

## Rollout and migration

- Feature-flag log archival; tune buffer sizes per [../open-questions.md](../open-questions.md).

## Open questions

- Short-term log buffer sizing vs attestation volume ([../open-questions.md](../open-questions.md)).
- Tamper-evident / WORM logs ([../open-questions.md](../open-questions.md)).

## Out of scope / future work

- In-browser binary preview for arbitrary artifact types.
