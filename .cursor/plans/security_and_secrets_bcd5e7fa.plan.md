---
name: Security and Secrets
overview: "Phase 3 detailed plan for the Meticulous security layer: per-job PKI for secret delivery, OIDC/JWT authentication (platform and pipeline workload identity), secrets broker with provider abstraction (Vault, AWS SM, K8s, built-in), RBAC, audit logging, syscall/binary auditing, network metadata capture, and blast radius tracking."
todos:
  - id: security-pki
    content: "Implement per-job PKI: intermediate CA management, ephemeral keypair generation, CSR validation/signing, hybrid encryption (X25519 + AES-256-GCM), zeroize + mlock protections"
    status: completed
  - id: security-broker-trait
    content: Define SecretProvider trait and SecretsBroker orchestrator with resolution pipeline, retry, circuit breaker
    status: completed
  - id: security-provider-builtin
    content: Implement built-in Postgres-backed secret provider with AES-256-GCM envelope encryption and master key rotation
    status: completed
  - id: security-provider-vault
    content: Implement Vault/OpenBao provider (AppRole auth + JWT auth) and met-cli policy generator
    status: completed
  - id: security-provider-aws
    content: Implement AWS Secrets Manager provider (static creds + Roles Anywhere via pipeline OIDC)
    status: completed
  - id: security-provider-k8s
    content: Implement Kubernetes Secrets provider (in-cluster service account)
    status: completed
  - id: security-preflight
    content: Implement pre-flight secret validation (fail before job dispatch if secrets are unresolvable)
    status: completed
  - id: security-masking
    content: Implement secret masking filter in met-agent log pipeline and second-pass filter in met-logging
    status: completed
  - id: security-jwt
    content: Implement JWT issuance and validation for platform auth (local, OIDC federation, API tokens)
    status: completed
  - id: security-oidc-discovery
    content: Implement OIDC discovery endpoints and pipeline workload identity token generation with key rotation
    status: completed
  - id: security-join-tokens
    content: Implement agent join token creation, scoping, validation, expiry, revocation
    status: completed
  - id: security-agent-registration
    content: Implement agent registration with security bundle validation, NTP check, binary SHA check
    status: completed
  - id: security-agent-jwt
    content: Implement agent JWT issuance, renewal with approval workflow for long-lived agents, revocation
    status: completed
  - id: security-rbac
    content: Implement RBAC model (5-tier hierarchy) and authorize() middleware in met-api
    status: completed
  - id: security-audit-log
    content: Implement append-only audit logging with Postgres trigger preventing UPDATE/DELETE
    status: completed
  - id: security-syscall-audit
    content: Implement seccomp-bpf execve auditing (Linux), binary SHA computation and reporting to control plane
    status: completed
  - id: security-network-metadata
    content: Implement network metadata capture (conntrack on Linux) for per-run connection tracking
    status: completed
  - id: security-blast-radius
    content: "Implement blast radius tracking: known_binaries table, query API, binary flagging (UI + CLI + background scan)"
    status: completed
  - id: security-db-schema
    content: Write SQL migrations for all security-related tables (12 tables with indexes and triggers)
    status: completed
  - id: security-integration-tests
    content: "Write integration tests: PKI flow, each auth flow, masking, provider round-trips, blast radius queries"
    status: completed
isProject: false
---

# Security and Secrets -- Detailed Plan

Parent: [Master Architecture](master_architecture_4bf1d365.plan.md)

This plan covers **Phase 3** of the Meticulous build: the security layer that protects secrets in transit and at rest, authenticates users and pipelines via OIDC/JWT, integrates with external secret providers, audits syscall-level binary execution, and enables blast radius tracking when a tool is compromised.

**Primary Crate**: `met-secrets`
**Touches**: `met-core`, `met-agent`, `met-controller`, `met-engine`, `met-store`, `met-api`, `met-logging`

Full reference document: [design/plans/security-and-secrets.md](design/plans/security-and-secrets.md)

---

## 1. Threat Model Summary

Five threat actors drive the security design:

- **Compromised agent** -- Secrets scoped to a single job, never persisted beyond job lifetime, zeroized from memory on completion.
- **Network adversary** -- TLS on all channels, plus per-job ephemeral PKI encryption so a TLS termination compromise still yields only ciphertext.
- **Malicious pipeline author** -- Secret values injected only at runtime, never visible at parse time, masked in all log output.
- **Supply chain attack** -- Syscall auditing, binary SHA tracking, network metadata capture, and multi-layer log masking.
- **Insider threat** -- RBAC scoping, append-only audit logging, short-lived credentials.

---

## 2. Per-Job PKI

Core mechanism ensuring secrets never traverse the network in plaintext.

### Flow

1. Server publishes job to NATS (no secrets in payload)
2. Agent picks up job, generates ephemeral X.509 keypair (EC P-256 or Ed25519)
3. Agent sends CSR + job claim to Controller via gRPC
4. Controller validates (agent registered, job pending, CSR well-formed), signs CSR with platform intermediate CA
5. Secrets Broker resolves secrets, encrypts each with agent's ephemeral pubkey (X25519 + AES-256-GCM hybrid)
6. Encrypted bundle delivered to agent (each entry: key, encrypted_value, sha256_hmac)
7. Agent decrypts, validates HMACs, injects into execution environment
8. On completion: zeroize private key, delete on-disk material, report to Controller
9. Controller marks certificate as consumed (single-use)

### CA Hierarchy

```
Root CA (offline, HSM-backed)
  +-- Intermediate CA (online, managed by met-secrets)
        +-- Agent Identity Certs (long-lived, per-registration)
        +-- Job Ephemeral Certs (short-lived, per-job, max 1h)
```

### Rust Crates

- `ring` / `rustls` for key generation
- `rcgen` for CSR/cert generation, `x509-parser` for validation
- `x25519-dalek` + `aes-gcm` for hybrid encryption
- `zeroize` with `ZeroizeOnDrop` on all secret-holding structs

### Memory Protection

- `mlock()` on pages holding decrypted secrets (via `memsec` / `libc`)
- `prctl(PR_SET_DUMPABLE, 0)` on Linux before secret injection
- Platform-equivalent protections on macOS/Windows

---

## 3. OIDC / JWT Authentication

### 3.1 Platform Auth

JWT for all API authentication. Supported flows:

- **Local auth**: username/password with Argon2id, returns JWT
- **OIDC federation**: GitHub, GitLab, Google, generic OIDC provider token exchange
- **API tokens**: long-lived, configurable scopes, stored as salted SHA-256 hashes

Token claims include: `sub`, `iss`, `aud`, `org_id`, `roles`, `exp`, `iat`, `jti`

### 3.2 Pipeline Workload Identity

Each pipeline run gets a signed OIDC JWT for zero-credential access to external systems:

- **AWS Roles Anywhere**: OIDC token presented to STS, no AWS keys in Meticulous
- **Vault/OpenBao JWT auth**: Vault validates Meticulous OIDC token, returns scoped Vault token
- **GCP Workload Identity Federation**: same federated pattern
- **Container registries**: OIDC token exchanged for push/pull tokens

Claims: `sub` (pipeline:org:project:pipeline:ref:branch), `org_id`, `project_id`, `pipeline_id`, `run_id`, `job_id`, `ref`, `sha`, `trigger`, `runner_os`, `runner_arch`

### 3.3 OIDC Discovery

`met-api` serves `/.well-known/openid-configuration` and `/.well-known/jwks.json`. Separate signing keys for API auth vs pipeline OIDC. Keys stored in Postgres with `active_from`/`active_until` for rotation windows.

### Rust Crates

- `jsonwebtoken` for JWT signing/verification
- `openidconnect` for OIDC federation client
- `argon2` for password hashing

---

## 4. Secrets Broker (`met-secrets`)

Central component that resolves, encrypts, and delivers secrets to jobs.

### 4.1 Provider Trait

```rust
#[async_trait]
pub trait SecretProvider: Send + Sync {
    fn provider_type(&self) -> &str;
    async fn resolve(&self, reference: &SecretReference, context: &JobContext) -> Result<SecretValue, SecretError>;
    async fn health_check(&self) -> Result<(), SecretError>;
}
```

### 4.2 Providers

- **Vault / OpenBao** (P0) -- AppRole + JWT auth
- **AWS Secrets Manager** (P0) -- Roles Anywhere + IAM fallback
- **Kubernetes Secrets** (P0) -- in-cluster service account
- **Built-in Postgres** (P0, discouraged in UX) -- AES-256-GCM envelope encryption
- **GCP Secret Manager** (P1) -- Workload Identity Federation
- **Azure Key Vault** (P2) -- Workload Identity

### 4.3 Vault Integration

Two auth methods: AppRole (role_id + encrypted secret_id) and JWT auth (preferred -- no Vault credentials stored in Meticulous at all). CLI includes `met secrets vault generate-policy` to output required Vault policies and JWT auth role config in HCL or JSON.

### 4.4 AWS Integration

Preferred: Roles Anywhere (OIDC token to STS CreateSession). Fallback: IRSA/instance profile on EKS/EC2, or static creds with UI warning.

### 4.5 Built-in Store

AES-256-GCM with platform master key (env var, file, or K8s Secret). Per-version nonce. Master key rotation supported. UX actively discourages usage (warning banners, audit labels). Admins can disable entirely.

### 4.6 Resolution Pipeline

Parse YAML, extract refs, pre-flight check (fail fast), dispatch job, agent sends ephemeral pubkey, broker groups by provider, authenticates (OIDC where possible), fetches with retry + circuit breaker, encrypts per-secret, HMACs, delivers bundle.

### 4.7 Secret Masking

Agent-side masking filter on log capture pipeline: matches raw, base64, URL-encoded, shell-escaped variants, replaces with `*`**. Runs in agent process (not subprocess). Second-pass filter in `met-logging` on control plane as defense-in-depth.

---

## 5. Agent Join Token Security

### Scoping

- `platform` -- any job from any project
- `org:<org_id>` -- any job in the org
- `project:<project_id>` -- specific project only
- `pipeline:<pipeline_id>` -- specific pipeline only

### Token Format

`met_join_XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX` (base62). Stored as SHA-256 hash. Record includes: scope, created_by, expires_at (max 30d), max_uses, use_count, revoked.

### Registration Flow

Agent generates identity keypair + security bundle (os, arch, hostname, IPs, kernel, NTP status, binary SHA). Controller validates token, security bundle, NTP sync (mandatory), optional binary SHA match. Creates agent record, signs CSR with intermediate CA, issues JWT (24h ephemeral, 7d long-lived).

### Long-Lived Agents

JWT renewal requires admin approval (configurable auto-approve). Security bundle diff shown for review. Significant changes force manual review even with auto-approve. Failed renewals remove agent from queues.

---

## 6. Syscall Auditing and Binary Tracking

### Objective

Track every executable invoked during a pipeline run for tool inventory, anomaly detection, and blast radius input.

### Platform Strategies

- **Linux**: seccomp-bpf (`seccompiler` crate) logging `execve`/`execveat`, or `ptrace` fallback. `fanotify` for container execution interception.
- **macOS**: Endpoint Security framework, `dtrace` fallback.
- **Windows**: ETW Process creation provider, WMI fallback.

### Data Captured

Per execution: `binary_path`, `sha256`, `argv`, `timestamp`, `pid`, `ppid`, `agent_id`

### Network Metadata

Connection metadata (not content): src/dst IP:port, protocol, direction, timestamp, pid. Via `conntrack` (Linux), Network Extension (macOS), ETW (Windows).

---

## 7. Blast Radius Tracking

### Tool Database

`known_binaries` table keyed by SHA-256. Tracks first/last seen, run count, flagged status.

### Blast Radius Query

Given a compromised binary SHA and time window, returns all pipeline runs, projects, and commits that used it.

### Proactive Flagging

Admins flag binaries via UI/CLI (`met admin flag-binary --sha256 ... --reason "CVE-...")`). Future runs using flagged binary: configurable warn or block. Background job scans recent runs against newly flagged binaries and generates notifications. Cross-references SBOMs to extend blast radius from runs to artifacts and deployments.

---

## 8. RBAC Model

Five-tier hierarchy: Platform Admin > Org Admin > Project Admin > Project Member > Project Viewer.

- Stored as `(user_id, resource_type, resource_id, role)` relations
- Checked via `authorize()` middleware in `met-api`
- Group membership expanded at query time
- API tokens inherit explicitly granted scopes, not full user permissions

---

## 9. Audit Logging

Append-only `audit_log` table: actor (user/agent/system/api_token), action, resource, org/project IDs, JSONB metadata, source IP, user agent. Postgres trigger prevents UPDATE/DELETE. Production should back with S3 Object Lock for tamper-evidence.

---

## 10. Database Schema

Security-related tables added to `met-store` migrations (full DDL in [design/plans/security-and-secrets.md](design/plans/security-and-secrets.md)):

- `secret_provider_configs` -- per-project or global provider configuration
- `builtin_secrets` -- AES-256-GCM encrypted values with per-version nonce
- `agent_join_tokens` -- scoped, expirable, revocable enrollment tokens
- `agents` -- identity records with security bundles
- `job_certificates` -- per-job ephemeral cert audit trail
- `oidc_signing_keys` -- rotatable signing keys for API auth and pipeline OIDC
- `api_tokens` -- hashed, scoped, revocable API tokens
- `user_roles` -- RBAC role assignments
- `audit_log` -- append-only security event log
- `run_binary_executions` -- syscall audit records (indexed by SHA-256)
- `run_network_connections` -- network metadata records
- `known_binaries` -- tool/binary inventory with flagging

---

## 11. `met-secrets` Crate Structure

```
crates/met-secrets/src/
  lib.rs, broker.rs, masking.rs, encryption.rs, audit.rs
  provider/  -- mod.rs (trait), vault.rs, aws_sm.rs, k8s.rs, builtin.rs, gcp_sm.rs, azure_kv.rs
  pki/       -- mod.rs, ca.rs, csr.rs, ephemeral.rs
  oidc/      -- mod.rs, discovery.rs, jwks.rs, pipeline_token.rs
tests/
  pki_flow_test.rs, vault_provider_test.rs, masking_test.rs, oidc_token_test.rs
```

---

## 12. Key Dependencies

- **Crypto**: `ring`, `rcgen`, `x509-parser`, `aes-gcm`, `x25519-dalek`, `zeroize`
- **Auth**: `jsonwebtoken`, `openidconnect`, `argon2`
- **AWS**: `aws-sdk-secretsmanager`, `aws-sdk-sts`
- **HTTP**: `reqwest` (Vault API)
- **Syscall**: `seccompiler`, `nix` (mlock, prctl, ptrace)

---

## 13. Implementation Phases (Within Phase 3)

### Phase 3a -- Core PKI and Encryption

Intermediate CA management, ephemeral keypair generation (agent), CSR validation/signing (controller), hybrid encryption, zeroize + mlock protections, integration tests.

### Phase 3b -- Secrets Broker and Providers

SecretProvider trait, built-in provider, Vault/OpenBao (AppRole then JWT), AWS SM (static then Roles Anywhere), K8s Secrets, pre-flight validation, masking filter (agent + control plane).

### Phase 3c -- OIDC and Authentication

JWT issuance/validation, OIDC discovery endpoints, pipeline OIDC tokens, key rotation, OIDC federation, Argon2id passwords, API tokens, integration tests.

### Phase 3d -- Agent Security Hardening

Join tokens (create/scope/validate), agent registration with security bundle, NTP + binary SHA checks, agent JWT issuance/renewal/approval, revocation/suspension, RBAC + authorize() middleware, audit logging.

### Phase 3e -- Syscall Auditing and Blast Radius

seccomp-bpf execve auditing (Linux), binary SHA reporting, known_binaries + run_binary_executions ingestion, network metadata capture, blast radius query API, binary flagging (UI + CLI + background scan), Vault policy generator in met-cli, end-to-end tests.

---

## 14. Open Questions

1. **Ed25519 vs EC P-256** -- Ed25519 faster/simpler, P-256 has broader HSM support. Decide based on HSM timeline.
2. **Seccomp notification vs ptrace** -- SECCOMP_RET_USER_NOTIF more performant but needs Linux 5.0+. Define minimum kernel target.
3. **FUSE vs fanotify** for container binary tracking -- fanotify simpler but may miss nested container paths. Prototype both.
4. **Secret caching in broker** -- Cache for pipeline run duration? Fewer API calls vs larger attack window.
5. **HSM/KMS for intermediate CA** -- PKCS#11 or cloud KMS from day one, or add later?
6. **Audit log retention** -- Partition by month, archive to S3 with Object Lock.

