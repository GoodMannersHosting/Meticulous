# PRD: Kubernetes operator and agent fleet

**Status:** Draft  
**Owner:** TBD  
**Last updated:** 2026-03-31  
**Related:** [../features.md](../features.md), [../agents.md](../agents.md), [../../skills/meticulous-agent-security-invariants/SKILL.md](../../skills/meticulous-agent-security-invariants/SKILL.md), PRD 040, PRD 060

## Context

**met-agent** runs on **Linux, macOS, Windows** ([../agents.md](../agents.md)). **Kubernetes** deployments use **met-operator** for **ephemeral pools**, **CRD-driven** configuration, **scaling**, and **join token** patterns ([../features.md](../features.md)). Agents are **untrusted until enrolled**; **join tokens** are scoped; **revocation** and **JWT** lifecycle support long-lived runners ([../../skills/meticulous-agent-security-invariants/SKILL.md](../../skills/meticulous-agent-security-invariants/SKILL.md), [../agents.md](../agents.md)).

## Problem statement

Operating hundreds of agents without an operator pattern is error-prone; enrollment must stay **scoped** and **revocable** while supporting **dry/test** modes for developer iteration ([../agents.md](../agents.md)).

## Goals

- **Join token** issuance with scope: pipeline, project, or broader group semantics ([../agents.md](../agents.md)).
- **Provisioning flow:** token, security bundle + pubkey, validation, JWT, queue join ([../agents.md](../agents.md)).
- **Server-side revocation** and health visibility per pool ([../features.md](../features.md)).
- **Kubernetes operator:** scale agent pods, reconcile CRDs, align with GHA runner-controller spirit ([../agents.md](../agents.md)).
- **Configurable join checks:** OS, hostname, network posture, patch level (details TBD) ([../agents.md](../agents.md)).
- **Test/dry** execution modes that reduce production side effects ([../agents.md](../agents.md)).

## Non-goals

- Running the control plane only inside customer clusters without a central API (hybrid deferred).
- BSD agents in v1 ([../agents.md](../agents.md)).

## Users and stakeholders

| Role | Need |
| --- | --- |
| Platform operator | Scale pools, rotate join tokens, see unhealthy agents. |
| Security | Enforce join policy; revoke compromised hosts. |
| Developer | Fast local iteration with dry/test modes. |

## Functional requirements

| ID | Requirement | Priority | Notes |
| --- | --- | --- | --- |
| FR-1 | Issue and revoke join tokens; bind to scope and expiry. | P0 | |
| FR-2 | Agent registration with security bundle and pubkey storage. | P0 | [../agents.md](../agents.md). |
| FR-3 | JWT issuance with renewal policy for long-lived agents. | P0 | [../agents.md](../agents.md). |
| FR-4 | Heartbeats and last-seen for pool health UI/API. | P0 | |
| FR-5 | Operator reconciles desired vs actual agent count (K8s). | P1 | |
| FR-6 | Pluggable join validation rules (admin-configurable). | P1 | **Required checks (always run):** OS type within allowlist, kernel version ≥ 5.0 (seccomp-notif support), NTP sync within 5 s of controller clock. **Optional (operator-configurable):** hostname regex match, public IP within allowed CIDR ranges, OS patch level ≥ policy baseline. Failed required check → join rejected with 403 and audit event. Failed optional check → join allowed with warning in audit log. [../agents.md](../agents.md). |
| FR-7 | NTP/clock skew warnings or hard fail per policy. | P1 | [../agents.md](../agents.md). |

## Non-functional requirements

| ID | Requirement | How verified |
| --- | --- | --- |
| NFR-1 | Revocation effective within **30 seconds** of operator action: controller stops dispatching jobs to revoked agent; in-flight jobs complete but no new jobs are assigned. | Test: revoke, assert no new job dispatched within 30s. |
| NFR-2 | Operator reconciliation loop stable under churn. | Soak test: continuous pod churn for 30 min with no stuck reconcile loops. |
| NFR-3 | Join token issuance rate-limited to 100/minute per org to prevent credential flooding. | Load test. |

## Security and privacy

- **Threats:** Stolen join tokens, rogue agents, MITM on registration (mitigate with mTLS per [ADR-003](../adr/003-grpc-agent-control-plane.md), token scope).
- **Controls:** Per-job PKI for secrets (PRD 060); kill switch for agent IDs; revocation effective within 30 seconds (NFR-1).
- **Revocation grace period:** When an agent is revoked, in-flight jobs are allowed to complete (agent holds its current JWT until expiry) but no new jobs are dispatched. The controller sets a `revoked_at` timestamp on the agent row and the dispatch gate checks this before publishing to NATS. If immediate termination is required (security incident), the operator may also expire the JWT early via an `InvalidateJWT` admin RPC (to be defined).
- **Long-lived agent approval:** macOS and Windows agents that cannot be ephemeral require an explicit operator approval step for JWT renewal beyond 7 days. The controller records approvals in `audit_log`.

## Dependencies and assumptions

- **Depends on:** PRD 040 for queue membership after enrollment.
- **Assumes:** Agents have outbound gRPC and NATS connectivity.

## Success metrics

| Metric | Target | Measurement |
| --- | --- | --- |
| Time to enroll new agent | TBD | UX |
| Revoked agents stop receiving work | 100% | Tests |

## Rollout and migration

- Document token rotation; version operator CRDs with conversion if needed.

## Open questions

- Full environment validation matrix ([../open-questions.md](../open-questions.md), [../agents.md](../agents.md)).

## Out of scope / future work

- BSD amd64/arm64 agents without containers ([../agents.md](../agents.md)).
