# Settings parity: UI vs API vs deployment

This inventory tracks where configurable behavior is defined so we can converge on **admin APIs + OpenAPI** as the source of truth.

| Area | UI-only / local | API (OpenAPI) | Deployment (YAML / secrets) |
|------|-----------------|---------------|-----------------------------|
| API token max TTL, credential rate limits | — | `GET/PATCH /api/v1/admin/policy` | Default row in `org_policy` / migrations |
| Per-user API tokens (cap, deactivate, revoke, delete) | `/settings/security` | `GET/POST/PATCH/DELETE /api/v1/tokens` (+ deactivate/reactivate/revoke) | — |
| Org token inventory | `/admin/policy` | `GET /api/v1/admin/tokens` | — |
| Archived projects/pipelines | `/admin/archive` | `GET /api/v1/admin/archive`, unarchive + pipeline purge routes | — |
| Meticulous Apps enable/disable | App detail | `PATCH /api/v1/admin/meticulous-apps/{id}` | — |
| Project app installation | — | `POST /api/v1/projects/{id}/meticulous-apps/installations` | — |
| Webhook registration delete | Project → Triggers | `DELETE /api/v1/projects/{id}/webhooks/{registration_id}` | — |
| Blast radius search | `/security/blast-radius` | `GET /api/v1/security/blast-radius?q=` | — |
| Profile / appearance / notification toggles | `/settings` | Mostly **not** backed by API yet | — |
| `MET_BUILTIN_SECRETS_MASTER_KEY` | — | Consumed by `met-api` / `met-controller` | Kubernetes `secretKeyRef` (see labops `SECRETS.md`) |

**Follow-ups:** Wire notification preferences and theme to explicit APIs if product requires server-side persistence; keep this table updated when adding new toggles.
