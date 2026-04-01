# PRD: Tenancy, RBAC, and API tokens

**Status:** Draft  
**Owner:** TBD  
**Last updated:** 2026-03-31  
**Related:** [../architecture.md](../architecture.md), PRD 020 (SCM), PRD 120 (Web UI)

## Context

Meticulous separates work by **organization (tenant)** and **project**, with pipelines, secrets, variables, and triggers scoped beneath projects. Operators need predictable **identity**, **grouping**, and **machine access** (API tokens) without weakening the security-first posture. See domain model in [../architecture.md](../architecture.md).

## Problem statement

Without clear tenancy and authorization, teams cannot safely share a platform or audit who changed what; without scoped API tokens, automation either over-privileges scripts or bypasses the control plane.

## Goals

- Model **orgs and projects** with owners and membership aligned to product needs.
- Provide **RBAC** (or equivalent) so roles limit CRUD on projects, pipelines, secrets, and fleet operations.
- Support **API token** issuance per user, per group, and **platform-admin** paths, with rotation and revocation.

## Non-goals

- Full enterprise IdP customization beyond OIDC/JWT assumptions in architecture (defer specifics to implementation ADRs).
- Billing or usage metering per tenant.

## Users and stakeholders


| Role                       | Need                                                             |
| -------------------------- | ---------------------------------------------------------------- |
| Platform admin             | Create orgs, manage global policies, issue admin-scoped tokens.  |
| Project owner / maintainer | Manage project members, project settings, project-scoped tokens. |
| Developer                  | Run pipelines, view runs, use personal tokens for API/CLI.       |


## Functional requirements


| ID   | Requirement                                                                | Priority | Notes                                                                         |
| ---- | -------------------------------------------------------------------------- | -------- | ----------------------------------------------------------------------------- |
| FR-1 | CRUD organizations and projects with stable identifiers.                   | P0       | Owner model TBD: [../open-questions.md](../open-questions.md).                |
| FR-2 | User and group entities; assign membership to org/project with roles.      | P0       | UI: Group and User Management ([../user-interface.md](../user-interface.md)). |
| FR-3 | API token create/list/revoke; scopes tied to user or group or admin.       | P0       | UI: API Token management ([../user-interface.md](../user-interface.md)).      |
| FR-4 | User profile basics (identity display, preferences).                       | P1       | UI: User Profiles ([../user-interface.md](../user-interface.md)).             |
| FR-5 | Audit log of security-relevant changes (tokens, membership, role changes). | P1       | Correlation IDs per [../architecture.md](../architecture.md).                 |


## Non-functional requirements

Reference [../constraints.md](../constraints.md).


| ID    | Requirement                                          | How verified            |
| ----- | ---------------------------------------------------- | ----------------------- |
| NFR-1 | Token secrets shown once at creation; stored hashed. | Security review, tests. |
| NFR-2 | Permission checks on every mutating API path.        | Integration tests.      |


## Security and privacy

- **Trust boundaries:** Human users vs service tokens; admin vs tenant scope.
- **AuthZ:** Deny-by-default; explicit roles for secrets and global workflow management.
- **Threats:** Token theft, privilege escalation via group membership, orphaned tokens after offboarding.

## Dependencies and assumptions

- **Depends on:** HTTP API identity integration (OIDC/JWT per architecture).
- **Assumes:** Single Postgres metadata store for RBAC state.

## Success metrics


| Metric                               | Target | Measurement                |
| ------------------------------------ | ------ | -------------------------- |
| Time to grant project access         | TBD    | UX study / support tickets |
| Unauthorized access attempts blocked | 100%   | Audit + tests              |


## Rollout and migration

- Feature-flag optional strict RBAC modes; document breaking changes to token formats if any.

## Open questions

- ~~Project owner: user vs group~~ **Resolved:** `project_members` join table, no `owner_user_id` column, creator seeded as `admin`. See [../open-questions.md](../open-questions.md).
- ~~Project fields~~ **Resolved:** `slug` (unique per org, immutable), `display_name`, `description`, `visibility` (private|internal), `archived`. See [../open-questions.md](../open-questions.md).

## Out of scope / future work

- Fine-grained ABAC beyond roles; cross-org federation.

