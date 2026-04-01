# PRD: Observability (OpenTelemetry metrics and tracing)

**Status:** Draft  
**Owner:** TBD  
**Last updated:** 2026-03-31  
**Related:** [../features.md](../features.md), [../architecture.md](../architecture.md), PRD 040, PRD 050

## Context

Operators need **metrics** and **traces** across API, scheduler, controller, and agents to run the platform reliably. Direction is **OpenTelemetry** export to **Prometheus-compatible** backends ([../features.md](../features.md), [../architecture.md](../architecture.md)).

## Problem statement

Without consistent instrumentation, incidents in dispatch, execution, or storage show up as user-visible failures with no internal SLO signals.

## Goals

- Emit OTel **metrics** for core services (request rates, errors, queue lag, job durations).
- Emit **traces** spanning webhook ingestion through job completion where practical.
- Correlate logs with **trace IDs** (coordination with PRD 050 logging).

## Non-goals

- Shipping a hosted observability product inside Meticulous.
- Custom metric protocol parallel to OTel long term.

## Users and stakeholders

| Role | Need |
| --- | --- |
| SRE | Dashboards and alerts on platform health. |
| Developer | Trace slow pipeline stages. |

## Functional requirements

| ID | Requirement | Priority | Notes |
| --- | --- | --- | --- |
| FR-1 | OTel SDK integrated in API, engine/scheduler, controller. | P0 | |
| FR-2 | Standard exemplars or attributes: org_id, project_id, run_id (privacy review). | P1 | Cardinality controls. |
| FR-3 | Agent-side metrics for step timing and resource usage (opt-in). | P1 | Required metrics: `met_agent_step_duration_seconds` (histogram), `met_agent_memory_bytes` (gauge), `met_agent_cpu_usage_ratio` (gauge), `met_agent_log_lines_total` (counter), `met_agent_secret_exchanges_total` (counter). All labeled `{org_id, pool_id, step_name}`. |
| FR-4 | Documented scrape or OTLP export configuration. | P0 | |

## Non-functional requirements

| ID | Requirement | How verified |
| --- | --- | --- |
| NFR-1 | High-cardinality labels blocked or sampled. | Config review |
| NFR-2 | <1% overhead on API median latency at baseline load. | Benchmark |

## Security and privacy

- Avoid labeling with secret material or unsanitized branch names if high-cardinality.
- **Threats:** Metric labels leaking internal hostnames; gate via config.

## Dependencies and assumptions

- **Depends on:** Deployment provides OTLP endpoint or Prometheus scraper.
- **Assumes:** OTel collector optional in reference Helm/charts (future).

## Success metrics

| Metric | Target | Measurement |
| --- | --- | --- |
| Mean time to detect dispatch failures | < 5 min (alert fires before user report) | On-call data |
| Trace coverage of critical paths | ≥ 80% of webhook-to-completion spans sampled at baseline load | Sampling stats |

## Rollout and migration

- Start with RED metrics on API and controller; expand iteratively.

## Open questions

- **Unified correlation ID scheme** — **Working resolution:** Use W3C `traceparent` as the canonical correlation carrier. On NATS, serialize `traceparent` as a NATS message header (`traceparent: <value>`); NATS JetStream supports headers natively since v2.2. On gRPC, use the standard `traceparent` metadata key. Agents extract this from the `JobDispatch` message header and initialize their span context from it. This closes the ADR-003 / ADR-007 gap without a custom correlation header. Needs an implementation ADR to make it official.

## Out of scope / future work

- Real user monitoring for the web UI (separate initiative).
