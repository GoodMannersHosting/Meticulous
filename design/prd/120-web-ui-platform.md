# PRD: Web UI (runs, logs, variables, grouping, DAG)

**Status:** Draft  
**Owner:** TBD  
**Last updated:** 2026-03-31  
**Related:** [../user-interface.md](../user-interface.md), PRD 010, PRD 030, PRD 050, PRD 100

## Context

The **SvelteKit** web UI is the primary surface for operators and developers ([../architecture.md](../architecture.md)). This PRD covers **run-centric** experiences called out in [../user-interface.md](../user-interface.md) that are **not** owned by PRD 010 (identity/admin), PRD 020 (GitHub App), or PRD 090 (supply-chain views).

## Problem statement

Users cannot effectively operate pipelines without readable **logs**, **run history**, **variable** overrides, **grouping**, **DAG** context, and **release-window** scheduling visibility.

## Goals

- **Build logs in browser** with streaming and historical playback (PRD 050).
- **Diff log output** against a previous run for faster regression spotting ([../user-interface.md](../user-interface.md)).
- **CRUD variables for runs** in the UI with RBAC from PRD 010.
- **Group** job/workflow runs (by pipeline, branch, trigger, custom tags TBD).
- **DAG viewer** for workflows/pipelines/job dependencies (data from PRD 030).
- **Run scheduling** UX aligned with release windows (backend PRD 100).

## Non-goals

- Replacing the CLI for all power-user automation.
- Mobile-native apps.

## Users and stakeholders

| Role | Need |
| --- | --- |
| Developer | Watch logs, tweak run variables, compare failures. |
| Release manager | See grouped runs and schedule in window. |
| Viewer | Read-only DAG and logs. |

## Functional requirements

| ID | Requirement | Priority | Notes |
| --- | --- | --- | --- |
| FR-1 | Live log viewer with pause, search, and download. | P0 | [../user-interface.md](../user-interface.md). |
| FR-2 | Select baseline run and show diff vs current log text. | P1 | [../user-interface.md](../user-interface.md). |
| FR-3 | Create/update/delete run-scoped variables before/during queued runs per policy. | P1 | [../user-interface.md](../user-interface.md). |
| FR-4 | Filter and group run list by dimensions (pipeline, status, time). | P1 | [../user-interface.md](../user-interface.md). |
| FR-5 | Render pipeline DAG with status overlays. | P1 | PRD 030. |
| FR-6 | Schedule or defer runs within allowed release window (when PRD 100 ships). | P2 | [../user-interface.md](../user-interface.md). |

## Non-functional requirements

| ID | Requirement | How verified |
| --- | --- | --- |
| NFR-1 | Log viewer remains responsive for large streams (virtualization). | UX test |
| NFR-2 | WCAG-oriented keyboard navigation for core run pages. | A11y audit TBD |

## Security and privacy

- Respect RBAC: no log or variable access without permission (PRD 010).
- **Threats:** XSS via malicious log lines — sanitize using an allowlist of safe ANSI escape sequences; strip raw HTML from log output before rendering.
- **WebSocket authentication:** The WebSocket log stream endpoint must validate the same JWT/API token as HTTP routes. Send the token in the initial HTTP upgrade request (e.g. `Authorization` header via subprotocol convention or a one-time ticket issued by the API). Do not accept unauthenticated WebSocket upgrades. Ticket-based auth (short-lived one-time token scoped to a `job_run_id`) is preferred to avoid long-lived credentials on the wire.

## Dependencies and assumptions

- **Depends on:** PRD 050 APIs/WebSocket; PRD 030 graph API; PRD 010 auth.
- **Assumes:** Browser supports required features for streaming.

## Success metrics

| Metric | Target | Measurement |
| --- | --- | --- |
| Task success: find failing step | TBD | UX study |
| Log viewer error rate | TBD low | Client metrics |

## Rollout and migration

- Ship log viewer first; diff and DAG in follow-on milestones.

## Open questions

- Exact grouping dimensions and saved views.

## Out of scope / future work

- In-UI pipeline YAML editor with full LSP.
