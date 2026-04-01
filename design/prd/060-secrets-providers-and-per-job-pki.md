# PRD: Secrets providers and per-job PKI

**Status:** Draft  
**Owner:** TBD  
**Last updated:** 2026-03-31  
**Related:** [../features.md](../features.md), [../security.md](../security.md), [../agents.md](../agents.md), [../pipelines.md](../pipelines.md), [../../skills/meticulous-agent-security-invariants/SKILL.md](../../skills/meticulous-agent-security-invariants/SKILL.md)

## Context

Secrets must be **scoped** to the active job, **revocable**, and **encrypted** for the agent hop ([../../skills/meticulous-agent-security-invariants/SKILL.md](../../skills/meticulous-agent-security-invariants/SKILL.md)). Integrations target **Vault, OpenBao, AWS Secrets Manager, Kubernetes secrets** ([../features.md](../features.md)); **OIDC-style** access to providers mirrors GitHub Actions intent ([../agents.md](../agents.md)). **Per-job PKI** flow is outlined in [../agents.md](../agents.md).

## Problem statement

Long-lived secrets on runners and plaintext on the wire increase blast radius; running without required secrets wastes resources and confuses users.

## Goals

- **Pre-run validation:** refuse to start if required secret references are missing or unresolved ([../features.md](../features.md), [../pipelines.md](../pipelines.md)).
- Resolve secrets from external providers with **least privilege** and audit metadata.
- Deliver secret material to agents using **per-job** keys: server encrypts per agent/job public key; agent verifies integrity (e.g. digests) per design notes.
- Support **OIDC/JWT** patterns for cloud and vault auth where applicable.
- Optional **built-in** secret storage with UX that **discourages** production use ([../open-questions.md](../open-questions.md)).

## Non-goals

- Storing plaintext secrets in Postgres for production (anti-goal).
- Full HashiCorp policy generator UX in v1 (noted as direction in [../security.md](../security.md) for later).

## Users and stakeholders

| Role | Need |
| --- | --- |
| Security engineer | External stores, rotation, audit. |
| Pipeline author | Declarative secret references in YAML. |
| Agent | Decrypt only current job material. |

## Functional requirements

| ID | Requirement | Priority | Notes |
| --- | --- | --- | --- |
| FR-1 | Declare secrets in pipeline with provider-specific refs (e.g. AWS ARN). | P0 | Example [../pipelines.md](../pipelines.md). |
| FR-2 | Block run start when required secrets cannot be resolved. | P0 | [../pipelines.md](../pipelines.md). |
| FR-3 | Per-job key exchange and encrypted secret payload to agent. | P0 | Flow [../agents.md](../agents.md). |
| FR-4 | Integrations: Vault/OpenBao, AWS SM, GCP Secret Manager, K8s secrets. | P0 | GCP SM was absent from the original list; added for parity. |
| FR-5 | Redact secrets in logs and API responses (masked fields). | P0 | [../pipelines.md](../pipelines.md). |
| FR-6 | OIDC/JWT auth to providers comparable in intent to GHA OIDC. | P1 | [../agents.md](../agents.md). |

## Non-functional requirements

| ID | Requirement | How verified |
| --- | --- | --- |
| NFR-1 | No secret plaintext in NATS or persistent job rows. | Audit |
| NFR-2 | Clock sync (NTP) enforced or warned for agents ([../agents.md](../agents.md)). | Health check |

## Security and privacy

- **Threats:** Agent compromise, provider credential theft, confused deputy on OIDC.
- **Controls:** Short-lived job keys, revocation (PRD 110), minimal secret surface in messages.

## Dependencies and assumptions

- **Depends on:** PRD 010 for who may define secrets; PRD 050 for injection into steps.
- **Assumes:** Providers reachable from control plane with outbound credentials configured.

## Success metrics

| Metric | Target | Measurement |
| --- | --- | --- |
| Runs blocked on missing secrets | 100% before agent dispatch | Metrics |
| Secret leakage incidents | Zero | Incidents |

## Rollout and migration

- Document migration from built-in store to external providers when built-in exists.

## Open questions

- ~~Built-in secret UX~~ **Resolved:** UI warning on creation, 64-secret-per-project cap, visual badge, shadow migration pattern (provider mapping overrides built-in without re-keying pipelines). See [../open-questions.md](../open-questions.md).
- ~~Secret scope hierarchy~~ **Resolved:** Two tiers (org + project) in v1; `environment_id nullable` reserved for v2 environment-scoped secrets. See [../open-questions.md](../open-questions.md).
- OpenBao/Vault AppRole and policy generator scope ([../security.md](../security.md)): still open.

## Out of scope / future work

- AWS Roles Anywhere for all workloads ([../security.md](../security.md)) without scoped design.
