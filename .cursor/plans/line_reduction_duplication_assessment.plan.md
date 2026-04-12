# Line reduction & duplication assessment (plan only)

**Goal:** Identify **duplicative or consolidatable** code paths to target **hundreds–thousands** of lines removed or avoided, without changing behavior. This document is an **assessment and phased plan**—no implementation is implied.

**Rough scale (for context):** ~**88k** lines of Rust under `crates/` (all `*.rs`); frontend has several **1k–3k+** line Svelte pages plus a **~1k** line API client.

**Already completed (baseline):** Pagination cursor merge, `/proc/net/tcp` hex parsing, `split_key_value`, store `FromStr` for roles—see [`design/notes/rust-dedup-crate-sweep.md`](../../design/notes/rust-dedup-crate-sweep.md).

---

## 1. Largest files (where duplication often hides)

These are **primary** places to look for repeated patterns (not all are “bad”; some are legitimately dense domains).

| Area | File (approx. lines) | Why it matters |
| --- | --- | --- |
| API | `crates/met-api/src/routes/admin.rs` (~3k) | Many similar admin handlers + auth + repo wiring |
| API | `crates/met-api/src/routes/webhooks.rs` (~2k) | Webhook + GitHub/GitLab-style branching |
| API | `crates/met-api/src/routes/runs.rs` (~1.5k) | Run listing, jobs, logs, WS—many routes |
| Store | `crates/met-store/src/repos/runs.rs` (~1.6k) | Heavy SQL; repeated query shapes |
| Parser | `crates/met-parser/src/parser.rs` (~1.4k) | Pipeline/workflow resolution |
| Controller | `crates/met-controller/src/grpc.rs` (~1.75k) | gRPC/service glue |
| Frontend | `frontend/src/routes/projects/[id]/+page.svelte` (~3.2k) | UI + data loading in one file |
| Frontend | `frontend/src/routes/pipelines/[id]/+page.svelte` (~3.2k) | Same pattern |
| Frontend | `frontend/src/lib/api/client.ts` (~1k) | One method per endpoint |

**Note:** Splitting files **moves** lines; it does not **shrink** them. Line reduction requires **shared helpers**, **macros**, **codegen**, or **deleting** redundant paths.

---

## 2. Tier A — plausible line savings (ordered by impact vs. risk)

### A1. `met-store`: `project_members` vs `pipeline_members` (~2–300 lines each)

**Duplication:** Same three-tier role (`admin` / `developer` / `readonly`), same `rank()`, same `FromStr` table, very similar `effective_role_*` + `*_with_visibility` SQL (user + group UNION), similar `list_members`/`add_member` shapes.

**Directions (pick one):**

- **Macro** for `FromStr` + `rank` + `max_*_role` (small net win, **~40–80 lines**).
- **Shared trait + generic repo helpers** for “user+group role rows → max role” where SQL differs only by table/column (larger refactor, **~100–200 lines** if done carefully).
- **Single internal enum** `MemberRole` with two newtype wrappers only where API must differ (riskier for API clarity).

**Risk:** Medium—ACL semantics must stay identical; heavy tests.

---

### A2. Parallel RBAC models: `met-secrets` vs `met-api` (~100–500 lines potential)

**Duplication:** `met_secrets::rbac::Role` and `met_api::auth::rbac::ApiRole` both encode a **numeric hierarchy** (PlatformAdmin → … → Viewer) with overlapping concepts; comments already say they should match.

**Direction:** Move **one** canonical role enum + ordering + `includes()` / `has_at_least` into **`met-core`** (or a tiny `met-rbac` crate), with `met-secrets` and `met-api` as thin adapters. **Savings:** highly dependent on how much logic is shared vs. secrets-specific policy evaluation.

**Risk:** Medium–high—serde, error types, and dependency direction must be designed so `met-core` does not pull heavy deps.

---

### A3. `met-api` admin (and similar route modules): handler boilerplate

**Duplication:** ~50+ `async fn` handlers in `admin.rs` alone; repeated patterns: `Auth` → load org → repo call → map `StoreError` → JSON.

**Direction:** Small **private** helpers (e.g. `require_org_admin`, `json_ok`, typed wrappers for common repo errors) or a thin **internal trait** for “admin routes that need the same prelude.” **Realistic savings:** **~100–400 lines** across admin + a few other huge route files if applied consistently.

**Risk:** Low–medium—easy to over-abstract; keep helpers boring.

---

### A4. `met-store` `runs.rs` repo: repeated SQL / mapping

**Duplication:** Large file with many queries; likely repeated `SELECT … JOIN` shapes and row mapping.

**Direction:** Internal **query builders** or small **fn** per concern (e.g. “base run row + pipeline name”), `FromRow` where structs align. **Savings:** **~150–400 lines** (estimate) if many queries are copy-pasted.

**Risk:** Medium—performance and index use must stay explicit.

---

### A5. Frontend: `client.ts` + giant `+page.svelte` files

**Duplication:** `client.ts` is mostly **one wrapper per endpoint**; pages duplicate **load/error/submit** patterns.

**Directions:**

- **OpenAPI → codegen** for the client (largest potential **~1k+ lines** removed from hand-written client, but **new** tooling and CI).
- **Extract** repeated page sections into components (pipeline/project pages): **~200–800 lines** per pair of pages if patterns are truly shared—not guaranteed without reading each page.

**Risk:** Codegen is high upfront; component extraction is medium (design consistency).

---

## 3. Tier B — mostly organizational (small net line win)

- **Split** `admin.rs` / `webhooks.rs` into `admin/users.rs`, `admin/groups.rs`, etc. **Net lines ~0**, but makes **A3** easier to apply.
- **Clippy-driven** cleanups (`collapsible_if`, `field_reassign_with_default`) across `met-api`: **tens** of lines, not hundreds.
- **Test fixtures:** builders for `RawJob` / `RawWorkflowDef` in `met-parser` tests to remove repeated struct literals — **~50–150 lines** in tests only.

---

## 4. Tier C — usually low ROI for “line count”

- **Protobuf / generated code:** Do not hand-merge; count separately if you measure “repo size.”
- **Schema / `met-parser` schema.rs:** Large but **structural**; dedup only where multiple structs share fields with identical validation.
- **Broader “generic CRUD” frameworks** for REST: often **add** abstraction lines before they pay off.

---

## 5. Suggested sequencing (for assessment / PRs)

1. **Confirm** ACL duplication (`A1`) with a side-by-side diff of `project_members` vs `pipeline_members` methods + SQL.
2. **Prototype** one `met-api` route helper module used from **3–5** admin handlers (`A3`) and measure line delta.
3. **Audit** `met_secrets::rbac` vs `met_api::auth::rbac` for a shared core type (`A2`)—spike in a branch.
4. **Sample** `met-store` `runs.rs` for top 3 repeated SQL blocks (`A4`).
5. **Frontend:** decide whether **codegen** (`A5`) is worth the process change vs. **component** extraction on the two largest pages.

---

## 6. Out of scope / do not merge blindly

- Anything touching **secrets, tokens, or ACL** without parity tests and security review (workspace rules: no hardcoded credentials; treat consolidation as a security-sensitive change).
- **Certificate** handling and crypto policy (per `.cursor/rules`).

---

## 7. Expected outcome

- **Realistic “few hundred lines”** without codegen: **A1 + A3 + A4** (or a subset), plus test builders.
- **“Few thousand lines”** usually requires **frontend codegen** or **major** store/API consolidation—not a single PR.

This plan is intentionally **conservative** on numbers until each area is diffed in detail.
