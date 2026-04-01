# PRD: SCM webhooks and pipeline triggers

**Status:** Draft  
**Owner:** TBD  
**Last updated:** 2026-03-31  
**Related:** [../features.md](../features.md) (Triggers), PRD 010, PRD 030, PRD 040

## Context

Pipelines run in response to **SCM events**, **manual** actions, **tags/releases**, and **schedules**. Inbound webhooks are a primary attack surface; they must be **authenticated** and **replay-safe**. Product direction includes **GitHub App**-style setup for webhooks ([../user-interface.md](../user-interface.md)).

## Problem statement

Unauthenticated or replayable webhooks let attackers enqueue arbitrary runs or probe internal behavior; manual trigger and schedule UX must stay simple without bypassing policy.

## Goals

- Accept webhooks from major SCMs (GitHub, GitLab, similar) with **signature or app token** verification and **idempotent** processing where providers allow.
- Support **manual**, **tag/release**, and **scheduled** triggers per pipeline configuration.
- Provide **one-click GitHub webhook provisioning** via GitHub App auth, **global or project** scoped ([../user-interface.md](../user-interface.md)).

## Non-goals

- Hosting Git or replacing SCM (Meticulous consumes events and refs only).
- Full parity with every Git hosting webhook variant in v1.
- **Outbound** notification webhooks (Slack, Teams, generic URLs for run alerts): **PRD 100** only. This PRD covers **inbound SCM** delivery only; see [OVERLAP-RESOLUTION.md](OVERLAP-RESOLUTION.md).

## Users and stakeholders

| Role | Need |
| --- | --- |
| Project maintainer | Connect repo, verify events trigger correct pipeline. |
| Security | Confidence in webhook authenticity and fork/PR policy (TBD). |
| Developer | Manual re-run without leaking secrets to untrusted contexts. |

## Functional requirements

| ID | Requirement | Priority | Notes |
| --- | --- | --- | --- |
| FR-1 | Register webhook endpoints per project with shared secret or App credentials. | P0 | |
| FR-2 | Validate payload signatures / JWTs per provider; reject stale replays per policy. | P0 | |
| FR-3 | Map events (push, PR, tag, etc.) to pipeline triggers declaratively. | P0 | |
| FR-4 | Manual run API and UI entry point with RBAC from PRD 010. | P0 | |
| FR-5 | Cron or schedule triggers with timezone and concurrency controls. | P1 | |
| FR-6 | Guided GitHub App install and webhook URL registration (one-click flow). | P1 | UI in [../user-interface.md](../user-interface.md). |

## Non-functional requirements

| ID | Requirement | How verified |
| --- | --- | --- |
| NFR-1 | Webhook ingress rate limits per project/org. | Load test / gateway config |
| NFR-2 | Structured logs for delivery attempts without secret leakage. | Log review |

## Security and privacy

- **Trust:** Treat PR/fork triggers as potentially untrusted; align with [../constraints.md](../constraints.md) (default deny for secrets on untrusted workflows).
- **Threats:** Replay, SSRF via callback URLs (if any), webhook secret brute force.

## Dependencies and assumptions

- **Depends on:** PRD 010 for authz; PRD 030 for pipeline definitions; public HTTPS endpoint for webhooks.
- **Assumes:** Operators configure DNS/TLS for API ingress.

## Success metrics

| Metric | Target | Measurement |
| --- | --- | --- |
| False positive webhook rejects | TBD low | Support + metrics |
| Time to connect GitHub App | TBD | UX timing |

## Rollout and migration

- Provider-specific modules behind feature flags; document manual webhook setup as fallback.

## Open questions

- ~~SCM attachment model, fork policy~~ **Resolved:** `project_repos` join table with `fork_policy enum(block|no_secrets|allow_secrets)` and `clone_depth`. One-to-one enforced in v1 via unique constraint; one-to-many in v2. Default `fork_policy: no_secrets`. See [../open-questions.md](../open-questions.md).

## Out of scope / future work

- Generic “any HTTP POST” trigger without strong auth.
