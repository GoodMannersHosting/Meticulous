# ADR-017: OIDC workload identity provider

**Status:** Proposed  
**Date:** 2026-04-11  
**PRDs:** [060](../prd/060-secrets-providers-and-per-job-pki.md), [110](../prd/110-kubernetes-operator-and-agent-fleet.md)

## Context

Running jobs frequently need to authenticate to external services: cloud providers (AWS, GCP, Azure), container registries, secret stores (Vault), artifact repositories. Today this requires injecting long-lived credentials as secrets. Long-lived credentials are difficult to rotate, over-scoped, and create a broad blast radius if leaked.

The industry standard for CI/CD workload identity is OIDC federation (as implemented by GitHub Actions, GitLab CI, CircleCI). Meticulous acts as an **OIDC identity provider (IdP)**, minting short-lived JWTs for running jobs. External services configure Meticulous as a trusted IdP and map token claims to IAM roles or policies.

This ADR defines the signing key infrastructure, discovery endpoints, token claims, and minting flow. It depends on pipeline environments (ADR-016) for the `environment` claim.

## Decision

### Signing key infrastructure

#### Key algorithm

**ES256 (ECDSA with P-256 and SHA-256)**, per RFC 7518 §3.4. P-256 is the most widely supported curve for OIDC token verification across cloud providers and Vault. RSA is not used because P-256 produces smaller tokens and faster verification.

#### Key storage

New table `oidc_signing_keys` (migration `045_oidc_signing_keys.sql`):

```sql
CREATE TABLE oidc_signing_keys (
    id              uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    kid             text NOT NULL UNIQUE,
    private_key_enc bytea NOT NULL,
    public_key_jwk  jsonb NOT NULL,
    algorithm       text NOT NULL DEFAULT 'ES256' CHECK (algorithm = 'ES256'),
    created_at      timestamptz NOT NULL DEFAULT now(),
    expires_at      timestamptz NOT NULL,
    revoked_at      timestamptz,
    CONSTRAINT valid_lifetime CHECK (expires_at > created_at)
);
```

- `kid`: Key ID, a URL-safe random string (16 bytes, base64url-encoded).
- `private_key_enc`: Private key encrypted at rest with the platform master key (AES-256-GCM, same mechanism as `builtin_secrets` encryption). The private key is a raw PKCS#8 DER encoding of the P-256 private key.
- `public_key_jwk`: The corresponding public key in JWK format (included in JWKS responses).
- Key generation uses the system CSPRNG via the `ring` or `p256` crate.

#### Key rotation

- New signing key generated every **90 days** automatically (background task in the controller).
- Old keys are retained in the JWKS response until `expires_at + 24 hours` (verification overlap window), then marked revoked.
- At any time, exactly **one** key is the active signing key (the newest non-revoked, non-expired key). Tokens are always signed with the active key.
- Emergency revocation: mark a key's `revoked_at`; it is immediately removed from JWKS and the controller stops using it. In-flight tokens signed by it will fail verification at the relying party.

### Discovery endpoints

These endpoints are **public** (no authentication required), per the OIDC specification.

#### `GET /.well-known/openid-configuration`

Returns the OIDC discovery document:

```json
{
  "issuer": "https://meticulous.example.com",
  "jwks_uri": "https://meticulous.example.com/.well-known/jwks.json",
  "response_types_supported": ["id_token"],
  "subject_types_supported": ["public"],
  "id_token_signing_alg_values_supported": ["ES256"],
  "claims_supported": [
    "iss", "sub", "aud", "exp", "iat", "jti",
    "org_id", "org_slug", "project_id", "project_slug",
    "pipeline_id", "pipeline_name", "run_id", "job_run_id",
    "ref", "sha", "environment", "runner_environment"
  ]
}
```

The `issuer` value is derived from the platform's configured external URL. It MUST be an HTTPS URL. The issuer is immutable once configured; changing it invalidates all existing IdP trust relationships at relying parties.

#### `GET /.well-known/jwks.json`

Returns all non-revoked, non-expired public keys:

```json
{
  "keys": [
    {
      "kty": "EC",
      "crv": "P-256",
      "kid": "abc123...",
      "use": "sig",
      "alg": "ES256",
      "x": "...",
      "y": "..."
    }
  ]
}
```

Responses are cached with `Cache-Control: max-age=3600` to reduce load but allow key rotation to propagate within an hour.

These routes live in a new file `crates/met-api/src/routes/oidc_provider.rs`, separate from the existing OIDC consumer auth routes in `crates/met-api/src/routes/auth.rs`.

### Token claims

```json
{
  "iss": "https://meticulous.example.com",
  "sub": "org:acme:project:api:pipeline:deploy:ref:refs/heads/main:environment:production",
  "aud": "sts.amazonaws.com",
  "exp": 1712847600,
  "iat": 1712847300,
  "jti": "550e8400-e29b-41d4-a716-446655440000",
  "org_id": "...", "org_slug": "acme",
  "project_id": "...", "project_slug": "api",
  "pipeline_id": "...", "pipeline_name": "deploy",
  "run_id": "...", "job_run_id": "...",
  "ref": "refs/heads/main", "sha": "abc123def456...",
  "environment": "production",
  "runner_environment": "self-hosted"
}
```

**Key design choices:**

- `sub` is a structured string built server-side from verified run metadata. The agent cannot influence its contents. Components with no value (e.g. no environment) are omitted from the `sub` string.
- `aud` is caller-specified and required; no default audience. This prevents token reuse across services.
- `jti` is a UUID v4, unique per token, enabling revocation and replay detection at relying parties.
- `environment` is the pipeline environment name from ADR-016, or empty string if the job has no environment. This enables IAM policies like "only allow production deployments to assume this role."
- `runner_environment` is always `"self-hosted"` (Meticulous does not offer hosted runners).

### gRPC minting flow

#### Proto changes

Add to `AgentService` in `agent.proto`:

```protobuf
rpc RequestIdToken(IdTokenRequest) returns (IdTokenResponse);

message IdTokenRequest {
    string agent_id = 1;
    string job_run_id = 2;
    string audience = 3;
}

message IdTokenResponse {
    string token = 1;
    google.protobuf.Timestamp expires_at = 2;
}
```

#### Flow

1. A step calls `met id-token --audience sts.amazonaws.com` (CLI tool on the agent).
2. The agent proxies the request to the controller via the `RequestIdToken` gRPC RPC, attaching its `agent_id` and the `job_run_id` from the active job.
3. The controller validates:
   - The agent is enrolled and not revoked.
   - The `job_run_id` corresponds to a currently running job assigned to this agent.
   - The `audience` is non-empty and does not exceed 256 characters.
4. The controller assembles claims from verified run metadata (not from agent-supplied data beyond `audience`).
5. The controller signs the token with the active ES256 private key (decrypted from `oidc_signing_keys`).
6. The signed JWT is returned to the agent, which writes it to stdout (for `met id-token`) or sets `$METICULOUS_ID_TOKEN` if requested.

#### Token lifetime

- Default: **5 minutes**.
- Maximum: **15 minutes** (configurable per org, hard cap).
- Tokens are not refreshable; steps must call `met id-token` again for a new token.
- Short lifetimes limit blast radius if a token is leaked from logs or process memory.

### Audit logging

Every `RequestIdToken` call is logged to an audit table:

```sql
CREATE TABLE oidc_token_audit (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    job_run_id  uuid NOT NULL,
    agent_id    uuid NOT NULL,
    audience    text NOT NULL,
    kid         text NOT NULL,
    jti         uuid NOT NULL,
    issued_at   timestamptz NOT NULL DEFAULT now(),
    expires_at  timestamptz NOT NULL
);
```

This enables forensic investigation if a token is misused and allows operators to correlate external service access logs with Meticulous run metadata.

## Consequences

### Positive

- Eliminates long-lived cloud credentials from stored secrets for supported providers.
- Fine-grained IAM policies via structured `sub` claim (org/project/pipeline/ref/environment).
- Standard OIDC protocol; works with AWS, GCP, Azure, Vault, and any OIDC-capable service without custom integration.
- Signing key never leaves the control plane; agents never see the private key.

### Negative

- Adds cryptographic signing to the critical path of job execution (mitigated by P-256's fast signing).
- Signing key management adds operational complexity (rotation, emergency revocation).
- Relying party configuration is manual per external service (no automation in this ADR).
- Public JWKS endpoint increases the attack surface of the API server (mitigated by rate limiting and caching).

### Migration

- Three new tables (`oidc_signing_keys`, `oidc_token_audit`, plus the discovery config in application settings).
- New gRPC RPC; backward-compatible (agents that don't use OIDC never call it).
- Initial key must be generated during first startup or via an admin CLI command.

## Threat model

- **Assets:** OIDC signing private key (allows minting tokens for any job); issued tokens (grant access to external services); JWKS endpoint availability.
- **Adversaries:** Compromised agent requesting tokens for other jobs; stolen signing key; token exfiltration from logs; DDoS of JWKS endpoint.
- **Mitigations:**
  - Signing key encrypted at rest with platform master key; decrypted only in controller memory during signing.
  - `RequestIdToken` validates that the requesting agent owns the claimed job (server-side check, not trusting agent assertions beyond `audience`).
  - `sub` claim computed server-side from verified run metadata — agent cannot forge org/project/pipeline/ref/environment.
  - Short token lifetimes (5 min default, 15 min max) limit window of token misuse.
  - `aud` is required and must be explicitly specified — no wildcard audience prevents token reuse.
  - `jti` enables replay detection at relying parties.
  - JWKS endpoint is rate-limited and responses are cached.
  - Key rotation every 90 days limits exposure from a compromised key.
  - Audit table provides forensic trail for every token issued.
- **Residual risk:** A compromised controller with access to the platform master key can mint arbitrary tokens. Mitigation requires HSM-backed signing keys (deferred; noted in the plan as a future enhancement per the crypto guidelines recommending HSM/TPM for production signing keys). Log scraping on agents may capture tokens written to stdout; steps should pipe `met id-token` output directly to the consuming tool.

**Certificates:** The OIDC signing keys are ECDSA P-256 keypairs, not X.509 certificates. If the platform's external URL uses custom TLS certificates, they should be verified per workspace certificate rules. The JWKS endpoint must be served over HTTPS with a valid, non-expired certificate.

## References

- [ADR-004](004-secrets-and-per-job-pki.md) — per-job PKI; OIDC is complementary (secrets delivery vs external auth)
- [ADR-008](008-tenancy-rbac-api-tokens.md) — RBAC model; OIDC endpoints are public, but token requests require active job
- [ADR-016](016-pipeline-environments.md) — pipeline environments; `environment` claim depends on this
- [`proto/meticulous/agent/v1/agent.proto`](../../proto/meticulous/agent/v1/agent.proto) — `AgentService`, new `RequestIdToken` RPC
- [`crates/met-api/src/routes/auth.rs`](../../crates/met-api/src/routes/auth.rs) — existing OIDC consumer auth (separate from provider)
- [RFC 7518 §3.4](https://datatracker.ietf.org/doc/html/rfc7518#section-3.4) — ES256 algorithm
- [OpenID Connect Discovery 1.0](https://openid.net/specs/openid-connect-discovery-1_0.html)
- [Platform evolution plan](../../.cursor/plans/platform_evolution_plan_a66c44a0.plan.md) — Feature 5
