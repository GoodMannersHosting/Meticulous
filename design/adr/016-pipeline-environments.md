# ADR-016: Pipeline environments and approval gates

**Status:** Proposed  
**Date:** 2026-04-11  
**PRDs:** [060](../prd/060-secrets-providers-and-per-job-pki.md), [010](../prd/010-tenancy-rbac-api-tokens.md)

## Context

Pipelines currently have no concept of named deployment targets. Secrets and variables are scoped to organizations or projects, with no way to differentiate between staging and production credentials. There is no mechanism for approval gates that pause a pipeline before deploying to a sensitive environment.

The plan introduces **pipeline environments**: named deployment targets (e.g. `staging`, `production`) that bundle environment-scoped variables, environment-scoped secrets, branch restrictions, and approval gates.

## Decision

### Data model

#### Environments table (migration `043_environments.sql`)

```sql
CREATE TABLE environments (
    id                     uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id                 uuid NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    project_id             uuid NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name                   text NOT NULL CHECK (name ~ '^[a-z0-9][a-z0-9-]{0,62}$'),
    display_name           text NOT NULL,
    description            text,
    require_approval       boolean NOT NULL DEFAULT false,
    required_approvers     int NOT NULL DEFAULT 1,
    approval_timeout_hours int NOT NULL DEFAULT 72,
    allowed_branches       text[],
    auto_deploy_branch     text,
    variables              jsonb NOT NULL DEFAULT '{}',
    tier                   text NOT NULL DEFAULT 'development'
                           CHECK (tier IN ('development','staging','production','custom')),
    created_at             timestamptz NOT NULL DEFAULT now(),
    updated_at             timestamptz NOT NULL DEFAULT now(),
    UNIQUE (project_id, name)
);
```

- `name` is the machine identifier used in YAML; `display_name` is the human-readable label.
- `tier` is informational metadata for UI grouping and OIDC claims (ADR-017); it does not enforce behavior beyond what the other columns specify.
- `allowed_branches` is a nullable text array. When non-null, only runs triggered from a matching branch ref can deploy to this environment. Patterns support `*` glob within a single path segment (e.g. `release/*`).
- `auto_deploy_branch` triggers automatic deployment when the specified branch is pushed (requires `require_approval = false` or auto-approval for the branch).

#### Environment-scoped secrets (migration `044_secrets_environment_scope.sql`)

```sql
ALTER TABLE builtin_secrets ADD COLUMN environment_id uuid REFERENCES environments(id);
```

Update the existing unique index on `builtin_secrets` to include `environment_id` (allowing the same secret name to exist at both project scope and environment scope):

```sql
DROP INDEX IF EXISTS builtin_secrets_unique_name;
CREATE UNIQUE INDEX builtin_secrets_unique_name
    ON builtin_secrets (org_id, COALESCE(project_id, '00000000-0000-0000-0000-000000000000'),
                        COALESCE(environment_id, '00000000-0000-0000-0000-000000000000'),
                        name);
```

#### Secret resolution order

When resolving a secret reference for a job running in a named environment, the resolution order is:

1. **Environment-scoped** secret (matching `environment_id`)
2. **Project-scoped** secret (`environment_id IS NULL`, matching `project_id`)
3. **Org-scoped** secret (`environment_id IS NULL`, `project_id IS NULL`, matching `org_id`)

The first match wins. This allows environments to override project-level secrets (e.g. different AWS credentials for staging vs production) without duplicating every secret.

### YAML surface

```yaml
workflow_invocations:
  - id: deploy-staging
    workflow: project/deploy
    environment: staging
  - id: deploy-production
    workflow: project/deploy
    environment: production
    needs: [deploy-staging]
```

In `crates/met-parser/src/schema.rs`, add `environment: Option<String>` to `RawWorkflowInvocation`.

### Approval gates

When a job targets an environment with `require_approval = true`:

1. The engine pauses the run before dispatching any jobs in that environment.
2. An `approval_required` event is emitted (NATS, stored in a new `environment_approvals` table for audit).
3. The run enters a `waiting_for_approval` state visible in the UI.
4. An authorized user (per RBAC — ADR-008) calls the approval API endpoint.
5. Once `required_approvers` count is met, the engine resumes dispatch.
6. If `approval_timeout_hours` elapses without approval, the run is cancelled.

#### Approval tracking (migration `043_environments.sql`, same file)

```sql
CREATE TABLE environment_approvals (
    id              uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    run_id          uuid NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    environment_id  uuid NOT NULL REFERENCES environments(id) ON DELETE CASCADE,
    approved_by     uuid REFERENCES users(id),
    decision        text NOT NULL CHECK (decision IN ('approved', 'rejected')),
    comment         text,
    decided_at      timestamptz NOT NULL DEFAULT now(),
    UNIQUE (run_id, environment_id, approved_by)
);
```

### Variable overlay

Environment variables from `environments.variables` are merged into the execution context with the following precedence (highest wins):

1. Step-level variables
2. Job-level variables
3. Environment variables (`environments.variables`)
4. Pipeline-level variables
5. Project-level variables

This matches the existing variable scoping model, inserting environments between pipeline and job scope.

### Branch restrictions

In `crates/met-engine/src/executor.rs`, before dispatching jobs in a named environment:

1. Look up the environment by `(project_id, name)`.
2. If `allowed_branches` is non-null, match the trigger ref against the patterns. Reject with a clear error if no pattern matches.
3. If `require_approval` is true, enter the approval gate flow.
4. Merge environment variables into the execution context.
5. Pass `environment_id` to `met-secret-resolve` for scoped secret resolution.

### API routes

New file `crates/met-api/src/routes/environments.rs`:

| Method | Path | Description |
| --- | --- | --- |
| `GET` | `/api/v1/projects/{id}/environments` | List environments |
| `POST` | `/api/v1/projects/{id}/environments` | Create environment |
| `PATCH` | `/api/v1/projects/{id}/environments/{env_id}` | Update environment |
| `DELETE` | `/api/v1/projects/{id}/environments/{env_id}` | Delete environment |
| `POST` | `/api/v1/runs/{run_id}/environments/{env_name}/approve` | Approve deployment |
| `POST` | `/api/v1/runs/{run_id}/environments/{env_name}/reject` | Reject deployment |

All routes require project-level RBAC (ADR-008). Approval/rejection requires `environment:approve` permission.

## Consequences

### Positive

- Secrets and variables can differ per deployment target without pipeline-level conditionals.
- Approval gates provide a human checkpoint before production deployments.
- Branch restrictions prevent accidental deployment from feature branches.
- Environment names appear in OIDC token claims (ADR-017), enabling fine-grained external access control.

### Negative

- Two new DB migrations; `builtin_secrets` unique index changes require careful migration ordering.
- Approval gates add latency to pipeline runs (by design, but operators must account for timeout behavior).
- Environment-scoped secrets increase the resolution search space (three-level lookup vs two-level).

### Migration notes

- Existing secrets with `environment_id IS NULL` continue to work unchanged.
- The unique index change is backward-compatible: existing rows all have `environment_id IS NULL`, so uniqueness is preserved.
- No proto changes required; environment resolution is server-side only. The engine attaches resolved secrets and variables to `JobDispatch` as before.

## Threat model

- **Assets:** Environment-scoped secrets (production credentials); approval gate integrity (preventing unauthorized deployments); environment configuration.
- **Adversaries:** Attacker with project write access attempting to bypass approval gates; branch name spoofing; unauthorized secret access via environment escalation.
- **Mitigations:**
  - Approval requires explicit `environment:approve` RBAC permission, separate from general project write access.
  - Branch restrictions are evaluated server-side against the verified trigger ref (not user-supplied input).
  - Secret resolution order is deterministic and auditable; environment-scoped secrets cannot be accessed from jobs running without that environment.
  - `environment_approvals` table provides an audit trail of who approved/rejected and when.
  - Approval timeout prevents indefinite run stalls and stale approval requests.
  - Environment deletion cascades secret scope but does not delete the secrets themselves (they become project-scoped by setting `environment_id = NULL` via a pre-delete hook, or are deleted — operator choice configured per org).
- **Residual risk:** An attacker with admin RBAC can approve their own deployments. Enforce separation of duties via `required_approvers > 1` and org policy. Self-approval prevention is deferred to a future enhancement.

**Certificates:** Not directly applicable. If environment-scoped secrets include TLS certificates, they should be verified per workspace certificate rules before use.

## References

- [ADR-004](004-secrets-and-per-job-pki.md) — secret resolution pipeline; environment adds a scope layer
- [ADR-008](008-tenancy-rbac-api-tokens.md) — RBAC model; `environment:approve` permission
- [ADR-010](010-project-and-scm-data-model.md) — project/secret scope hierarchy
- [ADR-017](017-oidc-workload-identity.md) — OIDC token `environment` claim depends on this ADR
- [`crates/met-store/src/repos/builtin_secrets.rs`](../../crates/met-store/src/repos/builtin_secrets.rs) — secret storage
- [`crates/met-secret-resolve/src/resolve.rs`](../../crates/met-secret-resolve/src/resolve.rs) — resolution logic
- [`crates/met-engine/src/executor.rs`](../../crates/met-engine/src/executor.rs) — pre-dispatch logic
- [Platform evolution plan](../../.cursor/plans/platform_evolution_plan_a66c44a0.plan.md) — Feature 4
