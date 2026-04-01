# PRD verification (definition of done)

Maps each PRD to **tests**, **observability** (PRD 080), and **security checks**. Paths and suite names are filled in as the codebase grows.

| PRD | Integration / e2e | Metrics / traces (080) | Security |
| --- | --- | --- | --- |
| 010 | RBAC tests: deny cross-tenant read/write; token revoke invalidates API | Audit events emitted for token/membership changes | Token never logged; hashed at rest |
| 020 | Webhook signature invalid + replay window tests | Trace from ingress to run create | No run on bad signature; rate limits |
| 030 | DAG cycle rejected; invalid YAML errors with context | Parser span attributes | No code exec from YAML |
| 040 | Enqueue idempotency `(job_run_id, attempt)`; NATS redelivery handled | `dispatch_lag`, publish errors | Assert no cleartext secrets in NATS payload |
| 050 | Log redaction unit tests; artifact upload ACL | Log pipeline latency; trace run id | Secret patterns stripped |
| 060 | Harness: decrypt only on agent; DB/NATS inspection tests | Spans on provider calls | ADR-004 review checklist |
| 070 | Telemetry RBAC; assert no payload fields | Telemetry cardinality limits | Retention and access audit |
| 080 | Smoke: RED metrics on API; one cross-service trace | Self-check dashboards | No PII in metric labels |
| 090 | Deferred post slice v1 unless SBOM in scope | SBOM storage size alerts | Signed attestation verification when enabled |
| 100 | Channel retry tests; SSRF allowlist for outbound URLs | Notification delivery metrics | Webhook URLs stored as secrets |
| 110 | Register + revoke stops new jobs; heartbeat timeout | Agent pool health gauges | Join token scope enforcement |
| 120 | UI e2e: log stream, DAG view (when built) | RUM optional | XSS sanitization on log lines |
| 130 | Debug commands require same RBAC as API endpoint; `met lint` golden-test suite (valid + invalid fixtures); no secret value ever returned by any debug command | Audit events for all debug command invocations | Secret masking identical to PRD 050 pipeline; no special debug bypass |

## Vertical slice v1 (manual run)

Minimum bar before expanding scope:

- [ ] One integration test: manual run → agent → terminal status (see [.cursor/plans/vertical_slice_manual_run.plan.md](../../.cursor/plans/vertical_slice_manual_run.plan.md)).
- [ ] ADR-001–004 reviewed against implementation (Proposed → Accepted when team agrees).
- [ ] ADR-009 (linter): at least the `depends-on` cycle detection rule and one secret hygiene rule are implemented and tested.
- [ ] ADR-010 (data model): `project_members` and `project_repos` migrations applied; no `owner_user_id` column on `projects`.
- [ ] PRD 080: at least one metric on `met-api` request path for the slice.
- [ ] `webhook_deliveries` table exists and dedup is enforced for GitHub webhooks (ADR-005).
- [ ] Tenant fairness: per-org `max_concurrent_jobs` cap enforced in scheduler (PRD-040).

## Definition of done per ADR

| ADR | Done when |
| --- | --- |
| 001 | Status machine transitions tested; `version` column present; timeout sweeper passes chaos test |
| 002 | Subject grammar validated; DLQ consumer created; NATS ACLs enforced per-agent credential |
| 003 | mTLS cert presented by agent in integration test; JWT proactive renewal tested; reconnect backoff verified |
| 004 | `ExchangeJobKeys` failure triggers NAK and retry; `PathRedactor` applied before proto send |
| 005 | `webhook_deliveries` table live; GitHub dedup test passes; Bitbucket handler returns `501` until HMAC implemented |
| 006 | `seccomp-notif` or polling mode detected at startup and logged; path redaction in step report |
| 007 | All three histogram types use explicit `.with_boundaries()`; W3C `traceparent` in NATS headers |
| 008 | `project_members` table seeded at project creation; audit_log append-only trigger verified |
| 009 | `met lint` runs in < 1s on 500-line fixture; golden tests pass; JSON output matches schema |
| 010 | `project_repos.fork_policy` read by webhook handler; `secrets.scope_type+scope_id` double-checked |

## References

- [OVERLAP-RESOLUTION.md](OVERLAP-RESOLUTION.md)
- [design/adr/README.md](../adr/README.md) (ADR-005 through ADR-008 cover webhook security, telemetry schema, observability, RBAC)
