# Security principles and threat model

This document captures the platform-level threat model, trust boundaries, and security invariants. Per-feature security is in the relevant ADR or PRD; this document covers cross-cutting concerns.

## Trust boundaries

```
[External world]
    │  SCM webhooks (HMAC-verified, replay-safe — ADR-005)
    │  Human users (OIDC/JWT — ADR-008)
    │  API tokens (hashed at rest — ADR-008)
    ▼
[Control plane — trusted]
    HTTP API / pipeline engine / scheduler / controller
    Postgres (system of record)
    NATS JetStream (broker — see caveat below)
    Object storage (S3-compatible)
    │
    │  gRPC over mTLS (ADR-003)
    │  NATS subscription with per-agent credentials (ADR-002)
    ▼
[Agents — semi-trusted after enrollment]
    Enrolled via join token + mTLS cert
    Receive only encrypted secret material (ADR-004)
    Cannot call inbound control-plane endpoints
    │
    │  Container runtime (containerd)
    ▼
[Job containers — untrusted]
    Arbitrary user code
    No direct access to secrets store
    No direct network access to control plane
    Capability-restricted (no CAP_SYS_PTRACE by default)
```

**NATS caveat:** NATS is a messaging layer, not a fully-trusted component. Secrets never appear in plaintext on NATS. Per-agent credentials limit subscription scope to the agent's own pool subjects (ADR-002).

## Threat model summary

| Threat                               | Mitigation                                                                                                      | Residual risk                                                                                               |
| ------------------------------------ | --------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| Forged webhook triggers a run        | HMAC-SHA256 signature verification; replay window (ADR-005)                                                     | GitHub webhooks without configured secret accepted in dev mode; production must have `REQUIRE_SECRET=true`  |
| Secret exfiltration via NATS         | Encrypted-only secrets on wire; per-agent NATS ACLs (ADR-002, ADR-004)                                          | Insider with NATS admin credentials can read ciphertext (not plaintext)                                     |
| Secret exfiltration via logs         | `SecretRedactor` pipeline on all log paths (PRD-050, PRD-060)                                                   | Base64-wrapped secrets must also be detected; regex coverage is defense-in-depth only                       |
| Rogue agent receiving secrets        | mTLS client cert + scoped join token; secret delivery only after `ExchangeJobKeys` over gRPC (ADR-003, ADR-004) | Compromised agent with valid JWT until revocation (30s SLA — PRD-110)                                       |
| IDOR across organizations            | Org FK on every query; `scope_type + scope_id` double-check on secrets (ADR-010)                                | Misconfigured middleware skipping auth; covered by RBAC integration tests (VERIFICATION.md)                 |
| Token theft                          | Tokens hashed at rest; short TTLs (90d API, 24h agent JWT); revocation path (ADR-008)                           | Stolen token valid until expiry unless actively revoked                                                     |
| Fork PR exfiltrating secrets         | Three-tier fork trust model (ADR-005); `fork_policy: no_secrets` default on project_repos                       | Operator misconfiguring `allow_secrets` on a fork-enabled project                                           |
| Path traversal in artifact names     | `{name}` validated as URL-safe, no `..` allowed (PRD-050)                                                       | None identified                                                                                             |
| SSRF via outbound notification URLs  | RFC 1918 / link-local denylist applied at save and dispatch time (PRD-100)                                      | Custom DNS that resolves public names to private IPs (DNS rebinding) — mitigate by re-resolving at dispatch |
| XSS via malicious log content        | ANSI escape allowlist in UI log renderer; raw HTML stripped (PRD-120)                                           | Novel escape sequences not in allowlist                                                                     |
| Memory scraping for secrets on agent | `prctl(PR_SET_DUMPABLE, 0)`, `mlock` for secret buffers; zeroize on job completion (ADR-004)                    | OS-level memory access by another process in the same PID namespace                                         |

## Security invariants (non-negotiable)

These must never be violated, regardless of performance or feature pressure:

1. **No plaintext secrets on NATS.** Enforce in code review and NATS payload inspection test (VERIFICATION.md PRD-040).
2. **No secret values in logs.** `SecretRedactor` is in the hot path; no opt-out per step.
3. **No inbound connections to agents.** All agent communication is agent-initiated (egress-only).
4. **Tokens hashed at rest.** The raw token value is shown once at creation and never stored.
5. **mTLS in production.** TLS-only only in local dev (`METICULOUS_ENV=development`).
6. **Fork PR gets no secrets by default.** `fork_policy: no_secrets` is the default; `allow_secrets` requires explicit operator action and is flagged by `met lint`.
7. **Audit log is append-only.** Postgres trigger prevents UPDATE/DELETE on `audit_log`; Object Lock Governance on archive.

## Provider credential management

External secret provider credentials (Vault AppRole, AWS OIDC role ARN, GCP SA key) are stored as project-scoped or org-scoped secrets in the platform itself (the `secrets` table with `provider = 'built_in'` for the bootstrap credential, or fetched via OIDC). They must be:

- Rotated on a schedule defined per provider (document in per-provider runbook)
- Scoped to read-only access on the specific paths/ARNs needed (least privilege)
- Audit-logged on every use (provider integration emits an OTel span per secret fetch)

**Vault/OpenBao:** Use AppRole with a short `secret_id_ttl` (1 hour) and `secret_id_num_uses: 1` per job for maximum isolation. The platform generates a new `secret_id` per job via the `met-secrets` crate. Policy generator scope (generating least-privilege Vault policies from pipeline `secrets:` declarations) is a future feature ([security.md open items](#open-items)).

## Open items

- **Vault/OpenBao AppRole policy generator** — generate least-privilege Vault policy from pipeline `secrets:` block. Needs design and ADR.
- **AWS Roles Anywhere** — for non-Kubernetes workloads needing AWS credential federation without a long-lived key. Scoped design required before implementation.
- **SOC 2 Type II readiness** — approval gate evidence (PRD-100 FR-3, deferred P2), audit log export to SIEM, penetration test scope definition.
- **mTLS certificate rotation automation** — cert-manager integration for K8s operator agent pools; manual runbook for bare-metal agents.
- **DNS rebinding mitigation for SSRF** — re-resolve notification URLs at dispatch time (not just at save time) to catch DNS rebinding. Implement in the notification worker.
