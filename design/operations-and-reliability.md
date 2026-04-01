# Operations and reliability

Complements [constraints.md](constraints.md) with deployment-facing decisions. Not an ADR; split into ADRs when a single choice must be frozen.

## Control plane

- **Postgres:** Automated backups, PITR where available, restore drills. Migrations applied with rolling API/controller strategy; document incompatible migrations as downtime windows.
- **NATS JetStream:** Persistent volumes, stream replication per vendor docs, monitored disk and consumer lag. Align stream names with [ADR-002](adr/002-nats-subjects-and-envelopes.md).
- **API and controller:** Horizontally scaled stateless instances behind HTTP/2-capable load balancer; sticky sessions **not** required if JWT validates on every request.

## Webhooks and ingress

- Terminate TLS at edge; limit request body size for webhook routes; rate limit per `trigger_id` or IP at gateway (see [ADR-005](adr/005-scm-webhook-security.md)).

## Object storage

Separate buckets per concern, each with its own lifecycle policy:

| Bucket          | Default retention   | Transition                    | WORM / Object Lock                                                                                               |
| --------------- | ------------------- | ----------------------------- | ---------------------------------------------------------------------------------------------------------------- |
| `met-artifacts` | 90 days             | Intelligent-Tiering after 30d | Not required; optional Governance mode                                                                           |
| `met-logs`      | 30 days             | IA after 7d                   | Not required by SOC 2 / ISO 27001; HMAC-SHA256 chain is sufficient                                               |
| `met-audit`     | 1 year              | Glacier after 90d             | **Governance mode recommended** for production (supports SOC 2 evidence without Compliance-mode irreversibility) |
| `met-sboms`     | **3 years minimum** | Glacier/IA after 180d         | Governance mode recommended; soft-delete only (tombstone + operator confirmation)                                |

SBOMs must **never** share the artifact lifecycle — they must outlive their corresponding artifacts to serve CVE blast-radius queries. The `sbom_reports.s3_key` column in Postgres remains queryable after the artifact is purged.

Build log WORM status: SOC 2 Type II and ISO 27001 A.8.15 do not mandate WORM for build logs; tamper-evidence via HMAC-SHA256 per `LogChunk` is sufficient. Audit logs (`audit_log` table + `met-audit` bucket) warrant stronger protection.

## Log streaming and buffering

- **Agent-side buffer:** 512-line bounded Tokio channel per step streaming task. Batch 50–100 lines or flush every 100 ms (whichever comes first) before gRPC send.
- **Overflow policy:** Drop-oldest with a `[N lines dropped due to backpressure]` sentinel. Preserves recent context; never drop audit events.
- **Local WAL:** 50 MB per job on agent disk for reconnection recovery; gzip compressed.
- **`StreamLogs` gRPC stream:** one stream per job, with a `step_index` field for demultiplexing steps. Do not open one gRPC stream per step.

## Container security defaults

- Job containers run with the containerd `RuntimeDefault` seccomp profile.
- `CAP_SYS_PTRACE` is dropped from the container capability bounding set by default. Opt-in for steps that declare `debug: true`; grant is logged as an audit event.
- PID namespace isolation prevents cross-container `ptrace`; no extra seccomp rule needed for that case.

## Upgrades

- Agent binary version skew: support **N-1** protocol compatibility or enforce minimum agent version via controller handshake.

## NATS operations

- Credentials rotation: when an agent JWT is renewed (ADR-003), the controller also rotates that agent's NATS credential (NKey or user JWT). Old credentials are invalidated by removing the user from the NATS server configuration or letting the credential TTL expire (set to `agent_jwt_ttl + 5min`).
- Stream recovery: if a JetStream stream is lost (disk failure), re-create it from the ADR-002 configuration; any `queued` jobs in Postgres with no corresponding NATS message are re-published by a startup reconciliation pass in the controller.
- Consumer lag alert: alert if any pool consumer lag > 100 unacknowledged messages for > 5 minutes (indicates agents are not keeping up or are offline).

## Deferred

- Multi-region active/active ([constraints.md](constraints.md)).
- Per-tenant quotas and billing meters (product roadmap).
- Pre-populated build-tool volumes (see [features.md](features.md) and [adr/README.md](adr/README.md) for future ADR).
