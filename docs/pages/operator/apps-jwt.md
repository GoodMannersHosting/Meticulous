# Meticulous Apps & JWTs

**Meticulous Apps** are first-class integration identities. Unlike API tokens (which are long-lived secrets), Apps use asymmetric cryptography: you hold a private key; Meticulous holds the corresponding public key. Short-lived JWTs are minted on demand — the secret never travels over the wire.

---

## Concepts

| Term | Description |
|------|-------------|
| **Application** | A registered integration identity in Meticulous (admin UI → Apps). Has a name and one or more public keys. |
| **Installation** | A grant of the application on a specific project. Defines what permissions the app has in that project context. |
| **Key** | An RSA public key registered against an application, identified by a `kid`. Multiple keys can be active (for rotation). |
| **App JWT** | A short-lived RS256 JWT minted by your system (operator, script, etc.) using the registered private key. |

---

## Creating an application

1. Navigate to **Admin → Apps → New Application**.
2. Give the app a descriptive name (e.g. `k8s-operator-prod`).
3. Generate an RSA key pair (see below) and register the **public key** with the app, setting a `kid`.
4. **Install** the application on the target project, granting the necessary permissions.

### Key generation

```bash
# Generate RSA-4096 private key
openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:4096 -out private_key.pem

# Extract the public key for registration in Meticulous
openssl rsa -in private_key.pem -pubout -out public_key.pem
```

Register `public_key.pem` in the admin UI. Store `private_key.pem` in a Kubernetes Secret or your secrets manager — **never commit it to source control**.

---

## Minting an App JWT

Your system (e.g. the Kubernetes operator) mints a JWT on demand:

### Claims

| Claim | Value |
|-------|-------|
| `iss` | Application ID (from Meticulous) |
| `sub` | Installation ID (from Meticulous) |
| `aud` | Value from `GET /api/v1/integration/public-context` |
| `iat` | Current Unix timestamp |
| `exp` | `iat + 480` (8 minutes; must be ≤ `app_max_ttl_secs` of 600s) |

### Header

```json
{
  "alg": "RS256",
  "kid": "<key_id matching a non-revoked key on the application>"
}
```

### Retrieving the audience

```bash
curl https://ci.example.com/api/v1/integration/public-context
```

Response:

```json
{
  "audience": "meticulous-api.JOBS",
  "api_version": "1.0"
}
```

The audience is derived from the NATS JetStream stream name configured at deployment. Fetch it fresh per deployment rather than hardcoding.

### Example (Python)

```python
import time
import jwt  # PyJWT
from cryptography.hazmat.primitives.serialization import load_pem_private_key

with open("private_key.pem", "rb") as f:
    private_key = load_pem_private_key(f.read(), password=None)

now = int(time.time())
token = jwt.encode(
    {
        "iss": "01HAPP...",       # application ID
        "sub": "01HINST...",      # installation ID
        "aud": "meticulous-api.JOBS",
        "iat": now,
        "exp": now + 480,
    },
    private_key,
    algorithm="RS256",
    headers={"kid": "my-key-2026-04"},
)
```

---

## Using the App JWT

Pass the minted JWT as a Bearer token to API endpoints that accept app authentication:

```bash
curl -X POST https://ci.example.com/api/v1/integration/join-tokens \
  -H "Authorization: Bearer $APP_JWT" \
  -H "Content-Type: application/json" \
  -d '{
    "project_id": "01HPROJ...",
    "pool_tags": {"amd64": true},
    "ttl_secs": 604800
  }'
```

---

## Key rotation

1. Generate a new key pair.
2. Register the new public key in the admin UI with a new `kid`.
3. Update your system to use the new private key and `kid`.
4. Verify the new key is working (mint a JWT, make a test call).
5. Revoke the old key in the admin UI.

Both keys are valid during the transition window, enabling zero-downtime rotation.

---

## Clock skew

The API validates `iat` and `exp` with a **60-second leeway** (`app_leeway_secs`). Ensure cluster nodes are synchronized via NTP. A skew of more than 60 seconds will cause JWT validation to fail with `401 Unauthorized`.
