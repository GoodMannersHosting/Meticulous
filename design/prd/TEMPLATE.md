# PRD: <short title>

**Status:** Draft  
**Owner:** <name or team>  
**Last updated:** YYYY-MM-DD  
**Related:** <issues, ADRs, other PRDs>

## Context

<Why this work exists. Link customer pain, security posture, or roadmap phase.>

## Problem statement

<One tight paragraph: what is broken or missing today?>

## Goals

- <Measurable or verifiable outcome>
- <...>

## Non-goals

- <Explicitly out of scope to prevent scope creep>
- <...>

## Users and stakeholders

| Role | Need |
| --- | --- |
| <e.g. platform admin> | <...> |
| <e.g. developer> | <...> |

## User stories (optional)

- As a `<role>`, I want `<capability>` so that `<benefit>`.
- <...>

## Functional requirements

| ID | Requirement | Priority (P0/P1/P2) | Notes |
| --- | --- | --- | --- |
| FR-1 | <...> | P0 | <...> |

## Non-functional requirements

Reference [../constraints.md](../constraints.md) and add **initiative-specific** items only (latency, availability, audit, data residency).

| ID | Requirement | How verified |
| --- | --- | --- |
| NFR-1 | <...> | <test, metric, review> |

## Security and privacy

- Trust boundaries and secret handling: <...>
- AuthZ model (who can do what): <...>
- Threats considered: <...>

## Dependencies and assumptions

- **Depends on:** <services, other PRDs, migrations>
- **Assumes:** <e.g. NATS JetStream available, OIDC issuer configured>

## Success metrics

| Metric | Target | Measurement |
| --- | --- | --- |
| <e.g. time to first successful agent join> | <...> | <...> |

## Rollout and migration

<Feature flags, backwards compatibility, data backfills, comms to operators.>

## Open questions

<Bullets; move resolved items to [../open-questions.md](../open-questions.md) or an ADR when decided.>

## Out of scope / future work

- <...>
