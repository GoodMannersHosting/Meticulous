# Authentication

The Meticulous API supports three authentication methods, checked in order:

| Method | Header | Use case |
|--------|--------|----------|
| **Session JWT** | `Authorization: Bearer <jwt>` | Browser / CLI sessions after OIDC login |
| **API token** | `Authorization: Bearer met_<id>_<secret>` | Long-lived machine access; CI pipelines |
| **API token (alt scheme)** | `Authorization: Token met_<id>_<secret>` | Same as above; alternate `Token` scheme |

---

## Session JWT (OIDC login)

When a user logs in via the web UI or CLI, they authenticate through an OIDC provider (GitHub, GitLab, Google, or a generic OIDC issuer configured per-org). On successful login, the API issues a short-lived **Meticulous JWT**:

- **Lifetime**: 1 hour, with silent refresh via OIDC refresh token.
- **Claims**: `sub` (user ID), `org_id`, `roles[]`, `exp`.

```http
GET /api/v1/projects
Authorization: Bearer eyJhbGc...
```

JWTs are stateless — revocation happens at expiry or on logout (which invalidates the session server-side).

---

## API tokens

API tokens are suitable for long-lived machine access: CI scripts, CLI tools, and service accounts.

### Format

```
met_<token_id>_<secret>
```

Both parts are required. The token is presented either as `Bearer met_...` or `Token met_...`.

### Creating a token

**Via the UI:** Profile → API tokens → New token. Select scopes and optional expiry.

**Via the API:**

```http
POST /api/v1/tokens
Authorization: Bearer <session-jwt>
Content-Type: application/json

{
  "name": "CI pipeline token",
  "scopes": ["pipelines:read", "runs:trigger", "runs:read"],
  "expires_in_days": 90
}
```

Response:

```json
{
  "id": "01HXYZ...",
  "token": "met_01HXYZ_aBcDeFgH...",
  "scopes": ["pipelines:read", "runs:trigger", "runs:read"],
  "expires_at": "2026-07-15T00:00:00Z"
}
```

!!! warning
    The raw token value is only returned once at creation. Store it in a secrets manager — not in source code.

### Scopes

| Scope | Actions |
|-------|---------|
| `pipelines:read` | List and read pipeline definitions |
| `pipelines:write` | Create, update, delete pipelines |
| `runs:trigger` | Trigger pipeline runs |
| `runs:read` | Read run status, logs, artifacts |
| `runs:cancel` | Cancel running jobs |
| `secrets:read` | Read secret metadata (not values) |
| `secrets:write` | Create and update secrets |
| `tokens:read` | List own tokens |
| `tokens:write` | Create and revoke own tokens |
| `join_tokens:create` | Issue agent join tokens |
| `join_tokens:revoke` | Revoke agent join tokens |
| `agents:delete` | Soft-delete agents |
| `*` | All permissions (service accounts only) |

---

## Meticulous Apps (operator / machine JWTs)

Meticulous Apps are integration identities used by automated systems (e.g. the Kubernetes operator) that need to perform trusted actions without a human user session.

### Flow

```mermaid
sequenceDiagram
  participant O as Operator
  participant API as met-api

  O->>O: Load private key (RS256, from K8s Secret)
  O->>API: GET /api/v1/integration/public-context
  API-->>O: audience string (e.g. "meticulous-api.JOBS")
  O->>O: Mint JWT (iss=appId, sub=installationId, aud=audience, exp=8min)
  O->>API: POST /api/v1/integration/join-tokens\nAuthorization: Bearer <app-jwt>
  API->>API: Verify JWT signature via kid → public key lookup
  API-->>O: join_token
```

### JWT structure

```json
{
  "iss": "<meticulous-application-id>",
  "sub": "<installation-id>",
  "aud": "<value from GET /api/v1/integration/public-context>",
  "iat": 1713000000,
  "exp": 1713000480
}
```

JWT header must include `"kid": "<key_id>"` matching a non-revoked key registered with the Meticulous application.

### Key requirements

- Algorithm: **RS256** (RSA-2048 minimum, RSA-4096 recommended).
- Lifetime: **8 minutes maximum** (API default `app_max_ttl_secs` is 600s).
- Clock skew tolerance: **60 seconds** — keep node clocks in sync (NTP).

### Kubernetes Secret (private key)

```yaml
apiVersion: v1
kind: Secret
metadata:
  name: meticulous-app-key
  namespace: meticulous
type: Opaque
stringData:
  private_key.pem: |
    -----BEGIN PRIVATE KEY-----
    <your RSA private key — generate with openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:4096>
    -----END PRIVATE KEY-----
```

!!! danger "Private key security"
    Never commit the private key to source control. Use Kubernetes Secrets, Vault, or a secrets manager. Rotate keys if they are ever exposed.

See [Meticulous Apps & JWTs](../operator/apps-jwt.md) for full operator configuration.

---

## Example: triggering a run with curl

```bash
# 1. Create or retrieve your API token
TOKEN="met_01HXYZ_aBcDeFgH..."

# 2. Find your pipeline ID
curl -s -H "Authorization: Bearer $TOKEN" \
  https://ci.example.com/api/v1/pipelines \
  | jq '.data[] | {id, name}'

# 3. Trigger a run
curl -s -X POST \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"ref": "main", "message": "manual trigger"}' \
  https://ci.example.com/api/v1/pipelines/01HABC.../trigger
```
