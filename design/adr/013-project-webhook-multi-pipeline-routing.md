# ADR-013: Project-level SCM webhooks → one or many pipelines

**Status:** Proposed  
**Date:** 2026-04-07  
**PRDs:** [020-scm-webhooks-triggers](../prd/020-scm-webhooks-triggers.md), [010-tenancy-rbac-api-tokens](../prd/010-tenancy-rbac-api-tokens.md) (RBAC for target CRUD)

## Context

Inbound SCM webhooks are registered per **project** in `webhook_registrations` ([007 migration](../../crates/met-store/migrations/007_webhooks.sql)). The public URL embeds that row’s UUID (`/api/v1/webhooks/{provider}/{org_id}/{id}`). **ADR-005** specifies verification, deduplication, and fork/PR policy before any dispatch.

Today there is **no first-class link** from a registration to `pipelines`. The `triggers` table is one row per `pipeline_id`, but it is **not** joined to `webhook_registrations` in the schema. Handlers in [`webhooks.rs`](../../crates/met-api/src/routes/webhooks.rs) verify and parse events but **do not create `runs`**. Operators who want one GitHub webhook to start **several** pipelines today would need **duplicate webhooks** (different URLs) or a single orchestrator pipeline.

We need a **project-scoped** routing layer: **one HMAC-verified delivery** may enqueue **zero or more** runs, each for a pipeline in the **same project**, with optional **per-pipeline filters** (branch, event, later path).

## Decision

1. **New table `webhook_registration_targets`** — Each row connects one `webhook_registration_id` to one `pipeline_id`, with `enabled`, `filter_config` (JSONB, default `{}`), timestamps, and `UNIQUE (webhook_registration_id, pipeline_id)`.

2. **Invariant** — Every target’s pipeline MUST satisfy `pipelines.project_id = webhook_registrations.project_id`. Enforce via DB trigger, constrained write transaction, or validated application layer with tests (v1 may start with application validation).

3. **Dispatch (after ADR-005 gates)** — For a verified, non-duplicate delivery:
   - Resolve registration by path UUID; ensure `org_id` matches `projects.org_id` for the registration’s project (policy: prefer opaque `404`/`403` to limit enumeration).
   - Apply registration-level `events` filter, then for each **enabled** target apply `filter_config` (`branches`, `events` override; `paths` deferred to a later phase unless payload supports it).
   - For each matching target: create a run via **`RunRepo::create_full`** with `pipeline_id`, `org_id` from the project, `triggered_by` including provider and delivery id, and SCM metadata (`branch`, `commit_sha`, `trigger_data`).
   - **Fan-out policy:** **best-effort** — failure on one target logs and continues; HTTP **`200 OK`** with a JSON body listing `run_ids` and any per-target errors, so GitHub retries do not fire on partial success (document explicitly; avoid `5xx` unless the entire handler fails).

4. **Deduplication** — Reuse **ADR-005** `webhook_deliveries` (`provider`, `delivery_id`). On conflict: return **`200 OK`**, `duplicate: true`, **no new runs**. Multi-run idempotency: either one delivery row links to the **first** run only and duplicates skip all, or store an array / child table of `run_id`s; **pick one implementation** and document (recommend: on duplicate, short-circuit before any `INSERT` into `runs`).

5. **Coexistence with `triggers`** — **Option A (recommended v1):** `webhook_registration_targets` is the **source of truth** for SCM URL → pipelines. Keep `triggers` for manual/schedule/tag kinds; `runs.trigger_id` may be `NULL` for webhook-origin runs until optional synthetic trigger rows exist. **Option B:** mirror each target to a `triggers` row and store `trigger_id` on the target for strict lineage; choose if billing/audit requires it.

6. **Admin API** (authenticated, project RBAC):
   - `GET/POST/PATCH/DELETE` … `/api/v1/projects/{project_id}/webhooks/{registration_id}/targets`
   - Extend `POST /api/v1/projects/{project_id}/scm/setup` to accept optional `targets: [{ pipeline_id, filter_config? }]` when creating a registration.

7. **Response shape** — Extend webhook JSON (e.g. `run_ids`, `duplicate`, `targets_matched`) per OpenAPI; no secrets in response.

### Non-goals (initial release)

- Fan-out **across projects** from one registration.
- Replacing the entire `triggers` model in one step.
- Full GitHub App install UI (setup route may still return URL + secret only).

## Consequences

### Positive

- One SCM webhook configuration; monorepo can run `lint`, `build-api`, `build-agent` in parallel on the same push.
- Clear DB invariant ties routing to **project** boundaries.
- Aligns **ADR-005** “mapping to runs” with an explicit resolver.

### Negative / migration

- New migration and repos; handlers grow coordination logic.
- **Partial failure** semantics must be documented for operators (logs + response body).
- `webhook_deliveries` must exist before production multi-run dispatch (already required by ADR-005).

### Implementation phasing (informative)

| Phase | Deliverable |
| --- | --- |
| P0 | Migration + target CRUD + project/pipeline validation |
| P1 | GitHub handler wired to `create_full` + tests |
| P2 | GitLab/Bitbucket parity + metrics |
| P3 | Path filters + richer var injection |

## Threat model

- **Assets:** Ability to enqueue arbitrary pipeline runs in a project; webhook URL discoverability.
- **Adversaries:** Unauthenticated POST to webhook URL; replay; collaborator abusing fork PR policy (**ADR-005**).
- **Mitigations:** HMAC/token verification, dedupe, org path check, **ADR-005** fork tier, RBAC on target CRUD, rate limits (**ADR-005**), audit log of `(registration_id, delivery_id, run_ids)`.
- **Residual risk:** Weak or missing verifier in non-production configs; Bitbucket until verified per ADR-005.

**Certificates:** Not applicable to this ADR (TLS termination is infrastructure). If webhook ingress uses mTLS or custom trust stores, follow workspace certificate verification guidance separately.

## References

- [ADR-005: SCM inbound webhook security](005-scm-webhook-security.md) — verification, dedupe, fork policy  
- [ADR-010: Project and SCM data model](010-project-and-scm-data-model.md) — project scope  
- [ADR-001: Run and job lifecycle](001-run-and-job-lifecycle.md) — runs and scheduling expectations  
- [`crates/met-api/src/routes/webhooks.rs`](../../crates/met-api/src/routes/webhooks.rs)  
- [`crates/met-api/src/routes/pipelines.rs`](../../crates/met-api/src/routes/pipelines.rs) — `POST …/trigger`  
- [`crates/met-store/src/repos/runs.rs`](../../crates/met-store/src/repos/runs.rs) — `create_full`  
- Supersedes working notes: ~~[`design/plans/project-webhook-multi-pipeline-routing.md`](../plans/project-webhook-multi-pipeline-routing.md)~~ (now points here)
