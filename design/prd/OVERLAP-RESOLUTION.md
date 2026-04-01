# PRD overlap resolution

Single place to record **scope ownership** where two PRDs could be read as owning the same word (“webhook”, “schedule”) or the same data (**tools** vs **executed binaries**). Cross-PRD **dependencies** (e.g. RBAC in PRD 010) are intentional and not listed here.

## Glossary

| Term | Meaning | Owner PRD |
| --- | --- | --- |
| **Inbound SCM webhook** | Git host delivers events to Meticulous (push, PR, tag). | **020** |
| **Outbound notification webhook** | Meticulous calls Slack, Teams, generic HTTPS URL for alerts. | **100** |
| **Pipeline trigger schedule** | Cron/time-based start of a pipeline from its definition. | **020** |
| **Release / maintenance window** | Coordinated time for releases, comms, optional alert silencing. | **100** |
| **Run deferral / queue UX** | User defers a run or places it in a window via UI policy. | **120** (policy may come from **100**) |

## Ownership table

| Topic | PRDs | Resolution |
| --- | --- | --- |
| **Webhooks** | 020, 100 | **020** = inbound SCM only. **100** = outbound notifications only. Do not use “webhook” without one of these qualifiers. |
| **Scheduling** | 020, 100, 120 | **020** = trigger schedules. **100** = release windows and coordinated comms. **120** = UI to defer/schedule runs under policy. |
| **Live logs** | 050, 120, 110 | **050** = transport, storage, redaction. **120** = browser UX. **110** may assume logs exist; no duplicate FR rows for streaming. |
| **Tool / binary identity** | 070, 090 | **070** = collection + raw storage contract. **090** = derived tool index, search, blast radius. Canonical event/schema: **ADR-006** (when drafted). |
| **Remote cache** | 030, 050 | **030** = YAML declares cache refs and runner context. **050** = execution data plane (restore/save, keys, isolation). Details: **ADR-001** and related execution notes. |
| **Run/release notifications** (Slack, Teams, …) | [features.md](features.md) lists under Triggers | Product behavior is **PRD 100 only**; [features.md](features.md) is high-level only. |

## Additional ownership clarifications (added 2026-03-31)

| Topic | PRDs | Resolution |
| --- | --- | --- |
| **Debug CLI vs run logs** | 050, 130 | **050** = log transport, storage, and masking pipeline. **130** = CLI UX for read-only log retrieval via the same 050 APIs. PRD 130 has no duplicate FR rows for the log streaming protocol. |
| **Pipeline lint vs pipeline authoring** | 030, 130 | **030** = YAML schema validation (parse errors, DAG structure). **130 / ADR-009** = security-rule lint (`met lint`) and AI suggestions (`met suggest`). Server-side lint gate at ingestion is a 030 concern (FR-2); the rule engine itself is ADR-009. |
| **SBOM component storage vs search** | 050, 090 | **050** = artifact upload pipeline (blobs to object storage). **090** = SBOM metadata ingestion, component indexing, blast-radius queries. `sbom_reports` and `sbom_components` tables are owned by 090; the upload path that lands the blob is 050. |
| **Project membership vs token scopes** | 010, 008 | **PRD-010** = product-level roles and membership UX. **ADR-008** = implementation: `project_members` table, effective role computation, token permission scopes. Both reference each other; ADR-010 is the authoritative schema. |
| **SCM attachment vs webhook triggers** | 020 | `project_repos` attachment (ADR-010) stores the repo connection and `fork_policy`. Webhook trigger *events* and signature verification are PRD-020 / ADR-005. Both own different columns: `project_repos` has connection metadata; `webhook_registrations` has the per-trigger secret and delivery log. |

## Traceability to FR IDs (optional)

| FR / area | Primary PRD |
| --- | --- |
| SCM signature, replay, GitHub App ingress | 020 FR-1–FR-6 |
| Channel delivery, comms templates | 100 FR-1–FR-6 |
| Log stream API, artifact upload | 050 FR-1–FR-4 |
| Log viewer, DAG overlay, run variables UI | 120 |
| Exec + network telemetry | 070 |
| SBOM, tool DB, blast UI | 090 |

See also [VERIFICATION.md](VERIFICATION.md) for done criteria per PRD.
