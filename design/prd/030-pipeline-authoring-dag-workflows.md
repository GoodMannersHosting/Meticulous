# PRD: Pipeline authoring, DAG, and reusable workflows

**Status:** Draft  
**Owner:** TBD  
**Last updated:** 2026-03-31  
**Related:** [../features.md](../features.md), [../pipelines.md](../pipelines.md), PRD 020, PRD 040, PRD 050

## Context

Pipelines are defined as **jobs** (DAG) containing **steps**, with **reusable workflows** at `global/` (platform) or `project/` scope and explicit **versions** ([../architecture.md](../architecture.md)). **YAML** is the primary authoring format; **TypeScript** and **Python** parsers support generated definitions ([../features.md](../features.md)).

## Problem statement

Ad-hoc CI scripts do not scale; teams need composable, reviewable definitions with clear **dependencies**, **parallelism**, and **failure propagation**, plus **runner** selection that matches agent capabilities.

## Goals

- Parse and validate pipeline definitions from YAML; support TS/Python generation paths.
- Resolve **DAG** edges, parallel stages, and failure semantics (fail-fast vs continue).
- Resolve **reusable workflows** with version pins and input/output wiring between composed units.
- Express **runner selection** via pool tags (arch, GPU, OS) aligned with agent pools.

## Non-goals

- GitHub Actions YAML compatibility as a full import format (migration aids may come later per [../vision.md](../vision.md)).
- Visual pipeline editor as the only authoring mode (text-first).
- **Remote cache** backend semantics, key isolation, and restore/save behavior: **PRD 050** owns execution-plane cache behavior. This PRD only declares **authoring-time** references (cache key expressions, restore keys, paths) and runner context. Cache key expressions must never interpolate secret values or secret-derived material; the engine must validate this at parse time. See [OVERLAP-RESOLUTION.md](OVERLAP-RESOLUTION.md).

## Users and stakeholders

| Role | Need |
| --- | --- |
| Pipeline author | Express reusable, reviewable workflows. |
| Platform admin | Curate `global/` workflows and versions. |
| Security reviewer | Understand blast radius of shared workflows. |

## Functional requirements

| ID | Requirement | Priority | Notes |
| --- | --- | --- | --- |
| FR-1 | Load pipeline from repo path (e.g. `.stable/*.yaml`) or API-stored definition. | P0 | Example: [../pipelines.md](../pipelines.md). |
| FR-2 | Validate schema: triggers, jobs, steps, `depends-on`, `runs-on` tags. | P0 | |
| FR-3 | Expand `workflow: global/...` and `workflow: project/...` with version. | P0 | |
| FR-4 | Pass inputs/outputs between workflow invocations deterministically. | P1 | |
| FR-5 | TypeScript and Python parsers emit canonical IR or YAML for the engine. | P2 | |

## Non-functional requirements

| ID | Requirement | How verified |
| --- | --- | --- |
| NFR-1 | Parse errors include file/line context for authors. | Golden tests |
| NFR-2 | DAG resolution bounded time; detect cycles with clear errors. | Unit tests |

## Security and privacy

- **AuthZ:** Only permitted roles may publish or pin `global/` workflows (PRD 010).
- **Threats:** Malicious workflow composition referencing unexpected secrets (coordination with PRD 060).

## Dependencies and assumptions

- **Depends on:** PRD 010 (project scope); PRD 020 (triggers).
- **Assumes:** Custom engine (not Tekton as core) per [../open-questions.md](../open-questions.md).

## Success metrics

| Metric | Target | Measurement |
| --- | --- | --- |
| Authoring error clarity | TBD | Qualitative feedback |
| Share of pipelines using reusable workflows | TBD | Product analytics |

## Rollout and migration

- Version pins required for `global/` references; warn on floating tags if ever allowed.

## Open questions

- Pipeline recommendation/linter ([../open-questions.md](../open-questions.md)).

## Out of scope / future work

- Dynamic graph mutation at runtime beyond declared DAG.
