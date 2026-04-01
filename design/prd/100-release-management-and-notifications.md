# PRD: Release management and notifications

**Status:** Draft  
**Owner:** TBD  
**Last updated:** 2026-03-31  
**Related:** [../features.md](../features.md), PRD 090, PRD 020, PRD 120

## Context

Later roadmap phase covers **release workflows**: **scheduling**, **communication templates** (“comms generator”), coordinated **notifications**, integration with **external alerting** (downtime windows, alert silencing hooks), and **rollback or promotion hooks** where targets support them ([../features.md](../features.md)). Notification channels include **outbound webhooks**, Slack, Teams, Webex, Discord (extensible). **Inbound SCM webhooks** are **PRD 020** only; see [OVERLAP-RESOLUTION.md](OVERLAP-RESOLUTION.md).

## Problem statement

Shipping is more than green CI; teams need timed communications, coordinated infra changes, and visibility in chat systems without manual copy-paste.

## Goals

- Model **release** entities or workflows tied to pipeline runs and artifacts.
- **Schedule** release-related actions (e.g. maintenance window) with RBAC (PRD 010).
- **Notify** via configurable channels on run completion, release milestones, and failures.
- **Comms generator:** templates for status pages or stakeholder messages (format TBD).
- Invoke **external hooks** for alert silencing or promotion where integration exists.

## Non-goals

- Owning an incident management product (PagerDuty replacement).
- Universal automatic rollback for all artifact types ([../features.md](../features.md)).
- **Inbound** Git/GitHub/GitLab webhook ingestion (pipeline triggers): **PRD 020** only.

## Users and stakeholders

| Role | Need |
| --- | --- |
| Release manager | Schedule and announce releases; silence known noise. |
| On-call | Get failures in Slack/Teams with deep links. |
| Platform admin | Configure org-wide notification defaults. |

## Functional requirements

| ID | Requirement | Priority | Notes |
| --- | --- | --- | --- |
| FR-1 | Subscribe channels to run events (success/fail/cancel). | P0 | |
| FR-2 | Integrations: incoming webhooks, Slack, Teams, Webex, Discord. | P1 | Extensible list [../features.md](../features.md). |
| FR-3 | Release workflow object with schedule and approvers (TBD). | P2 | |
| FR-4 | Comms templates with variable substitution from run metadata. | P2 | |
| FR-5 | Optional HTTP hooks for alert silencing APIs (provider-specific). | P2 | |
| FR-6 | Document promotion/rollback hooks per registry or deploy target. | P2 | |

## Non-functional requirements

| ID | Requirement | How verified |
| --- | --- | --- |
| NFR-1 | Retry with exponential backoff (1s → 2s → 4s → 8s → 16s, max 5 attempts) on notification delivery failures. Deliver failures are logged and visible in the UI. | Tests |
| NFR-2 | Rate limit outbound notifications: **60 notifications/minute per org**, **10/minute per channel**. Excess notifications are queued up to a 5-minute window; beyond that, they are dropped with a logged warning. | Config + load test |

## Security and privacy

- Webhook URLs and tokens are **secrets** (PRD 060).
- **Threats:** SSRF via user-supplied notification URLs. **Mitigation:** validate outbound URLs against a server-side allowlist or denylist (block RFC 1918, link-local, and loopback ranges: `10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`, `169.254.0.0/16`, `::1`, `fc00::/7`). Resolve hostname at URL save time and at dispatch time; reject if the resolved IP is in a blocked range. Do not follow redirects to untrusted destinations.

## Dependencies and assumptions

- **Depends on:** PRD 010, PRD 050 run lifecycle events.
- **Assumes:** Operators supply channel credentials.

## Success metrics

| Metric | Target | Measurement |
| --- | --- | --- |
| Notification delivery success rate | TBD high | Metrics |
| Mean time to announce release | TBD | Survey |

## Rollout and migration

- Ship Slack + generic webhook first; add vendors incrementally.

## Open questions

- Approval gates and SOC2-style evidence ([../constraints.md](../constraints.md) deferrals).

## Out of scope / future work

- Built-in status page hosting.
