# ADR-002: NATS subjects and job dispatch envelopes

**Status:** Proposed  
**Date:** 2026-03-31  
**PRDs:** [040](../prd/040-scheduling-and-nats-dispatch.md), [110](../prd/110-kubernetes-operator-and-agent-fleet.md)

## Context

Agents subscribe with **egress-only** networking; work is pushed via **NATS JetStream**. The repo already defines a protobuf message for dispatch ([proto/meticulous/controller/v1/controller.proto](../../proto/meticulous/controller/v1/controller.proto)). We need consistent **subject naming**, **stream configuration**, and **payload rules** so pools are isolated and secrets never appear in cleartext on the bus.

## Decision

1. **Message type:** Use **`JobDispatch`** (and related types in `controller.proto`) as the canonical serialized payload on NATS unless a future ADR switches to JSON. Serialization: protobuf binary on the wire; content-type negotiated per consumer library.

2. **Subject pattern:** Two-segment normalized form: `meticulous.jobs.<org_slug>.<pool_id>` where each segment matches `[a-z0-9][a-z0-9-]{0,62}` (no dots; no spaces; max 63 chars per segment). **Do not** encode tag lists as dot-separated segments — that creates wildcard collision risk (a subscription `meticulous.jobs.org-a.>` would match a different tenant's subjects if org slugs share a prefix). Instead, pool-tag filtering uses JetStream `FilterSubjects` (NATS ≥ 2.10) or per-pool durable consumers with subject-per-pool isolation. Agents receive concrete subscription patterns at registration time ([ADR-003](003-grpc-agent-control-plane.md) `RegisterResponse.nats_subjects`). **Org slugs are server-normalized** and uniqueness-enforced to prevent subject injection.

3. **JetStream configuration defaults:**

   | Parameter | Default | Rationale |
   |---|---|---|
   | `retention` | `WorkQueuePolicy` | Jobs consumed once; prevents unbounded growth |
   | `storage` | `FileStorage` | Durable across NATS restarts |
   | `num_replicas` | 3 (prod) / 1 (dev) | JetStream HA recommendation |
   | `max_msg_size` | 512 KiB | Control messages only; large blobs go to object storage |
   | `max_age` | 24h | Stale jobs must not run; tie to deployment lease timeout |
   | `discard` | `DiscardNew` | Fail fast on backpressure; do not silently drop |
   | `ack_policy` | `AckExplicit` | Required for at-least-once with idempotent job IDs |
   | `max_deliver` | 5 | After 5 unacked deliveries, route to DLQ |

   **Dead-letter queue:** NATS JetStream has no native DLQ. Create a sibling stream `meticulous.jobs.dlq.<org_slug>` and publish to it inside the controller's `max_deliver` exhaustion handler. A durable consumer on the DLQ stream drives alerting and manual replay. Controllers must not re-dispatch DLQ'd jobs without operator confirmation.

4. **No cleartext secrets on NATS:** `JobDispatch.secrets` holds **encrypted** blobs only; plaintext secret values are never fields on the dispatch message. If `requires_secret_exchange` is true, bulk secret delivery may occur over gRPC ([ADR-004](004-secrets-and-per-job-pki.md)).

5. **Idempotency:** Message metadata includes `job_run_id`, `run_id`, `attempt` (see proto). Consumers treat duplicate delivery as idempotent when `(job_run_id, attempt)` already terminal in Postgres.

## Consumer and credential naming

- **Durable consumer name:** `met-agent-<agent_id>` (unique per registered agent, stable across reconnects). The controller creates or upserts this consumer on the agent’s stream at registration time ([ADR-003](003-grpc-agent-control-plane.md)).
- **NATS credentials:** Each agent receives a unique NKey or credential file scoped to `meticulous.jobs.<org_slug>.<pool_id>` subscribe and `meticulous.jobs.dlq.<org_slug>` publish. Credentials are provisioned by the controller at registration and rotated when the agent JWT is renewed. Agents must not share credentials between pool membership.
- **ACL enforcement:** NATS server user permissions for each agent credential are: `subscribe: ["meticulous.jobs.<org_slug>.<pool_id>"]`, `publish: ["$JS.ACK.>", "meticulous.jobs.dlq.<org_slug>"]`. The scheduler service has a separate credential with `publish: ["meticulous.jobs.>"]` and `subscribe: ["$JS.API.>"]`. Org slug isolation is enforced at the NATS permission level, not just by application convention.

## Consequences

- Controller and scheduler must use the same subject builder as the agent’s registered subscriptions.
- Changing subject grammar requires coordinated rollout of agents and control plane.
- NATS user provisioning (credential generation) must be part of the agent registration flow; the controller needs write access to NATS user/account configuration at startup.

## Threat model

- **Assets:** Dispatch reveals pipeline name, job name, variable **keys**, and encrypted blobs.
- **Adversaries:** Subscriber on wrong subject (misconfiguration); insider with NATS creds.
- **Mitigations:** ACLs per tenant/pool; separate NATS credentials per agent as already suggested by `NatsCredentials` in proto; no plaintext secrets.
- **Residual risk:** Weak pool isolation if subject patterns overlap; review during deployment.

## Proto alignment notes

- **`JobDispatch` subject comment** (controller.proto line 11): Currently says `meticulous.jobs.<pool>.<tags>`. Must be updated to `meticulous.jobs.<org_slug>.<pool_id>` to match this ADR's grammar.
- **Additional NATS subjects** in use that need documented grammar:
  - `meticulous.claims.<job_run_id>` (JobClaim — agent claims a job)
  - `meticulous.completions.<run_id>` (JobCompletion — agent reports job done)
  - `meticulous.agents.<org_id>` (AgentEvent — controller publishes lifecycle events)
  - `meticulous.runs.<pipeline_id>` (RunEvent — engine publishes run state)
  These follow the two-segment principle: top-level type segment + scoping segment. All subject strings must use the same org-slug normalization as the jobs subjects.

## References

- [proto/meticulous/controller/v1/controller.proto](../../proto/meticulous/controller/v1/controller.proto) `JobDispatch`, `JobPayload`, `JobClaim`, `JobCompletion`.
- [PRD overlap](../prd/OVERLAP-RESOLUTION.md) (notifications vs SCM).
