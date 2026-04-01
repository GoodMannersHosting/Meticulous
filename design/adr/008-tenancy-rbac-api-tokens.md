# ADR-008: Tenancy, RBAC, and API tokens

**Status:** Proposed  
**Date:** 2026-03-31  
**PRDs:** [010](../prd/010-tenancy-rbac-api-tokens.md)

## Context

Human and machine access use JWT claims and **API tokens** validated in [crates/met-api/src/auth/api_token.rs](../../crates/met-api/src/auth/api_token.rs) and [extractors/auth.rs](../../crates/met-api/src/extractors/auth.rs). Permissions are string **scopes** in a `HashSet` (e.g. `pipelines:read`, wildcard `*`).

## Decision

1. **Permission model (current)** — Continue **resource:action** style scopes stored on API tokens and embedded in JWT `permissions`. Wildcard `*` grants all actions for automation accounts only; avoid assigning to human users in production.

2. **Enforcement** — Every mutating route checks `CurrentUser::has_permission` or `has_all_permissions` for the resource under the **organization/project** extracted from path or body. Cross-tenant access is always denied unless future super-admin role is explicitly scoped and audited.

3. **API token format** — `met_<token_id>_<secret>`; only **hash** of full token stored ([`hash_join_token`](../../crates/met-core)); matches workspace **no hardcoded credentials** rule.

   **Token expiry defaults:**
   - Human user API tokens: **90 days**, renewable; operators may configure shorter or disable renewal.
   - Machine / bot API tokens (service accounts): **90 days** with mandatory rotation reminder at 80 days.
   - Agent join tokens: **7 days** default, **30 days** maximum (requires explicit operator opt-in for > 7 days). Single-use preferred for ephemeral agent fleets.
   - Agent JWTs: 24h ephemeral, 7d long-lived (with approval per PRD 110).
   - Human session JWTs (web UI): 1h with silent refresh via OIDC refresh token flow.

4. **Audit events** — Log structured events for: token created/revoked, role or membership changed, failed auth spikes, secret accessed/modified. Store in Postgres `audit_log` table with an append-only constraint (no UPDATE/DELETE). **Schema** aligned with [OCSF](https://schema.ocsf.io/) event classes for SIEM interoperability:
   - `event_time` (timestamptz, UTC), `event_type` (string enum: `auth.login`, `token.created`, `token.revoked`, `permission.changed`, `secret.accessed`, etc.)
   - `actor_type` (`user` | `agent` | `api_token` | `system`), `actor_id`, `actor_ip`
   - `resource_type`, `resource_id`, `org_id`, `project_id` (nullable)
   - `outcome` (`success` | `failure`), `metadata` (JSONB for event-specific fields)
   Export to SIEM via OTLP log pipeline ([ADR-007](007-observability-opentelemetry.md)) using JSON encoding of the same schema.

5. **Project membership** — **Do not** use `projects.owner_user_id`. Use a `project_members(project_id, user_id, role enum(viewer|developer|maintainer|admin), created_at)` join table. The creator is seeded as `admin` at project creation. Effective role for authz = `max(org_members.default_project_role, project_members.role)`, where `org_members.default_project_role` defaults to `none`. This model supports multi-owner with no future breaking migration (additional admin rows, no schema change). If `owner_user_id` already exists in migrations, drop it in the same migration that creates `project_members` before any public API is released.

## Consequences

- New routes must declare required permission in router layer or extractor.
- Fine-grained ABAC (row-level beyond org/project) is out of scope for this ADR.

## Threat model

- **Assets:** Tokens, JWTs, org membership.
- **Adversaries:** Stolen API token, IDOR via swapped `project_id`, privilege escalation via `*`.
- **Mitigations:** Hash-at-rest, short JWT TTL, scope least privilege, revoke path, tests for IDOR (see [VERIFICATION.md](../prd/VERIFICATION.md)).
- **Residual risk:** Mis-scoped `*` tokens; operational review during onboarding.

## References

- [met-api/src/routes/tokens.rs](../../crates/met-api/src/routes/tokens.rs)
- [ADR-001](001-run-and-job-lifecycle.md) for org/project foreign keys
