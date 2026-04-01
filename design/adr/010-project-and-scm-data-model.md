# ADR-010: Project and SCM data model

**Status:** Proposed
**Date:** 2026-03-31
**PRDs:** [010](../prd/010-tenancy-rbac-api-tokens.md), [020](../prd/020-scm-webhooks-triggers.md), [060](../prd/060-secrets-providers-and-per-job-pki.md)

## Context

Three previously open questions converge on the same schema: project ownership model, secret scope hierarchy, and SCM repo attachment. Getting these tables wrong before the first public API release forces a breaking migration. All three are resolved here as a single ADR because they share foreign key relationships.

## Decision

### 1. Project membership (no single-column owner)

Do **not** add `owner_user_id` to `projects`. Use a join table:

```sql
-- Effective role = max(org_members.default_project_role, project_members.role)
CREATE TABLE project_members (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id  uuid NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    user_id     uuid NOT NULL REFERENCES users(id)    ON DELETE CASCADE,
    role        text NOT NULL CHECK (role IN ('viewer','developer','maintainer','admin')),
    created_at  timestamptz NOT NULL DEFAULT now(),
    UNIQUE (project_id, user_id)
);
```

The creator of a project is seeded as `admin`. `org_members` gains `default_project_role text DEFAULT 'none'`; authorization computes `max(default_project_role, project_members.role)` where `none < viewer < developer < maintainer < admin`. If `owner_user_id` already exists on `projects` in a migration, drop it in the same migration that creates `project_members`.

### 2. Project fields

```sql
CREATE TABLE projects (
    id           uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id       uuid NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    slug         text NOT NULL CHECK (slug ~ '^[a-z0-9][a-z0-9-]{0,62}$'),
    display_name text NOT NULL,
    description  text,
    visibility   text NOT NULL DEFAULT 'private' CHECK (visibility IN ('private','internal')),
    archived     boolean NOT NULL DEFAULT false,
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now(),
    UNIQUE (org_id, slug)
);
```

`slug` is immutable after creation (enforce in application layer; a rename requires creating a new project). `display_name` collisions within an org are allowed but surfaced as a UI warning. No `public` visibility in v1.

### 3. Secret scope (two-tier, environment reserved)

```sql
CREATE TABLE secrets (
    id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    scope_type     text NOT NULL CHECK (scope_type IN ('org','project')),
    scope_id       uuid NOT NULL,   -- org_id or project_id
    name           text NOT NULL CHECK (name ~ '^[A-Z_][A-Z0-9_]{0,127}$'),
    provider       text NOT NULL,   -- 'built_in' | 'aws_sm' | 'vault' | 'gcp_sm' | 'k8s' | 'openbao'
    provider_ref   text,            -- ARN, Vault path, K8s secret name, etc.
    encrypted_val  bytea,           -- non-null only when provider = 'built_in'
    environment_id uuid,            -- NULL in v1; FK to environments table in v2
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now(),
    UNIQUE (scope_type, scope_id, name, environment_id)
);
```

Resolution order at dispatch time: project-scoped (matching environment if set, else null) → org-scoped (same). No global tier; platform-wide shared secrets are org-scoped in a dedicated platform org. Built-in secrets (`provider = 'built_in'`) are hard-capped at 64 per project (enforced in application layer). Shadow migration: set `provider` and `provider_ref` on an existing row; the resolver switches to the external provider immediately. The pipeline YAML `secrets:` block is unchanged.

### 4. SCM repo attachment

```sql
CREATE TABLE project_repos (
    id            uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id    uuid NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    scm_provider  text NOT NULL CHECK (scm_provider IN ('github','gitlab','bitbucket','gitea','plain_git')),
    clone_url     text NOT NULL,
    ssh_url       text,
    default_branch text NOT NULL DEFAULT 'main',
    clone_depth   int CHECK (clone_depth IS NULL OR clone_depth > 0),  -- NULL = full
    fork_policy   text NOT NULL DEFAULT 'no_secrets'
                  CHECK (fork_policy IN ('block','no_secrets','allow_secrets')),
    webhook_id    text,   -- SCM-side webhook registration ID; used for de-registration
    created_at    timestamptz NOT NULL DEFAULT now(),
    -- v1: exactly one repo per project
    UNIQUE (project_id)   -- drop this constraint in v2 for monorepo support
);
```

No credentials stored here. SCM provider auth references a `secrets` row (e.g., GitHub App installation token stored as a project-scoped secret). `webhook_id` enables one-click de-registration when a project is deleted or the repo is detached. Monorepo path filtering belongs in the pipeline trigger YAML (`paths:` field), not in this table.

## Consequences

- Authorization logic must compute `max(default_project_role, project_members.role)` consistently; a shared `AuthContext::effective_project_role(user_id, project_id)` function in `met-core` prevents drift.
- `secrets` table requires a Postgres row-level security policy or application-level org check on every query; `scope_id` alone is not sufficient — `scope_type` must be validated to prevent an org_id being passed as a project scope.
- `project_repos.fork_policy` drives the fork/PR trust tier classification in [ADR-005](005-scm-webhook-security.md). The webhook handler must read this column before dispatching a run for a fork PR event.
- Adding the `environments` table in v2 requires only a migration to add the `environments` table and a foreign key on `secrets.environment_id`; no existing data is invalidated.

## Threat model

- **Assets:** `secrets.provider_ref` (may reveal internal Vault paths); `project_repos.clone_url` (repo topology).
- **Adversaries:** User A reading secrets scoped to Org B via a crafted API request (IDOR); a developer escalating from `viewer` to `admin` via a race on `project_members`.
- **Mitigations:** `scope_type + scope_id` double-check on every secrets query; optimistic concurrency on role updates (version column per [ADR-001](001-run-and-job-lifecycle.md) pattern); `fork_policy` default `no_secrets` — least privilege by default.
- **Residual risk:** `allow_secrets` on a project with untrusted contributors is an operator misconfiguration risk; `met lint` includes a rule requiring a `# risk-acknowledged` comment when this is set (per [ADR-009](009-pipeline-linter-architecture.md)).

## References

- [open-questions.md](../open-questions.md) Data model section (resolved)
- [ADR-008](008-tenancy-rbac-api-tokens.md) for token and role model
- [ADR-005](005-scm-webhook-security.md) for fork_policy usage in webhook handling
- [PRD-010](../prd/010-tenancy-rbac-api-tokens.md), [PRD-020](../prd/020-scm-webhooks-triggers.md), [PRD-060](../prd/060-secrets-providers-and-per-job-pki.md)
