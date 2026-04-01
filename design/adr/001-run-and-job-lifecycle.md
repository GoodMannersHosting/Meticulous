# ADR-001: Run and job lifecycle in Postgres

**Status:** Proposed  
**Date:** 2026-03-31  
**PRDs:** [030](../prd/030-pipeline-authoring-dag-workflows.md), [040](../prd/040-scheduling-and-nats-dispatch.md), [050](../prd/050-agent-execution-logs-artifacts.md)

## Context

The pipeline engine resolves a DAG into **runs** (pipeline execution) and **jobs** (DAG nodes). The scheduler enqueues work to NATS; agents report step status over gRPC. We need a **single system of record** for state so the API, UI, and controller agree on idempotency and retries.

## Decision

1. **Entities** (logical; exact table names live in `migrations/` and may evolve):
   - **Run:** one execution of a pipeline for a trigger context (manual, webhook, schedule). Holds `run_id`, `org_id`, `project_id`, `pipeline_id`, trigger metadata, overall status, timestamps.
   - **Job run:** one schedulable unit after DAG expansion. Holds `job_run_id`, `run_id`, `job_name` (or IR id), `status`, `attempt`, pool tags, links to step records.
   - **Step run:** optional normalized child of job run for per-step status, exit code, log offsets.

2. **Status machine (minimum):** `queued` → `dispatched` → `running` → (`succeeded` | `failed` | `cancelled` | `timed_out`). A `cancelling` intermediate state is valid when a cancel signal is in-flight to an active agent. Transitions are recorded with a **monotonic integer `version` column** (default 0, incremented on every state change); all writers use `UPDATE ... WHERE version = $expected_version` for optimistic concurrency. `updated_at` alone is insufficient because two writers in the same millisecond can both observe the same timestamp.

   **Timeout policy:** Each job run may carry a `timeout_seconds` from the pipeline definition. The scheduler sets a `deadline_at` column at dispatch time; a background sweeper promotes `dispatched` or `running` jobs past their deadline to `timed_out` and publishes a cancellation signal to the agent.

   **Cancellation flow:** A user-initiated cancel transitions the run to `cancelling` in Postgres, then publishes a `CancelJob` message over gRPC ([ADR-003](003-grpc-agent-control-plane.md)). The run moves to `cancelled` when the agent ACKs or when the heartbeat TTL elapses without confirmation.

3. **Idempotency:** Webhook and API ingress use a **dedupe key** (e.g. provider delivery id + event id) stored on `run` or a side table; scheduler uses `(run_id, job_run_id, attempt)` as the idempotent unit before publishing to NATS ([ADR-002](002-nats-subjects-and-envelopes.md)).

4. **Retry policy:** Each job definition may declare `max_attempts` (default: 1, max: 5). On non-zero exit or agent timeout, the scheduler increments `attempt` and re-enqueues with the same `job_run_id` and new `attempt` number. Step-level retry (retrying a single step within a job without re-running earlier steps) is a v2 feature — in v1, a retry re-runs the entire job. A job that exhausts `max_attempts` transitions to `failed` with `failure_reason: max_attempts_exceeded`.

5. **Remote cache:** Pipeline YAML (PRD 030) references cache configuration; **execution** owns persistence of cache keys and hit/miss metadata on job or step rows, or a dedicated table keyed by `job_run_id`. Key derivation and tenant isolation are specified alongside [ADR-004](004-secrets-and-per-job-pki.md) for secret material never appearing in cache keys in plaintext.

## Consequences

- Migrations must add or alter tables before enabling multi-job runs in production.
- Replay of the same webhook must not create duplicate runs if dedupe key matches.
- Observability ([PRD 080](../prd/080-observability-opentelemetry.md)) should attach `run_id`, `job_run_id`, `org_id` as trace attributes when present.

## Threat model

- **Assets:** Run metadata may reference branch names and commit SHAs (semi-sensitive).
- **Adversaries:** User A must not mutate or read runs for org B (enforced in API layer per PRD 010).
- **Mitigations:** Foreign keys and RLS or application-level org checks on every query.
- **Residual risk:** Misconfigured RBAC in API; covered by PRD 010 verification.

## References

- [proto/meticulous/controller/v1/controller.proto](../../proto/meticulous/controller/v1/controller.proto) (`JobDispatch` fields `run_id`, `job_run_id`, `attempt`).
- [skills/meticulous-rust-workspace/SKILL.md](../../skills/meticulous-rust-workspace/SKILL.md) for migrations location.
