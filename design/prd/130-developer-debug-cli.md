# PRD: Developer debug CLI

**Status:** Draft
**Owner:** TBD
**Last updated:** 2026-03-31
**Related:** [../open-questions.md](../open-questions.md), PRD 010, PRD 050, PRD 060

## Context

Developers need to diagnose failing pipeline runs without requiring a live shell on the runner host or exposing secret values. The debug CLI must offer enough observability to reproduce and investigate failures while keeping the threat model tractable. The core tension: every capability added to a debug CLI is a potential exfiltration surface if the RBAC model is misconfigured or if an attacker obtains developer credentials.

## Problem statement

Without a debug-oriented CLI experience, developers replicate failures by triggering full re-runs and reading live logs — a slow feedback loop. However, SSH re-run models (CircleCI) effectively expose job environment variables including secrets, which is incompatible with Meticulous's security-first posture.

## Threat model

**Assets at risk:**
- Secret values injected into the job environment (PRD 060).
- Artifact contents (may contain pre-release binaries, signing keys, sensitive configs).
- Internal topology (runner hostnames, network ranges) revealed by filesystem or env inspection.

**Adversaries:**
- Developer with valid credentials but insufficient authorization (IDOR, scope escalation).
- Attacker with stolen developer credentials using the debug CLI as an exfiltration path.
- Malicious pipeline author inserting a step that leaks secrets through debug-accessible surfaces.

**Mitigations:**
- All debug commands operate through the existing RBAC gate (PRD 010). No debug-specific privilege escalation.
- Secret values are never returned by any debug API — the same masking pipeline from PRD 050 applies.
- No live shell access to job containers, ever, without a separate security ADR.
- `--no-secrets` behavior is not a flag; it is hardcoded. There is no flag position that makes it safe to return secret material through a CLI.

## Goals

- Provide read-only observability into completed and in-progress runs: logs, step status, environment variable names (not values), and timing.
- Support local reproduction of a job's environment using sanitized variable snapshots.
- Surface DAG, pipeline, and lint output without starting a run.

## Non-goals

- Live shell access to job containers (`met debug shell`): explicitly out of scope without a separate ADR and threat model sign-off.
- Reading or displaying secret values, even for org admins.
- Writing to artifact storage or job state through the CLI.
- SSH-based runner access (the CircleCI model).

## Users and stakeholders

| Role | Need |
| --- | --- |
| Developer | Diagnose failing step without triggering a full re-run. |
| Security | Ensure debug surface does not become an exfiltration path. |
| Platform admin | Audit which users invoked debug commands and on which runs. |

## Functional requirements

| ID | Requirement | Priority | Notes |
| --- | --- | --- | --- |
| FR-1 | `met debug logs <run-id>` — fetch redacted log stream via the same masking pipeline as the UI (PRD 050). | P0 | |
| FR-2 | `met debug steps <run-id>` — list steps with status, exit codes, duration, error messages. | P0 | |
| FR-3 | `met debug env-names <run-id>` — list environment variable **names only**; values are never returned. | P1 | Useful for diagnosing missing variable configs without exposing values. |
| FR-4 | `met debug repro <run-id>` — generate a local `docker run` command reproducing the job's container and workspace state: secrets replaced with placeholder strings, non-secret vars preserved, image digest pinned. | P1 | Allows local re-run without any live secret material. |
| FR-5 | `met debug dag <pipeline-id>` — render the DAG for a pipeline without starting a run. | P1 | |
| FR-6 | `met lint <pipeline-file>` — static lint of a pipeline YAML. Rule engine only; no network calls; structured JSON output. | P0 | Separate from debug; included here as it is a primary CLI DX tool. See open-questions.md for rule taxonomy. |
| FR-7 | `met suggest <pipeline-file>` — AI-assisted suggestions fed by lint output. Non-blocking, labeled AI-generated, opt-in. | P2 | Separate binary or subcommand from `met lint`. |

## Non-functional requirements

| ID | Requirement | How verified |
| --- | --- | --- |
| NFR-1 | All debug commands require the same authz as the equivalent API endpoint (PRD 010). | Integration tests with RBAC matrix. |
| NFR-2 | Debug commands produce structured audit log events (actor, run-id, command, outcome). | Log review + unit tests. |
| NFR-3 | `met lint` runs in < 1s for typical pipeline files (< 500 lines). | Benchmark. |

## Security and privacy

- Every debug command is logged to `audit_log` (ADR-008) with `event_type: debug.logs_accessed`, `debug.steps_accessed`, etc.
- Redacted logs pass through the same `SecretRedactor` and `PathRedactor` pipeline as the live log stream; no special debug bypass.
- `met debug repro` must be reviewed to ensure the generated `docker run` command does not embed actual secret values in ENV flags even transiently. Implementation must source non-secret variables from a sanitized snapshot, not from the live execution record.

## Dependencies and assumptions

- **Depends on:** PRD 010 for authz; PRD 050 for log APIs and masking; PRD 060 secret redaction invariants.
- **Assumes:** CLI authenticates via an API token with appropriate scopes; no special debug-tier tokens.

## Success metrics

| Metric | Target | Measurement |
| --- | --- | --- |
| Time to diagnose a failing step (with debug CLI) | TBD improvement over re-run | UX study |
| Secret leakage incidents via debug CLI | Zero | Incidents |

## Rollout and migration

- Ship `met debug logs` and `met debug steps` first as they are lowest risk (pure read-only log fetch).
- `met debug repro` requires the sanitized snapshot pipeline to be implemented and reviewed before release.
- `met lint` can ship independently of the debug commands.
- `met debug shell` requires a standalone ADR and is deferred indefinitely.

## Open questions

- Exact capability allowlist for `met debug repro` (which workspace files are included in the snapshot, if any).
- Whether `met debug env-names` should require a higher permission level than `met debug logs` (env names may reveal internal naming conventions).
