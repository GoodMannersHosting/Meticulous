# ADR-021: Resource visibility, pipeline ACL, and admin role split

**Status:** Proposed  
**Date:** 2026-04-11  
**PRDs:** [010](../prd/010-tenancy-rbac-api-tokens.md), [060](../prd/060-secrets-providers-and-per-job-pki.md)

## Context

The current authorization model (ADR-008, ADR-010) provides project membership with four roles and API token scopes, but lacks several capabilities required before later platform features can land:

1. **No resource visibility tiers.** Projects and pipelines are either visible to all authenticated org members or not accessible at all. There is no way to share read-only metadata publicly (e.g. for open-source pipelines) or restrict a pipeline to a small team.
2. **No pipeline-level ACL.** All access is derived from project membership. A developer with project write access can modify any pipeline in that project, even sensitive deployment pipelines they should not control.
3. **No distinction between platform-wide admin and org admin.** The `admin` role conflates org administration with full content access. Compliance scenarios require an admin who can manage permissions and view metadata without reading pipeline definitions, run logs, or secret values.
4. **No unauthenticated access.** Public-facing read-only access to metadata (build status, pipeline names) is impossible without an API token.

Pipeline environments (ADR-016), OIDC identity (ADR-017), and remote validation (ADR-019) all depend on this authorization model being in place first.

## Decision

### Three-tier resource visibility

Introduce a `resource_visibility` enum shared by projects and pipelines:

| Tier | Who can see | What they see |
| --- | --- | --- |
| `public` | Anyone (including unauthenticated, when globally enabled) | Read-only metadata: name, description, status, run outcomes |
| `authenticated` | Any authenticated user in the org | Metadata + definitions + run logs (subject to role) |
| `private` | Only explicit members (project or pipeline) | Everything the member's role permits |

Default for all new resources: `authenticated` (no behavioral change for existing deployments).

### Pipeline membership and role inheritance

Pipelines gain their own member list. A pipeline member has one of three roles:

| Role | Permissions |
| --- | --- |
| `admin` | Full control: edit definition, manage members, manage secrets, trigger, approve |
| `developer` | Edit definition, trigger runs, view logs and secrets metadata |
| `readonly` | View definition, view run status and logs |

Pipeline membership has two sources:

1. **Direct:** Explicitly added to `pipeline_members`.
2. **Inherited:** Automatically synced from the parent project's `project_members`. Inherited members cannot be removed at the pipeline level; they can only be overridden to a higher role.

Effective pipeline role = `max(inherited_from_project, direct_pipeline_role)`, using the ordering `readonly < developer < admin`.

### Admin role split

Add `super_admin` to the existing `permission_role` enum. The platform has two admin tiers:

| Role | Scope | Content access |
| --- | --- | --- |
| `platform_admin` (existing `admin`) | Org-wide | Metadata and permission management only. Cannot read pipeline definitions, run logs, secret values, or artifact content. |
| `super_admin` (new) | Org-wide | Unrestricted. Full access equivalent to `has_permission("*")`. Intended for break-glass and initial setup. |

The `platform_admin` restriction is enforced at the API layer: routes that return content (definitions, logs, secrets) reject requests where the caller's access derives solely from `platform_admin` status and not from project/pipeline membership.

### Unauthenticated access

A new platform-level toggle `allow_unauthenticated_access` (default `false`). When enabled, requests without a valid session or API token can access `public` resources via read-only metadata endpoints. An `OptionalAuth` extractor returns `Option<CurrentUser>`; routes check visibility before deciding whether authentication is required.

### Data model

#### Migration `042_resource_visibility_and_pipeline_acl.sql`

```sql
CREATE TYPE resource_visibility AS ENUM ('public', 'authenticated', 'private');

ALTER TABLE projects
    ADD COLUMN visibility resource_visibility NOT NULL DEFAULT 'authenticated';

ALTER TABLE pipelines
    ADD COLUMN owner_type text NOT NULL DEFAULT 'user',
    ADD COLUMN owner_id   text NOT NULL DEFAULT '',
    ADD COLUMN visibility  resource_visibility NOT NULL DEFAULT 'authenticated';

CREATE TYPE pipeline_principal_type AS ENUM ('user', 'group');
CREATE TYPE pipeline_role AS ENUM ('admin', 'developer', 'readonly');

CREATE TABLE pipeline_members (
    id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    pipeline_id    uuid NOT NULL REFERENCES pipelines(id) ON DELETE CASCADE,
    principal_type pipeline_principal_type NOT NULL,
    principal_id   uuid NOT NULL,
    role           pipeline_role NOT NULL,
    inherited      boolean NOT NULL DEFAULT false,
    created_at     timestamptz NOT NULL DEFAULT now(),
    UNIQUE (pipeline_id, principal_type, principal_id)
);
CREATE INDEX idx_pipeline_members_pipeline  ON pipeline_members(pipeline_id);
CREATE INDEX idx_pipeline_members_principal ON pipeline_members(principal_type, principal_id);

ALTER TYPE permission_role ADD VALUE 'super_admin';

-- Backfill pipeline ownership from parent project
UPDATE pipelines p
SET owner_type = pr.owner_type, owner_id = pr.owner_id
FROM projects pr
WHERE p.project_id = pr.id AND p.owner_id = '';
```

#### Migration `043_platform_settings_unauth_access.sql`

```sql
ALTER TABLE platform_settings
    ADD COLUMN IF NOT EXISTS allow_unauthenticated_access boolean NOT NULL DEFAULT false;
```

### Core model updates

In `crates/met-core/src/models/`:

- New `ResourceVisibility` enum: `Public`, `Authenticated`, `Private`.
- Add `visibility: ResourceVisibility` to `Project`, `CreateProject`, `UpdateProject`.
- Add `owner_type`, `owner_id`, `visibility` to `Pipeline`, `CreatePipeline`, `UpdatePipeline`.
- Add `SuperAdmin` to `PermissionRole`.
- New `PipelineMember` struct: `id`, `pipeline_id`, `principal_type`, `principal_id`, `role`, `inherited`, `created_at`.

### Store layer

In `crates/met-store/src/repos/`:

- **`project_access.rs`:** Add `effective_role_for_user(pool, user_id, project_id) -> Option<ProjectRole>` that checks `project_members` and `org_members.default_project_role`. Add `list_members`, `remove_member` (with owner-cannot-be-removed guard).
- **New `pipeline_access.rs`:** `effective_role_for_user(pool, user_id, pipeline_id) -> Option<PipelineRole>` — computes `max(project_inherited, pipeline_direct)`. `add_member`, `remove_member` (rejects removing inherited members), `sync_inherited_from_project(pipeline_id)` (called on project member changes).
- **`projects.rs`:** On `create`, auto-insert the creator as `project_members(role=admin)`.
- **`pipelines.rs`:** On `create`, auto-insert owner as direct admin + call `sync_inherited_from_project` to seed inherited members.

### API authorization

In `crates/met-api/src/`:

- **`project_access.rs`:** `effective_project_role(pool, user, project_id) -> Option<ProjectRole>` — handles visibility: `public` resources return `readonly` for unauthenticated users (when enabled); `authenticated` resources return the computed role for any authenticated org member; `private` resources return `None` unless the user is an explicit member.
- **`pipeline_access.rs`:** `effective_pipeline_role(pool, user, pipeline_id) -> Option<PipelineRole>` — same visibility logic at the pipeline level, then `max(project_inherited, direct)`.
- **`is_super_admin(user) -> bool`** helper.
- **`is_platform_admin(user) -> bool`** helper — returns true if the user has org-level admin but not `super_admin`. Used to restrict content-bearing routes.
- **`OptionalAuth` extractor:** Returns `Option<CurrentUser>`. Falls back to `None` only when `allow_unauthenticated_access` is enabled and the request has no valid credentials.

### API routes

| Method | Path | Description | Auth |
| --- | --- | --- | --- |
| `GET` | `/projects/{id}/members` | List project members | Project admin |
| `POST` | `/projects/{id}/members` | Add project member | Project admin |
| `DELETE` | `/projects/{id}/members/{user_id}` | Remove project member (not owner) | Project admin |
| `GET` | `/pipelines/{id}/members` | List pipeline members (shows inherited flag) | Pipeline admin |
| `POST` | `/pipelines/{id}/members` | Add direct pipeline member | Pipeline admin |
| `DELETE` | `/pipelines/{id}/members/{member_id}` | Remove direct pipeline member (rejects inherited) | Pipeline admin |
| `PATCH` | `/projects/{id}` | Update visibility + other fields | Project admin |
| `PATCH` | `/pipelines/{id}` | Update visibility + other fields | Pipeline admin |

`platform_admin` can access member management routes but cannot access routes that return pipeline definitions, run logs, or secret values.

Public metadata routes (when `allow_unauthenticated_access = true`):

| Method | Path | Description |
| --- | --- | --- |
| `GET` | `/projects` | List projects with `visibility = 'public'` (metadata only) |
| `GET` | `/projects/{id}` | Project metadata if public |
| `GET` | `/pipelines/{id}/status` | Pipeline status and latest run outcome if public |

### Invariants

1. The resource owner is always an `admin` member and cannot be removed or downgraded.
2. Inherited pipeline members cannot be removed at the pipeline level; override to a higher role only.
3. `platform_admin` can manage permissions and view metadata but cannot read definitions, logs, secrets, or artifacts.
4. `super_admin` retains full access (`has_permission("*")`).
5. Unauthenticated access is globally disabled by default. When enabled, only `public` resources are visible, and only metadata endpoints respond.
6. Changing a project's visibility to `private` does not remove existing members; it only prevents non-members from discovering the project.
7. `sync_inherited_from_project` runs transactionally on project member changes to keep pipeline membership consistent.

## Consequences

### Positive

- Teams can restrict sensitive deployment pipelines to a subset of project members.
- Open-source or shared pipelines can expose build status publicly without exposing secrets or definitions.
- `platform_admin` can manage access without being able to read pipeline content, supporting compliance separation of duties.
- Pipeline environments (ADR-016) and remote validation (ADR-019) inherit this permission model without needing their own authorization layer.

### Negative

- Two new migrations; `ALTER TYPE ... ADD VALUE` for `super_admin` is not transactional in Postgres (must be in its own transaction or use the pre-existing migration runner workaround).
- Every API route must now compute effective role through a two-level lookup (project + pipeline). Caching effective roles per request amortizes the cost.
- Inherited membership sync adds write load on project member changes. Bounded by the number of pipelines per project (typically < 100).
- `platform_admin` content restriction may confuse operators who expect admin to mean "full access." Documentation and UI indicators are required.

### Migration notes

- All existing projects default to `visibility = 'authenticated'`, preserving current behavior.
- All existing pipelines default to `visibility = 'authenticated'` with empty owner fields; the backfill UPDATE populates `owner_type`/`owner_id` from the parent project.
- `pipeline_members` starts empty; no inherited members are synced automatically by the migration. The application layer syncs inherited members on first pipeline access or via a post-migration background task.
- `allow_unauthenticated_access` defaults to `false`; no behavioral change until an operator explicitly enables it.

## Threat model

- **Assets:** Pipeline definitions (may contain infrastructure topology); run logs (may contain sensitive output); secret metadata (names, provider types); member lists (organizational structure); unauthenticated access toggle.
- **Adversaries:** Org member escalating from `readonly` to `admin` on a pipeline; `platform_admin` attempting to read secret values; unauthenticated user probing for private resources; IDOR via swapped `pipeline_id` in member management routes.
- **Mitigations:**
  - Effective role is computed server-side from the join of `project_members`, `pipeline_members`, and `org_members`; the client never asserts their own role.
  - `platform_admin` content restriction is enforced in the API layer with explicit deny on content routes, not rely-on-absence of permission grants.
  - Unauthenticated requests are rejected with 401 unless `allow_unauthenticated_access` is enabled AND the resource is `public`. Timing-safe response to prevent resource enumeration.
  - Member management routes validate `pipeline_id` ownership within the caller's org before any mutation.
  - Owner immutability prevents lockout: the last admin on a project or pipeline cannot be removed.
  - Inherited member removal is blocked at the application layer to prevent ACL desynchronization.
- **Residual risk:** A `super_admin` can grant themselves any access. This is by design (break-glass), but operators should restrict `super_admin` assignment and audit its usage via the audit log (ADR-008). Self-service visibility changes (project admin can set `public`) may expose metadata unintentionally; UI should warn when changing from `private`/`authenticated` to `public`.

**Certificates:** Not directly applicable. API routes serving public metadata must be behind HTTPS; TLS certificate health should be verified per workspace certificate rules.

## References

- [ADR-008](008-tenancy-rbac-api-tokens.md) — RBAC scopes, API tokens, audit events
- [ADR-010](010-project-and-scm-data-model.md) — project membership, secret scope hierarchy
- [ADR-016](016-pipeline-environments.md) — pipeline environments; approval gates use this permission model
- [ADR-017](017-oidc-workload-identity.md) — OIDC claims include org/project context established here
- [`crates/met-core/src/models/`](../../crates/met-core/src/models/) — domain model structs
- [`crates/met-store/src/repos/`](../../crates/met-store/src/repos/) — store layer repos
- [`crates/met-api/src/extractors/auth.rs`](../../crates/met-api/src/extractors/auth.rs) — auth extractors
- [`crates/met-api/src/project_access.rs`](../../crates/met-api/src/project_access.rs) — project authorization
