# PRD: Supply chain (SBOM, tool inventory, blast radius)

**Status:** Draft  
**Owner:** TBD  
**Last updated:** 2026-03-31  
**Related:** [../features.md](../features.md), [../security.md](../security.md), [../user-interface.md](../user-interface.md), PRD 070, PRD 050

## Context

Meticulous prioritizes **supply-chain visibility**: **SBOM** generation and diff, **tool inventory** with versions/SHA per run, **attestation** storage where applicable ([../features.md](../features.md), [../security.md](../security.md)). The UI includes **SBOM change/diff viewer**, **tool search**, **blast radius** from a compromised tool SHA, and **flaky step** highlighting in reusable workflows ([../user-interface.md](../user-interface.md)).

## Problem statement

Teams cannot respond to CVEs or compromised tools without knowing **what shipped**, **what ran**, and **which workflows** were affected; flaky steps hide reliability debt.

## Goals

- Store and version **SBOM** artifacts linked to runs/releases; support **diff** between runs or tags.
- Maintain a **tool database** (identity, version, digest) derived from runs and attestations.
- Provide **blast-radius** queries: given a tool digest, list workflows/runs/agents touched (5W1H style questions per [../user-interface.md](../user-interface.md)).
- Surface **flaky** step signals (intermittent failure rate) without root-cause AI ([../user-interface.md](../user-interface.md)).

## Non-goals

- Guarantee SBOM for every ecosystem without native tooling.
- Auto-remediate vulnerabilities (separate workflow product).

## Users and stakeholders

| Role | Need |
| --- | --- |
| Security engineer | Answer impact of a bad tool or package. |
| Release manager | Compare SBOM between releases. |
| Developer | See which steps are statistically flaky. |

## Functional requirements

| ID | Requirement | Priority | Notes |
| --- | --- | --- | --- |
| FR-1 | Attach SBOM blobs to runs from build steps (e.g. Docker attestations). | P0 | [../pipelines.md](../pipelines.md) docker example. |
| FR-2 | API and UI for SBOM diff between two runs or artifacts. | P0 | UI [../user-interface.md](../user-interface.md). |
| FR-3 | Ingest tool identity + version + digest into searchable store. | P0 | |
| FR-4 | Blast-radius report from tool digest query. | P1 | UI [../user-interface.md](../user-interface.md). |
| FR-5 | Flaky heuristic: flag steps in reusable workflows exceeding threshold. | P2 | Threshold: failure rate > 20% over the most recent 50 runs with a minimum of 10 total runs in the window. Detection only, not diagnosis. |

## Non-functional requirements

| ID | Requirement | How verified |
| --- | --- | --- |
| NFR-1 | SBOM storage sized with lifecycle policies. | Ops guide |
| NFR-2 | Index queries complete under SLA for typical org size. | Load test TBD |

## Security and privacy

- SBOMs may contain sensitive package names; **RBAC** from PRD 010.
- **Threats:** Poisoned SBOM upload; validate signatures/attestations when present.

## Dependencies and assumptions

- **Depends on:** PRD 050 artifact pipeline; PRD 070 for executed binary cross-checks.
- **SBOM interchange format:** Support both **SPDX 2.3+** (JSON or tag-value) and **CycloneDX 1.5+** (JSON). CycloneDX has broader tooling support for container and language ecosystems (Syft, cdxgen); SPDX is NTIA-compliant and required by some government procurement. Store both as blobs; index metadata in Postgres. Do not mandate one format from build steps — accept either and normalize for diff/search.

## Success metrics

| Metric | Target | Measurement |
| --- | --- | --- |
| Runs with SBOM attached | > 80% of container-build runs in steady state | Analytics |
| Time to answer blast-radius query | < 5 s for orgs with ≤ 10 000 runs in index | UX (P95) |

## Rollout and migration

- Start with container builds; expand to language ecosystems incrementally.

## Open questions

- Attestation verification policy and key management ([../open-questions.md](../open-questions.md)).
- AI-assisted pipeline recommendations vs rule linter ([../open-questions.md](../open-questions.md)).

## Out of scope / future work

- Global public vulnerability database mirroring inside the product.
