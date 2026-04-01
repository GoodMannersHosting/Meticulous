# ADR-003: gRPC agent control plane

**Status:** Proposed  
**Date:** 2026-03-31  
**PRDs:** [050](../prd/050-agent-execution-logs-artifacts.md), [060](../prd/060-secrets-providers-and-per-job-pki.md), [110](../prd/110-kubernetes-operator-and-agent-fleet.md)

## Context

Agents **dial out** to the controller; all registration, heartbeats, job status, logs, and per-job key exchange use **gRPC** ([design/architecture.md](../architecture.md)). The service is already sketched in [proto/meticulous/agent/v1/agent.proto](../../proto/meticulous/agent/v1/agent.proto).

## Decision

1. **Service:** `meticulous.agent.v1.AgentService` remains the single agent-facing API: `Register`, `Heartbeat`, `ReportJobStatus`, `ReportStepStatus`, `StreamLogs`, `ExchangeJobKeys`, `Deregister`.

2. **Transport security:** Production requires **mTLS** — agent presents a client certificate signed by the platform intermediate CA (issued at registration and stored as `agent_cert_pem` in the agents table). TLS-only is permitted in local development only. Join token proves bootstrap identity during the initial `Register` call before a cert is issued; `RegisterResponse.jwt_token` authorizes subsequent RPCs. mTLS adds a second independent factor: a stolen JWT alone cannot impersonate an agent without also presenting its CA-signed cert. Load balancers must terminate mTLS (pass `X-Client-Cert-DN` downstream) or operate in TCP passthrough mode.

   **JWT renewal:** The proto already supports renewal via `HeartbeatResponse.new_jwt_token` / `new_jwt_expires_at` (agent.proto lines 95–97). The controller proactively pushes a new JWT in the heartbeat response when `exp − max(15min, 10% of TTL)` is reached server-side. The agent must apply the new token immediately on receipt. This is preferred over the agent calling `Register` again (which requires a new join token). Re-calling `Register` with the existing JWT is the fallback for agents that missed renewal through heartbeat (e.g., extended offline period). If renewal fails and the existing JWT expires, the agent stops pulling jobs and attempts re-enrollment with its join token (if not single-use). Long-lived agents (PRD 110 `renewable = true`) use `renewable` + approval policy from PRD 110.

3. **Streaming:** Log and status updates use **client-streaming** from agent to controller as in proto. The API server exposes logs to the UI via separate HTTP/WebSocket paths (PRD 050 / 120), fed from persistence or relay—not by exposing gRPC to browsers.

4. **Correlation:** Propagate W3C `traceparent` from the NATS `JobDispatch` message header into gRPC metadata (`traceparent` key) for OTel ([ADR-007](007-observability-opentelemetry.md), PRD 080).

5. **Reconnection policy:** Agents use exponential backoff with jitter: initial 1s, multiplier 2×, cap 60s, ±25% jitter. The controller does not distinguish a reconnecting agent from a new registration attempt — the agent re-presents its JWT and mTLS cert; the controller validates both and re-issues the NATS subscription list. No in-flight jobs are lost: the agent re-attaches to its durable NATS consumer ([ADR-002](002-nats-subjects-and-envelopes.md)) after reconnection. Jobs that were `running` when the agent disconnected are detected by the heartbeat timeout sweeper ([ADR-001](001-run-and-job-lifecycle.md)) and transitioned to `failed` for retry.

6. **Rate limiting on status RPCs:** The controller enforces a per-agent rate limit on `ReportStepStatus` and `StreamLogs`: max 200 RPC calls/second per agent (configurable). Agents exceeding this limit receive `ResourceExhausted` and must apply backoff. This prevents a runaway step from OOMing the controller via log volume alone.

7. **ExchangeJobKeys failure handling:** If `ExchangeJobKeys` fails (network error, timeout, or server-side key validation failure), the agent NAKs the NATS job message and does **not** start execution. The controller logs the failure with `job_run_id` and `attempt`; the scheduler retries up to `max_attempts`. A persistent `ExchangeJobKeys` failure (all attempts exhausted) transitions the job to `failed` with `failure_reason: key_exchange_failed`.

8. **Evolution:** Additive proto fields preferred; breaking changes require versioned package or new RPC with dual-publish window.

## Consequences

- `tonic` server in controller crate(s) must match proto; CI keeps `buf` or cargo proto generation in sync.
- Load balancers must support HTTP/2 for gRPC.

## Threat model

- **Assets:** JWTs, join tokens, log streams (may leak secrets if redaction fails).
- **Adversaries:** Stolen JWT, MITM without TLS, malicious agent sending forged statuses.
- **Mitigations:** Short JWT TTL, TLS, server-side validation of job_run_id against Postgres, log redaction pipeline (PRD 050).
- **Residual risk:** Compromised agent with valid JWT until revocation (PRD 110).

## References

- [proto/meticulous/agent/v1/agent.proto](../../proto/meticulous/agent/v1/agent.proto)
- [skills/meticulous-agent-security-invariants/SKILL.md](../../skills/meticulous-agent-security-invariants/SKILL.md)
