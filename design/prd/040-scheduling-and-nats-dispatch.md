# PRD: Scheduling and NATS job dispatch

**Status:** Draft  
**Owner:** TBD  
**Last updated:** 2026-03-31  
**Related:** [../architecture.md](../architecture.md), [../constraints.md](../constraints.md), PRD 030, PRD 050, PRD 110

## Context

The **pipeline engine** and **scheduler** enqueue work; **NATS JetStream** delivers jobs to agents subscribed by **pool tags** ([../architecture.md](../architecture.md)). Agents are **egress-only**; dispatch must tolerate broker and process restarts per [../constraints.md](../constraints.md).

## Problem statement

Without durable queues and clear **delivery semantics**, runs stall, duplicate, or execute on wrong pools; operators cannot reason about backlog or fairness.

## Goals

- Enqueue **job units** after DAG resolution with correct **pool tag** routing.
- Use JetStream (or equivalent) for **durable** dispatch matching agent subscriptions.
- Define **at-least-once** behavior with **idempotent** acceptance on agent/controller side (operational contract TBD in implementation).
- Surface **queue depth** / lag signals for operators (P1).

## Non-goals

- In-process-only queues without persistence for production.
- Cross-region active/active broker topology (defer to future ops PRD).

## Users and stakeholders

| Role | Need |
| --- | --- |
| Scheduler service | Deterministic enqueue order per policy. |
| Agent pool operator | Predictable subject naming and pool tags. |
| SRE | Observable lag, DLQ or retry policy visibility. |

## Functional requirements

| ID | Requirement | Priority | Notes |
| --- | --- | --- | --- |
| FR-1 | Publish job messages to subjects derived from pool tags / tenant policy. | P0 | |
| FR-2 | Persist enqueue state in Postgres for run correlation. | P0 | |
| FR-3 | Handle agent disconnect: redelivery after visibility timeout / NAK. | P0 | Tuned per deployment. |
| FR-4 | Reject or defer jobs when no healthy agents in pool (configurable). | P1 | |
| FR-5 | Dead-letter or poison-message path for permanently failing jobs. | P1 | |

## Non-functional requirements

| ID | Requirement | How verified |
| --- | --- | --- |
| NFR-1 | Survive NATS restart with configured retention. | Chaos test |
| NFR-2 | No plaintext secrets in NATS payloads. | Code review + PRD 060 |

## Tenant fairness

Without fairness controls, a single tenant submitting a burst of jobs can starve all other tenants sharing an agent pool. V1 fairness model:

- **Per-org concurrency cap:** configurable `max_concurrent_jobs` per org (default: 50); scheduler does not enqueue beyond this limit. Jobs above the cap are held in Postgres with status `queued` and enqueued when capacity is available.
- **Round-robin org ordering:** when multiple orgs have queued jobs, the scheduler cycles through eligible orgs in round-robin order before filling the next dispatch slot, rather than FIFO across all orgs. This is a best-effort fairness — not a strict fair-share scheduler — and is appropriate for v1 where all tenants share a pool.
- **Priority within an org:** FIFO by `created_at` within a single org's queue. No cross-org priority preemption.

## Security and privacy

- Messages carry **job identity** and references, not raw secret values.
- **Threats:** Subject namespace isolation between tenants; subscription ACLs enforced at NATS credential level ([ADR-002](../adr/002-nats-subjects-and-envelopes.md)); a misconfigured org slug could allow subject overlap — server-side normalization and uniqueness enforcement are mandatory.

## Dependencies and assumptions

- **Depends on:** PRD 030 resolved DAG; PRD 110 healthy agent registration.
- **Assumes:** JetStream enabled and sized per deployment.

## Success metrics

| Metric | Target | Measurement |
| --- | --- | --- |
| Duplicate execution rate | Near zero for same job id | Metrics |
| P95 dispatch latency | < 500 ms (webhook ingestion → NATS publish) | OTel (PRD 080) |

## Rollout and migration

- Version message schema; dual-read period if upgrading envelope format.

## Open questions

- **Idempotency keys:** `(job_run_id, attempt)` is the unit per ADR-001. Orphan cleanup: jobs in `dispatched` state with no heartbeat for `2 × heartbeat_interval + lease_timeout` are transitioned to `failed` by the scheduler sweeper. The exact timeout values are deployment-configurable; recommended defaults are heartbeat 30s, lease 5m.
- **Priority queuing:** No cross-tenant priority preemption in v1. Within a tenant, FIFO by `created_at`. Re-evaluate if customer-requested queue priority becomes a requirement.

## Out of scope / future work

- Global priority preemption across tenants.
