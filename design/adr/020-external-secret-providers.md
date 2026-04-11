# ADR-020: External secret providers and dual-mode resolution

**Status:** Proposed  
**Date:** 2026-04-11  
**PRDs:** [060](../prd/060-secrets-providers-and-per-job-pki.md)

## Context

Meticulous currently supports two secret sources: built-in (encrypted at rest in Postgres) and a small set of external providers (Vault/OpenBao, AWS Secrets Manager, GCP Secret Manager, Kubernetes) that are resolved server-side by the control plane. This model has three limitations:

1. **No ambient identity resolution.** Cloud-native environments (EKS with IRSA, GKE with Workload Identity, Azure with Managed Identity) provide automatic credentials to workloads. Today the control plane must hold long-lived provider credentials even when the agent already has the right identity to resolve secrets directly.
2. **Limited provider coverage.** Bitwarden, 1Password, Akeyless, and Conjur are commonly requested but unsupported.
3. **No provider configuration management.** External provider credentials are configured per-secret reference in code or via environment variables. There is no structured way to manage, rotate, or test provider connections.

The OIDC identity provider (ADR-017) enables a third authentication path: the agent can use a Meticulous-minted OIDC token for Vault JWT auth, AWS IRSA-via-OIDC, and similar federation patterns, making agent-side resolution viable for providers that accept OIDC tokens.

## Decision

### Provider configuration table

#### Migration `045_secret_provider_configs.sql`

```sql
CREATE TYPE secret_provider_type AS ENUM (
    'aws_sm', 'vault', 'gcp_sm', 'azure_kv',
    'kubernetes', 'bitwarden', 'onepassword',
    'akeyless', 'conjur'
);

CREATE TABLE secret_provider_configs (
    id                uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id            uuid NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    project_id        uuid REFERENCES projects(id) ON DELETE CASCADE,
    name              text NOT NULL CHECK (name ~ '^[a-z0-9][a-z0-9_-]{0,62}$'),
    provider_type     secret_provider_type NOT NULL,
    config_encrypted  bytea NOT NULL,
    resolution_mode   text NOT NULL DEFAULT 'remote'
                      CHECK (resolution_mode IN ('local', 'remote', 'auto')),
    enabled           boolean NOT NULL DEFAULT true,
    last_tested_at    timestamptz,
    last_test_ok      boolean,
    created_at        timestamptz NOT NULL DEFAULT now(),
    updated_at        timestamptz NOT NULL DEFAULT now(),
    UNIQUE (org_id, COALESCE(project_id, '00000000-0000-0000-0000-000000000000'), name)
);
```

- `config_encrypted`: Provider connection details encrypted with the platform master key (same mechanism as `builtin_secrets`). Contents vary by provider type (see per-provider schemas below).
- `resolution_mode`: `local` (agent resolves using ambient identity), `remote` (control plane resolves and delivers via per-job PKI), or `auto` (control plane decides based on whether `config_encrypted` contains credential material).
- Scope: `project_id IS NULL` = org-wide config; `project_id IS NOT NULL` = project-scoped config. Project-scoped configs shadow org-scoped configs with the same name.

### Dual-mode resolution

Every external secret reference is resolved through one of two paths:

```
Pipeline YAML                    Control plane                  Agent
─────────────                    ─────────────                  ─────
secrets:                         ┌───────────────────┐
  DB_PASSWORD:                   │ Look up provider  │
    provider: vault              │ config by name     │
    path: secret/db/prod    ──►  │                   │
    key: password                │ resolution_mode?  │
    resolution: local            └────┬──────────┬───┘
                                      │          │
                               ┌──────▼──┐  ┌───▼─────────┐
                               │ REMOTE  │  │ LOCAL       │
                               │         │  │             │
                               │ Resolve │  │ Build       │
                               │ secret  │  │ ExternalRef │
                               │ value   │  │ proto msg   │
                               │         │  │             │
                               │ Encrypt │  │ Attach to   │
                               │ via PKI │  │ JobDispatch │
                               │ (ADR-004)│ │             │
                               └────┬─────┘ └───┬─────────┘
                                    │            │
                                    ▼            ▼
                               Agent decrypts   Agent resolves
                               secret value     using ambient
                               (existing flow)  identity + ref
```

#### Remote resolution (default)

The existing path: the control plane uses credentials from `config_encrypted` to fetch the secret value, then delivers it encrypted to the agent via per-job PKI (ADR-004). The agent never sees provider credentials.

#### Local resolution

The control plane sends only a structured reference (`ExternalSecretRef` proto message) to the agent — never the secret value or provider credentials. The agent resolves the secret using ambient identity (IRSA, Workload Identity, pod service account, Managed Identity) or a Meticulous OIDC token (ADR-017).

#### Resolution mode selection

| `resolution_mode` in config | `config_encrypted` has credentials? | Effective mode |
| --- | --- | --- |
| `local` | No | `local` |
| `local` | Yes | **Error** at config save — credentials must be removed for local mode |
| `remote` | Yes | `remote` |
| `remote` | No | **Error** at config save — remote requires credentials |
| `auto` | Yes | `remote` (credentials present → control plane resolves) |
| `auto` | No | `local` (no credentials → agent must resolve) |

The YAML-level `resolution: local | remote` field on individual secret references can override the config-level mode, but cannot override the forced constraints (remote without credentials, or local with credentials).

### Proto changes

Add to `controller.proto`:

```protobuf
message ExternalSecretRef {
    string provider_type = 1;
    string name = 2;
    map<string, string> parameters = 3;
}
```

Add to `JobDispatch`:

```protobuf
repeated ExternalSecretRef agent_resolved_secrets = 29;
```

`parameters` contains the provider-specific reference fields (path, key, version, etc.) without any credential material.

### Agent-side resolution

New module `crates/met-agent/src/external_secrets.rs`:

1. On receiving a `JobDispatch` with `agent_resolved_secrets`, the agent instantiates the appropriate provider client for each reference.
2. Provider clients authenticate using ambient identity or OIDC:
   - **AWS:** IRSA (environment variables + web identity token file) or `met id-token --audience sts.amazonaws.com` for OIDC federation.
   - **Vault:** Kubernetes auth (`/var/run/secrets/`) or JWT auth with `met id-token --audience <vault_addr>`.
   - **GCP:** Workload Identity (GKE metadata server) or Application Default Credentials.
   - **Azure:** Managed Identity (IMDS endpoint).
   - **Kubernetes:** In-cluster service account token from `/var/run/secrets/kubernetes.io/serviceaccount/token`.
3. Resolved secret values are held in `zeroize`-aware memory (same as PKI-delivered secrets), scoped to the job, and zeroized at job completion.
4. Resolution failures are reported as job-level errors with the provider type and parameter names (never the secret value or credential details).

### Control-plane resolution changes

In `crates/met-secret-resolve/src/resolve.rs`:

- Replace the current `ExternalNotConfigured` rejection with a two-path dispatcher.
- For `remote` mode: look up `secret_provider_configs`, decrypt credentials, call the appropriate provider, return the plaintext value for PKI encryption.
- For `local` mode: build an `ExternalSecretRef` proto message from the YAML reference + config parameters, attach to `JobDispatch.agent_resolved_secrets`.

New provider broker in `crates/met-secrets/src/broker.rs`:

```rust
pub struct MultiProviderBroker {
    providers: HashMap<SecretProviderType, Box<dyn SecretProvider>>,
}

pub trait SecretProvider: Send + Sync {
    async fn resolve(&self, config: &ProviderConfig, reference: &SecretReference) -> Result<SecretValue>;
    async fn test_connection(&self, config: &ProviderConfig) -> Result<()>;
}
```

### Per-provider configuration schemas

Each provider's `config_encrypted` JSON has two sections: `connection` (endpoints, namespaces) and `auth` (credentials, if any). The `auth` section is absent for `local` mode configs.

#### AWS Secrets Manager

```json
{
    "connection": { "region": "us-east-1", "endpoint_url": null },
    "auth": {
        "method": "static | role_arn | oidc",
        "access_key_id": "...",
        "secret_access_key": "...",
        "role_arn": "arn:aws:iam::123:role/meticulous",
        "session_duration_seconds": 3600
    }
}
```

#### Vault / OpenBao

```json
{
    "connection": { "address": "https://vault.internal:8200", "namespace": "prod", "tls_ca_cert": null },
    "auth": {
        "method": "token | approle | jwt | kubernetes",
        "token": "...",
        "role_id": "...", "secret_id": "...",
        "mount_path": "auth/approle",
        "role": "meticulous-role"
    }
}
```

#### GCP Secret Manager

```json
{
    "connection": { "project_id": "my-gcp-project" },
    "auth": {
        "method": "service_account | workload_identity | oidc",
        "service_account_json": "..."
    }
}
```

#### Azure Key Vault

```json
{
    "connection": { "vault_url": "https://myvault.vault.azure.net" },
    "auth": {
        "method": "managed_identity | client_credentials | oidc",
        "tenant_id": "...", "client_id": "...", "client_secret": "..."
    }
}
```

#### Kubernetes

```json
{
    "connection": { "namespace": "default", "cluster_endpoint": null },
    "auth": {
        "method": "in_cluster | kubeconfig",
        "kubeconfig_base64": "..."
    }
}
```

#### Bitwarden

```json
{
    "connection": { "server_url": "https://bitwarden.example.com" },
    "auth": {
        "method": "api_key",
        "client_id": "...", "client_secret": "..."
    }
}
```

#### 1Password

```json
{
    "connection": { "connect_host": "https://connect.1password.example.com" },
    "auth": {
        "method": "connect_token",
        "token": "..."
    }
}
```

#### Akeyless

```json
{
    "connection": { "api_gateway_url": "https://api.akeyless.io" },
    "auth": {
        "method": "api_key | oidc | iam",
        "access_id": "...", "access_key": "...",
        "oidc_audience": "..."
    }
}
```

#### Conjur

```json
{
    "connection": { "appliance_url": "https://conjur.example.com", "account": "myaccount" },
    "auth": {
        "method": "api_key | oidc",
        "login": "host/meticulous", "api_key": "..."
    }
}
```

### YAML surface for secret references

In `crates/met-parser/src/schema.rs`, expand `RawSecretRef` to support all providers:

```rust
pub struct RawSecretRef {
    pub provider: String,
    pub resolution: Option<String>,  // "local" | "remote"
    // Provider-specific fields (parsed from YAML, validated by provider)
    pub path: Option<String>,        // Vault, Conjur
    pub key: Option<String>,         // Vault, AWS, GCP, Azure, Conjur
    pub version: Option<String>,     // AWS, GCP, Azure
    pub secret_name: Option<String>, // AWS, GCP, K8s
    pub vault_url: Option<String>,   // Azure
    pub namespace: Option<String>,   // K8s, Vault
    pub item_id: Option<String>,     // Bitwarden, 1Password
    pub vault_id: Option<String>,    // 1Password
    pub field: Option<String>,       // Bitwarden, 1Password
    pub akeyless_path: Option<String>,
}
```

### API routes

New file `crates/met-api/src/routes/provider_configs.rs`:

| Method | Path | Description | Auth |
| --- | --- | --- | --- |
| `GET` | `/projects/{id}/secret-providers` | List provider configs (project + inherited org) | Project developer+ |
| `POST` | `/projects/{id}/secret-providers` | Create project-scoped provider config | Project admin |
| `PATCH` | `/projects/{id}/secret-providers/{config_id}` | Update provider config | Project admin |
| `DELETE` | `/projects/{id}/secret-providers/{config_id}` | Delete provider config | Project admin |
| `POST` | `/projects/{id}/secret-providers/{config_id}/test` | Test connectivity | Project admin |
| `GET` | `/orgs/{org_id}/secret-providers` | List org-scoped provider configs | Org admin |
| `POST` | `/orgs/{org_id}/secret-providers` | Create org-scoped provider config | Org admin |
| `PATCH` | `/orgs/{org_id}/secret-providers/{config_id}` | Update org config | Org admin |
| `DELETE` | `/orgs/{org_id}/secret-providers/{config_id}` | Delete org config | Org admin |
| `POST` | `/orgs/{org_id}/secret-providers/{config_id}/test` | Test connectivity | Org admin |

Permission model follows ADR-021: project admin or org admin can manage provider configs. The connectivity test endpoint calls `SecretProvider::test_connection` and returns success/failure with a sanitized error message (no credential details in the response).

### Provider implementation crates

New implementations in `crates/met-secrets/src/providers/`:

| File | Provider | Dependencies |
| --- | --- | --- |
| `aws.rs` | AWS Secrets Manager | `aws-sdk-secretsmanager`, `aws-config` |
| `vault.rs` | Vault / OpenBao | `reqwest` (HTTP API client) |
| `gcp.rs` | GCP Secret Manager | `google-cloud-secretmanager` or `reqwest` + REST API |
| `azure.rs` | Azure Key Vault | `azure_security_keyvault_secrets` or `reqwest` + REST API |
| `kubernetes.rs` | Kubernetes Secrets | `kube` crate |
| `bitwarden.rs` | Bitwarden | `reqwest` (Bitwarden CLI/API) |
| `onepassword.rs` | 1Password | `reqwest` (Connect API) |
| `akeyless.rs` | Akeyless | `reqwest` (Gateway API) |
| `conjur.rs` | Conjur | `reqwest` (REST API) |

Add `Bitwarden`, `OnePassword`, `Akeyless`, `Conjur` to `ProviderType` in `crates/met-secrets/src/types.rs`.

## Consequences

### Positive

- Cloud-native deployments can use ambient identity (IRSA, Workload Identity) without storing long-lived credentials in Meticulous.
- OIDC tokens from ADR-017 enable agent-side resolution for Vault JWT auth and AWS OIDC federation without any static credentials.
- Provider configs are managed through the API with connectivity testing, reducing secret misconfiguration.
- Dual-mode design is backward-compatible: existing `remote`-only secrets continue to work unchanged.
- Ten provider types cover the most commonly requested external secret stores.

### Negative

- Each new provider is a separate integration to build, test, and maintain. SDK dependencies increase compile time and binary size.
- `local` mode requires agent-side network access to secret stores, which may conflict with network-restricted agent deployments.
- Provider config encryption adds operational dependency on the platform master key (same as existing `builtin_secrets`).
- `auto` mode behavior depends on config contents, which may be non-obvious to operators. Documentation and UI indicators are required.

### Migration notes

- One new migration (`045`). No existing tables are altered beyond the addition of a new enum type.
- Existing external secret references that use per-secret configuration (environment variables, inline config) continue to work during a transition period. The migration path is: create a `secret_provider_configs` entry, update YAML references to use `provider: <config_name>`, remove inline configuration.
- New proto field `agent_resolved_secrets` on `JobDispatch`; older agents ignore unknown fields (proto3 forward compatibility).

## Threat model

- **Assets:** Provider credentials in `config_encrypted` (grant access to external secret stores); secret values during resolution; ambient identity tokens on agents; OIDC tokens used for provider auth.
- **Adversaries:** Compromised control plane extracting provider credentials from DB; compromised agent attempting to resolve secrets for other jobs; network observer intercepting agent-to-provider traffic; supply chain attack on provider SDK dependencies.
- **Mitigations:**
  - Provider credentials encrypted at rest with the platform master key. Decrypted only in controller memory during resolution. Memory zeroized after use.
  - `local` mode sends only structured references (provider type, path, key) to the agent — never credentials or secret values from the control plane.
  - Agent-side resolution uses job-scoped identity: ambient identity is inherently scoped to the node/pod, and OIDC tokens are scoped to the specific job (ADR-017 `sub` claim).
  - Agent-resolved secrets are held in `zeroize`-aware memory and cleared at job completion.
  - `remote` mode delivers secrets via per-job PKI encryption (ADR-004), the same channel used for built-in secrets.
  - Provider config `auth` section presence/absence is validated at save time to prevent misconfigured `local` configs that accidentally ship credentials to agents.
  - Connectivity test endpoint sanitizes error messages to avoid leaking credential details or internal network topology.
  - `ExternalSecretRef` proto message contains no credential material by construction; only parameter names and values (path, key, version).
- **Residual risk:** An agent with ambient cloud identity may have broader access than needed for a specific job's secrets. Mitigation requires fine-grained IAM policies at the cloud provider level (e.g. AWS resource-based policies restricting access to specific secret ARNs). Meticulous documents recommended IAM configurations per provider but cannot enforce them. Provider SDK supply chain risks are mitigated by dependency pinning and audit (`cargo audit`).

**Certificates:** Provider connections (Vault, Akeyless, Conjur, 1Password Connect) use TLS. If providers use custom CA certificates (specified in `tls_ca_cert` config fields), operators must verify them per workspace certificate rules (`openssl x509 -text -noout -in <file>`) for expiry, key strength (RSA ≥ 2048 or P-256+), and signature algorithm (SHA-2 family). Self-signed CA certs in provider configs should only be used for internal/development environments.

## References

- [ADR-004](004-secrets-and-per-job-pki.md) — per-job PKI; `remote` mode reuses this encryption channel
- [ADR-008](008-tenancy-rbac-api-tokens.md) — RBAC; provider config management requires project/org admin
- [ADR-010](010-project-and-scm-data-model.md) — secret scope hierarchy; provider configs follow the same org/project scoping
- [ADR-017](017-oidc-workload-identity.md) — OIDC tokens enable `local` resolution with Vault JWT, AWS OIDC, etc.
- [ADR-021](021-resource-visibility-pipeline-acl.md) — permission model for API route authorization
- [`crates/met-secret-resolve/src/resolve.rs`](../../crates/met-secret-resolve/src/resolve.rs) — resolution dispatcher
- [`crates/met-secrets/src/providers/`](../../crates/met-secrets/src/providers/) — provider implementations
- [`crates/met-agent/src/`](../../crates/met-agent/src/) — agent-side external_secrets module
- [`crates/met-parser/src/schema.rs`](../../crates/met-parser/src/schema.rs) — `RawSecretRef` expansion
- [`proto/meticulous/controller/v1/controller.proto`](../../proto/meticulous/controller/v1/controller.proto) — `ExternalSecretRef`, `JobDispatch`
