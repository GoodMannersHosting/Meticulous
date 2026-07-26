# API reference

The Meticulous REST API provides programmatic access to all platform functionality. All routes are versioned under `/api/v1`.

---

## Interactive documentation

| Resource | URL |
|----------|-----|
| **Swagger UI** | [`/api/docs`](/api/docs) |
| **OpenAPI JSON** | [`/api/v1/openapi.json`](/api/v1/openapi.json) |

The Swagger UI lets you explore and try out every endpoint directly from your browser. Use an API token (see [Authentication](authentication.md)) to authorize requests.

---

## Base URL

All API requests are relative to your deployment origin:

```
https://ci.example.com/api/v1/
```

The OpenAPI spec's `servers` entry defaults to `/` so Swagger UI resolves paths correctly when hosted on the same origin.

---

## API categories

| Tag | Description |
|-----|-------------|
| `health` | Liveness and readiness probes |
| `auth` | Login, logout, session, OIDC flows |
| `organizations` | Org CRUD and settings |
| `projects` | Project CRUD |
| `pipelines` | Pipeline CRUD, trigger, validate |
| `runs` | Run management, logs, DAG, artifacts |
| `agents` | Agent registration and management |
| `tokens` | API token management |
| `secrets` | Secret CRUD (project and org scope) |
| `stored_secrets` | Platform-stored encrypted secrets |
| `variables` | Variable management |
| `workflows` | Reusable workflow catalog |
| `triggers` | Pipeline trigger configuration (webhooks, schedule) |
| `artifacts` | Build artifact and SBOM management |
| `debug` | Debug session management |
| `dashboard` | Org dashboard stats |

---

## Response format

All responses use JSON. Errors follow a consistent schema:

```json
{
  "error": "not_found",
  "message": "Pipeline 01HXYZ not found",
  "request_id": "01JABCD..."
}
```

The `X-Request-Id` response header matches the `request_id` in error bodies — useful for correlating errors with server logs.

---

## Pagination

List endpoints use keyset (cursor) pagination:

```http
GET /api/v1/pipelines?limit=25&after=01HXYZ...
```

Responses include a `meta` object:

```json
{
  "data": [...],
  "meta": {
    "has_more": true,
    "next_cursor": "01HABC..."
  }
}
```

---

## Rate limiting

The API enforces per-IP and per-token rate limits. When a limit is exceeded, the response is `429 Too Many Requests` with a `Retry-After` header.
