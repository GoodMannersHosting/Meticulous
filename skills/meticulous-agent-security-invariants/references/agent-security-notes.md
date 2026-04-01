# Agent security notes (canonical detail)

Consolidated from [design/agents.md](../../../design/agents.md) and the master architecture plan. Prefer updating **this file** or `SKILL.md` when agent security rules change.

## Policy bullets

- Agents should **never** have access to secrets outside their **specific pipeline scope**.
- Agents must be **revocable / killable** server-side.
- **Dev testing and iteration** should be less painful than typical hosted CI; encourage shell/Python/PowerShell and reusable test patterns.
- Support **test** or **dry** mode where productized.
- **OIDC claims and `sub`** for JWT auth to secrets providers, similar in intent to [GitHub Actions OIDC](https://docs.github.com/en/actions/concepts/security/openid-connect).
- **Live log streaming** to the control plane / agent controller for remote debugging (final wiring may move between components).
- **One-time auth tokens** to pull secrets where that pattern applies.
- **Join tokens** scoped (e.g. this pipeline, this project, or broader workflow/pipeline/group semantics).
- **Agent JWT expiration** and **approval workflow** for long-lived agents (Windows/macOS runners are often non-ephemeral).

## Operating environment validation

Validate and record posture where policy requires it, including:

- Public/private IPs, hostname, OS, physical vs virtual vs container
- Route to known IPs, patch level (details product-specific)

Administrators should be able to configure **join security checks**.

**NTP must be enabled** on agents where time skew breaks security (TLS, token validity, logs).

## Kubernetes

**Operator** for the agent controller, analogous in role to GitHub Actions Runner Controller patterns.

## Remote agents and PKI

Remote agents use **one-time** and **per-job PKI** to encrypt and decrypt secrets for that job.

## PKI process (design sketch)

1. Server: new job published to pub/sub.
2. Agent: picks up job.
3. Agent: creates one-time X.509 keypair.
4. Server: validates receipt of public key.
5. Server: determines secrets; fetches them.
6. Server: encrypts each `key=value` with agent pubkey; computes **SHA-256** per secret.
7. Server: responds with ciphertexts and digests.
8. Agent: decrypts and **validates SHA-256** per secret.

## Provisioning pipeline (numbered)

1. Controller: create join token.
2. Agent: start with join token.
3. Agent: generate security bundle and X.509 pubkey; send to controller.
4. Controller: validate bundle; store pubkey (agent metadata in DB).
5. Controller: if valid, create JWT (renewable or not).
6a. Controller: create job queue if needed.
6b. Agent: on success, receive JWT; validate; join job queue.
