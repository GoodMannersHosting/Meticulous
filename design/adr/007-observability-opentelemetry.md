# ADR-007: Observability (OpenTelemetry metrics and traces)

**Status:** Proposed  
**Date:** 2026-03-31  
**PRDs:** [080](../prd/080-observability-opentelemetry.md)

## Context

[crates/met-telemetry](../../crates/met-telemetry) initializes the OpenTelemetry SDK, OTLP export, and Axum middleware. [metrics.rs](../../crates/met-telemetry/src/metrics.rs) defines the **metric instrument names** below. [tracing.rs](../../crates/met-telemetry/src/tracing.rs) defines span helpers for HTTP, gRPC, NATS producer/consumer.

## Decision

1. **Metric naming** — Keep the `met_` prefix and current instrument names as the **stable contract** for dashboards:
   - `met_api_request_duration_seconds`, `met_api_requests_total`, `met_api_requests_in_flight`, `met_api_errors_total`
   - `met_pipeline_runs_total`, `met_pipeline_run_duration_seconds`, `met_pipeline_runs_active`
   - `met_job_executions_total`, `met_job_execution_duration_seconds`, `met_jobs_queued`, `met_jobs_running`
   - `met_agents_connected`, `met_agent_heartbeats_total`, `met_agent_job_assignments_total`
   - `met_storage_operations_total`, `met_storage_operation_duration_seconds`, `met_storage_bytes_transferred`

   **Histogram bucket boundaries** (explicit `.with_boundaries()` required; SDK defaults are poorly suited):
   - `met_api_request_duration_seconds`: `[0.005, 0.01, 0.025, 0.05, 0.075, 0.1, 0.25, 0.5, 0.75, 1.0, 2.5, 5.0, 7.5, 10.0]` — aligned with OTel HTTP semantic conventions semconv 1.24+.
   - `met_job_execution_duration_seconds`: `[1.0, 5.0, 10.0, 30.0, 60.0, 120.0, 300.0, 600.0, 1200.0, 1800.0, 3600.0]` — covers 1s no-op through 1h build.
   - `met_pipeline_run_duration_seconds`: `[10.0, 30.0, 60.0, 120.0, 300.0, 600.0, 1200.0, 1800.0, 3600.0, 7200.0]` — pipeline aggregates multiple jobs.

2. **Resource attributes** — Set `service.name`, `service.version`, and deployment environment from config. Add `met.organization_id` **only** where cardinality is bounded (e.g. internal admin dashboards), not on every HTTP span by default.

3. **Trace propagation** — W3C `traceparent` on HTTP ingress; propagate `trace_id` from [JobDispatch](../../proto/meticulous/controller/v1/controller.proto) into agent gRPC metadata and log context ([ADR-003](003-grpc-agent-control-plane.md)).

4. **RED for API** — Count requests, errors, and duration histograms as already defined; SLO dashboards should use these three.

## Consequences

- Renaming a metric is a breaking change for operators; use versioned suffix if needed (`met_api_requests_total_v2`).
- Cardinality guardrails: document blocked high-cardinality labels (raw `user_id`, unbounded `pipeline_name`) in runbooks.

## Threat model

- **Assets:** Metrics/traces may include branch or pipeline names.
- **Adversaries:** Information disclosure via mis-scoped exemplars or attributes.
- **Mitigations:** Allowlist attributes on public-facing aggregations; scrub in export pipeline if needed.

## References

- [met-telemetry/src/lib.rs](../../crates/met-telemetry/src/lib.rs)
- [PRD VERIFICATION](../prd/VERIFICATION.md) for PRD 080 done criteria
