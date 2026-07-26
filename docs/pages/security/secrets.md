# Join tokens & secrets

## Join token lifecycle

Join tokens are the bootstrap credential an agent uses to register with the controller.

### Issuance

```http
POST /api/v1/integration/join-tokens
Authorization: Bearer <api-token-or-app-jwt>

{
  "project_id": "01HXYZ...",
  "pool_tags": { "amd64": true },
  "ttl_secs": 604800
}
```

The caller must hold the `join_tokens:create` scope. The response contains the **raw token** — this is the only time it is available in plaintext. Store it securely (e.g. in a Kubernetes Secret).

### Storage

The token is hashed using Argon2id before being written to the database. The raw value is never stored. If you lose the raw token, you must issue a new one.

### Revocation

```http
DELETE /api/v1/integration/join-tokens/{token_id}
Authorization: Bearer <api-token-or-app-jwt>
```

Requires `join_tokens:revoke` scope. The Kubernetes operator automatically revokes tokens during agent teardown.

---

## Secret sources

Meticulous resolves secrets from different backends at job-start time:

=== "AWS Secrets Manager"
    ```yaml
    secrets:
      MY_SECRET:
        aws:
          arn: arn:aws:secretsmanager:us-east-1:123456789012:secret:prod/my-secret-AbCdEf
    ```
    The controller assumes an IAM role (configured via the deployment) and fetches the secret at job start. The value is encrypted to the job keypair and delivered to the agent.

=== "HashiCorp Vault / OpenBao"
    ```yaml
    secrets:
      MY_SECRET:
        vault:
          path: secret/data/my-app/my-secret
          key: value
    ```
    The controller authenticates to Vault using its own identity and fetches the secret. Vault token is never delivered to the agent.

=== "Kubernetes Secret"
    ```yaml
    secrets:
      MY_SECRET:
        kubernetes:
          namespace: production
          secret: my-k8s-secret
          key: my-key
    ```
    The controller reads the Kubernetes Secret using its ServiceAccount. The value is encrypted to the job keypair.

=== "Platform-stored (built-in)"
    ```yaml
    secrets:
      MY_SECRET:
        platform:
          name: MY_SECRET
    ```
    Built-in secrets are stored encrypted in PostgreSQL (AES-256-GCM with a master key stored outside the database). Use external stores for production sensitive values — built-in storage is convenient for low-sensitivity credentials.

---

## Secret delivery to the agent

Regardless of the source, delivery always uses per-job asymmetric encryption:

```
Backend ──plaintext──▶ Controller (in-memory only)
                            │
                            ▼ encrypt with job public key
Controller ──ciphertext──▶ Agent
                            │
                            ▼ decrypt with job private key (in-memory)
                        Step environment
                            │
                            ▼ discard key on job completion
```

!!! warning "Secret values in logs"
    Steps must not `echo` or `print` secret values. Meticulous masks known secret values in log output (`***`), but masking is best-effort — treat logs as a secondary defense, not the primary.

---

## Rotation and expiry

| Credential type | Default expiry | Notes |
|-----------------|----------------|-------|
| Human user JWT | 1 hour (with silent refresh) | Refresh via OIDC refresh token |
| API token (human) | 90 days | Rotation reminder at 80 days |
| API token (service account) | 90 days | Mandatory rotation reminder |
| Join token | 7 days | 30 days maximum with explicit opt-in |
| Agent job JWT | 24 hours (ephemeral) | Auto-issued per job; not user-managed |
| App JWT (operator) | 8 minutes | Minted per-request by the operator |
