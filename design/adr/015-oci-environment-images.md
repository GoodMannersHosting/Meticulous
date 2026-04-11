# ADR-015: OCI environment images, registry credentials, and integrity chain

**Status:** Proposed  
**Date:** 2026-04-11  
**PRDs:** [030](../prd/030-pipeline-authoring-dag-workflows.md), [060](../prd/060-secrets-providers-and-per-job-pki.md)

## Context

Jobs currently run directly on the agent host. There is no mechanism to specify a container image as the execution environment, authenticate to private registries, or verify image integrity. This limits reproducibility and forces operators to pre-configure agent hosts with all required toolchains.

The plan introduces two coupled features: a new **registry secret kind** for authenticated image pulls, and an **`environment:` block** on jobs for specifying digest-pinned, signature-verified OCI containers.

## Decision

### Part 1: Registry secret kind

#### Stored secret kind

Add `Registry` to the `StoredSecretKind` enum in `crates/met-store/src/repos/builtin_secrets.rs`. The DB CHECK constraint on `builtin_secrets.kind` gains `'registry'` (migration `042_registry_secret_kind.sql`).

#### Secret materialization

In `crates/met-secret-resolve/src/resolve.rs`, registry secrets materialize as `SECRET_MATERIAL_KIND_REGISTRY_AUTH = 3` (new enum value in `agent.proto`). Resolution-time behavior by `registry_type`:

| `registry_type` | Resolution |
| --- | --- |
| `password` | Pass-through: username + password delivered as-is |
| `bearer_token` | Pass-through: token delivered as-is |
| `cloud_auth` | Exchange for short-lived credential (ECR `GetAuthorizationToken`, GCR oauth2 token, ACR token exchange) following the existing `github_app` exchange pattern |

#### Metadata schema

```json
{
  "registry_type": "password",
  "target_urls": ["harbor.cloud.example.com/*"],
  "username_hint": "deploy-bot"
}
```

- `target_urls`: array of URL patterns. Wildcards (`*`) permitted within a single DNS label or path segment only. At least one literal DNS label is required (no `*/*` or `*`).
- API validation: on save, perform a HEAD request to `https://<target_url>/v2/` (with credential) to confirm the credential is valid. Failures are warnings, not hard errors (the registry may be temporarily unreachable).

#### Proto

Add to `SecretMaterialKind` in `agent.proto`:

```protobuf
SECRET_MATERIAL_KIND_REGISTRY_AUTH = 3;
```

The materialized payload for `REGISTRY_AUTH` contains JSON with `username`, `password` (or `token`), and `server_address`. The agent stores this in a job-scoped in-memory map — never written to disk, never injected as an environment variable.

### Part 2: OCI environment images

#### YAML surface

```yaml
jobs:
  build:
    environment:
      image: ghcr.io/acme/build-env@sha256:abc123...
      verify: cosign
      credentials: stored:acme-ghcr
      pull_policy: if-not-present   # always | if-not-present | never
    steps:
      - run: cargo build --release
```

#### Parser changes

In `crates/met-parser/src/schema.rs`, add:

```rust
pub struct RawEnvironment {
    pub image: String,
    pub verify: Option<String>,        // "cosign" | "none" (default: "none")
    pub credentials: Option<RawSecretRef>,
    pub pull_policy: Option<String>,   // "always" | "if-not-present" | "never"
}
```

Add `environment: Option<RawEnvironment>` to `RawJob`. In `crates/met-parser/src/parser.rs`, propagate into `JobIR` during workflow expansion. Validation rules:

- If `credentials` is present, it must be a `stored:` reference (not inline).
- If `verify` is `"cosign"`, image must include a digest (`@sha256:`).

#### Proto changes

Add to `controller.proto`:

```protobuf
message EnvironmentSpec {
    string image = 1;
    string expected_digest = 2;
    string registry_credential_name = 3;
    string verify_method = 4;          // "cosign" or empty
    string verify_key_ref = 5;         // public key reference for cosign
    string pull_policy = 6;
}
```

Add `EnvironmentSpec environment = 27` to `JobDispatch`.

#### Pre-flight validation (engine side)

In `crates/met-engine/src/executor.rs`, before dispatching a job with an environment:

1. Resolve the registry credential (if declared) and validate that `target_urls` match the image host.
2. HEAD request to the registry to confirm the image manifest exists and the digest matches.
3. Fail the run before dispatch on 401 (credential invalid) or 404 (image not found).

#### Agent-side execution

In `crates/met-agent/src/executor.rs`:

1. If `EnvironmentSpec` is present and `REGISTRY_AUTH` material was delivered: store credential in job-scoped map.
2. Pull the image using `crates/met-agent/src/backend/container.rs`:
   - Write an ephemeral `config.json` to a temporary directory with the registry credential.
   - Set `DOCKER_CONFIG` to the temp directory on the pull subprocess.
   - After pull completes: zeroize and delete the ephemeral config.
3. Verify the pulled image digest via `docker inspect --format='{{.RepoDigests}}'`. Compare against `expected_digest`. Abort on mismatch.
4. If `verify_method == "cosign"`: run `cosign verify --key <key_ref> <image>` and fail if verification fails.
5. Execute steps inside the container with the workspace mounted at `/workspace`.

#### Lint rules

Add to the `met lint` rule set (ADR-009):

| Rule | Severity | Description |
| --- | --- | --- |
| `SC-004` | Error | Environment image not pinned to digest (`@sha256:`) |
| `SC-005` | Error | Registry credential `target_urls` pattern too broad |
| `SC-006` | Warning | No signature verification configured (`verify` absent or `"none"`) |

## Consequences

### Positive

- Jobs run in reproducible, isolated environments regardless of agent host configuration.
- Digest pinning and cosign verification provide supply-chain integrity for execution environments.
- Registry credentials never touch disk on the agent (in-memory only, zeroized after use).

### Negative

- Image pull latency added to job startup (mitigated by `if-not-present` pull policy and image caching on agents).
- Requires Docker or a compatible container runtime on agents that use OCI environments.
- `cosign` must be installed on agents that verify signatures (or bundled with the agent binary).

### Migration

- New DB migration (`042`) extends the CHECK constraint; backward-compatible.
- New proto fields on `JobDispatch`; older agents ignore unknown fields.
- Existing jobs without `environment:` are unaffected.

## Threat model

- **Assets:** Registry credentials (passwords, tokens, cloud auth); container images (execution environment integrity); workspace files exposed to the container.
- **Adversaries:** Compromised registry serving a tampered image; compromised agent extracting registry credentials; man-in-the-middle on image pull.
- **Mitigations:**
  - Digest pinning ensures bit-for-bit image integrity (even if a tag is re-pushed).
  - Cosign verification provides cryptographic proof of publisher identity.
  - Registry credentials are in-memory only, zeroized after pull, never written to disk or env vars.
  - Ephemeral `DOCKER_CONFIG` directory is deleted immediately after pull.
  - Pre-flight HEAD request catches missing/unauthorized images before wasting agent time.
  - `target_urls` pattern validation prevents a single credential from being used against arbitrary registries.
- **Residual risk:** A compromised agent can observe registry credentials in memory during the pull window. Mitigation: per-job credential rotation via `cloud_auth` type (short-lived tokens). Image signature verification depends on `cosign` binary integrity on the agent.

**Certificates:** Registry TLS certificates are verified by the container runtime's default trust store. If custom CA bundles are used, operators must ensure they meet workspace certificate verification requirements (expiry, key strength, signature algorithm).

## References

- [ADR-004](004-secrets-and-per-job-pki.md) — per-job PKI; registry credentials use the same encrypted delivery channel
- [ADR-009](009-pipeline-linter-architecture.md) — lint rule architecture for SC-004/005/006
- [ADR-012](012-custom-execution-engine.md) — custom engine; OCI environments extend agent execution model
- [`crates/met-store/src/repos/builtin_secrets.rs`](../../crates/met-store/src/repos/builtin_secrets.rs) — stored secret kinds
- [`crates/met-secret-resolve/src/resolve.rs`](../../crates/met-secret-resolve/src/resolve.rs) — secret materialization
- [`crates/met-agent/src/backend/container.rs`](../../crates/met-agent/src/backend/container.rs) — container backend
- [`proto/meticulous/agent/v1/agent.proto`](../../proto/meticulous/agent/v1/agent.proto) — `SecretMaterialKind`
- [`proto/meticulous/controller/v1/controller.proto`](../../proto/meticulous/controller/v1/controller.proto) — `JobDispatch`
- [Platform evolution plan](../../.cursor/plans/platform_evolution_plan_a66c44a0.plan.md) — Features 1–2
