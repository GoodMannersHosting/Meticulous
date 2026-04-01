# PRD: Execution telemetry and audit metadata

**Status:** Draft  
**Owner:** TBD  
**Last updated:** 2026-03-31  
**Related:** [../features.md](../features.md), [../security.md](../security.md), PRD 050, PRD 090

## Context

Supply-chain and incident response require knowing **what ran** and **where it talked**. Product direction includes **binary execution metadata** (paths, SHA-256), optional **network flow metadata** (src/dst IP, ports; **no payloads**), and phased **syscall / binary auditing** ([../features.md](../features.md), [../security.md](../security.md)).

## Problem statement

Without structured execution and network metadata, security teams cannot answer blast-radius questions or detect unexpected exfil paths; over-collection risks privacy and cost.

## Goals

- Capture a **canonical list** of executed binaries with hashes per job/run where the platform allows ([../open-questions.md](../open-questions.md)).
- Optionally record **network connection metadata** (endpoints only) per step or run.
- Expose data via API for UI (PRD 090) and external analytics (PRD 080).
- Phase in **syscall** or kernel-level auditing where OS support exists.

## Non-goals

- Full packet capture or payload logging.
- Guaranteed coverage on every OS without documented limitations.

## Users and stakeholders

| Role | Need |
| --- | --- |
| Security engineer | Query what executed and where traffic went for a run. |
| Compliance | Evidence of monitoring controls. |
| Developer | Minimal performance impact; clear opt-in per pool. |

## Functional requirements

| ID | Requirement | Priority | Notes |
| --- | --- | --- | --- |
| FR-1 | Record executed binary path and SHA-256 (or stronger) when observable. | P0 | [../security.md](../security.md). |
| FR-2 | Associate metadata with job/step IDs and timestamps. | P0 | |
| FR-3 | Optional collection of TCP/UDP flow metadata (IPs, ports, direction). | P1 | [../open-questions.md](../open-questions.md). |
| FR-4 | Configurable retention and RBAC for telemetry rows. | P1 | |
| FR-5 | Syscall filtering / audit pipeline on supported Linux agents. | P2 | Phased |

## Non-functional requirements

| ID | Requirement | How verified |
| --- | --- | --- |
| NFR-1 | Telemetry overhead bounded per pool (CPU/mem). | Benchmark |
| NFR-2 | No secret values in telemetry fields. | Static analysis |

## Security and privacy

- **Threats:** PII in paths, leaking internal topology; mitigate with redaction policies.
- **Controls:** Tenant isolation for stored metadata; access via PRD 010 roles.

## Dependencies and assumptions

- **Depends on:** PRD 050 agent instrumentation hooks.
- **Assumes:** Agent has permission to observe processes per OS policy.

## Success metrics

| Metric | Target | Measurement |
| --- | --- | --- |
| Coverage rate of executed binaries | > 95% on Linux with `seccomp-notif`; best-effort on macOS/Windows | Metrics |
| False positive alert noise | < 1 alert/100 runs (tuned against 30d baseline) | SecOps feedback |

## Rollout and migration

- Feature flags per pool; document unsupported platforms.

## Open questions

- Storage volume and PII policy for binary list ([../open-questions.md](../open-questions.md)).
- Core dumps / ptrace limits when secrets in memory ([../open-questions.md](../open-questions.md)).

## Out of scope / future work

- User-mode hooking of interpreted languages beyond process exec events.
