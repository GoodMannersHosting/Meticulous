# ADR-004: Secrets resolution and per-job PKI

**Status:** Proposed  
**Date:** 2026-03-31  
**PRDs:** [060](../prd/060-secrets-providers-and-per-job-pki.md)

## Context

Secrets must be **scoped** to the job, **encrypted** for the agent hop, and resolved from **external** stores when possible ([design/agents.md](../agents.md), agent security skill). The proto already includes `ExchangeJobKeys`, `EncryptedSecret`, `requires_secret_exchange`, and `secret_resolution_hints_json` on `JobDispatch`.

## Decision

1. **Control plane resolution:** The API/engine resolves secret **references** (Vault path, AWS ARN, etc.) **only on the server** using credentials configured per org/project. Raw values never written to Postgres in plaintext.

2. **Per-job keys:** Agent generates an **ephemeral X25519 keypair** per job (or per dispatch attempt if retry policy demands); `ExchangeJobKeys` sends the X25519 public key; server performs ECDH, derives a symmetric key via **HKDF-SHA-256** with a domain-separation label (e.g. `meticulous-pki-hybrid-encryption-v1`), encrypts each secret value with **AES-256-GCM**, and returns ciphertext + per-secret **SHA-256 integrity digests**. On agents without AES-NI hardware acceleration (e.g. some ARM64 targets), **ChaCha20-Poly1305** is an acceptable fallback; the algorithm used is communicated via the proto field `encryption_algorithm` (add if not present). Agent verifies each digest after decryption; mismatches abort the job. **Secret TTL:** encrypted material is valid for `max(job_timeout, 1h)`; agents must zeroize memory (using `zeroize`-aware types) and mark the job key consumed at job completion, not at TTL expiry. **Size limit:** maximum 512 secrets per job, each value ≤ 64 KiB; payloads exceeding this must use a provider reference, not inline delivery.

3. **NATS vs gRPC:** If `JobDispatch.secrets` is populated, values are **ciphertext only**. If `requires_secret_exchange` is true, plaintext-equivalent material flows only after successful `ExchangeJobKeys` over **gRPC** (TLS).

4. **Hints:** `secret_resolution_hints_json` carries **non-secret** routing metadata for the agent if needed; must be validated as non-sensitive in code review.

5. **Pre-run gate:** Runs do not dispatch until required secret references resolve (PRD 060 FR-2).

## Consequences

- Controller must implement or delegate to `met-secrets` (or equivalent crate) with auditable provider plugins.
- Agents need a crypto-capable dependency for keygen and decrypt.
- Rotation of server-side provider credentials is an operational procedure outside this ADR.

## Threat model

- **Assets:** Plaintext secrets in memory on server and agent during job run; provider credentials; ciphertext on NATS.
- **Adversaries:** Broker insider reading ciphertext; agent host compromise; logs exfiltrating secrets.
- **Mitigations:** AEAD, short-lived job keys, redaction (PRD 050), no plaintext on NATS, revocation (PRD 110), least-privilege provider tokens.
- **Residual risk:** Memory scraping on agent during execution; mitigations in [open-questions.md](../open-questions.md) (core dumps).

**Certificates:** If X.509 is used for job-scoped keys or mTLS, verify each cert with `openssl x509 -text -noout` for validity window, key size/curve, and signature algorithm before relying on it in production (workspace rule).

## Proto alignment notes

The following discrepancies between this ADR and the current proto files must be resolved before implementation is considered complete:

1. **`JobKeyExchange.one_time_x509_public_key` (field 3 in agent.proto)** — The field name says "X509" but this ADR specifies an **X25519** public key (raw 32-byte Curve25519 point). The field name is misleading. Rename to `one_time_x25519_public_key` in the next proto revision. The content is already correct (raw bytes); only the name is wrong.

2. **Missing `encryption_algorithm` on `EncryptedSecretValue` and `EncryptedSecret`** — Both messages lack an algorithm discriminator field. Add `string encryption_algorithm = 5` to `EncryptedSecretValue` (agent.proto) with valid values `"AES256GCM"` and `"CHACHA20POLY1305"`. Add equivalently to `EncryptedSecret` in both proto files. Default to `"AES256GCM"` for backward compatibility.

3. **`JobDispatch.trace_id` (field 16, controller.proto)** — This field carries a W3C `traceparent` string (format: `00-<trace_id>-<parent_id>-<flags>`) per ADR-007. Rename the field to `traceparent` in the next proto revision, or document in comments that the value MUST conform to the W3C Trace Context spec.

4. **`ExecutionMetadata.binary_hashes` (map<string,string>, controller.proto)** — ADR-006 specifies `path`, `sha256`, `first_executed_at`, `last_executed_at`, `step_ids` per executed binary. The current `map<string,string>` (path→hash) is missing timestamps and step association. Replace with `repeated ExecutedBinary executed_binaries` where `ExecutedBinary` has those fields.

5. **`WorkspaceConfig` missing fork context** — The `fork_pr` flag and `author_association` (TRUSTED/COLLABORATOR/EXTERNAL tier per ADR-005) are absent. Add `bool fork_pr = 5` and `string fork_trust_tier = 6` to `WorkspaceConfig` so the agent can apply restrictions without querying the control plane.

## References

- [proto/meticulous/controller/v1/controller.proto](../../proto/meticulous/controller/v1/controller.proto) `JobDispatch`, `EncryptedSecret`
- [proto/meticulous/agent/v1/agent.proto](../../proto/meticulous/agent/v1/agent.proto) `ExchangeJobKeys`, `JobKeyExchange`, `EncryptedSecretValue`
- [ADR-002](002-nats-subjects-and-envelopes.md) no-cleartext-secrets rule
