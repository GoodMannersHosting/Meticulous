---
name: meticulous-agent-security-invariants
description: Use when changing agent enrollment, join tokens, controller–agent trust, per-job secrets delivery, revocation, OIDC-style provider auth, or supported agent platforms for Meticulous.
---

# Meticulous agent security invariants

## Overview

Agents are **untrusted executors** until enrolled. **Secrets** must stay **scoped**, **revocable**, and **encrypted in transit** for the relevant hop. **Join** and **job** flows must preserve **egress-only** agent networking assumptions described in `meticulous-system-architecture`.

## Non-negotiables (summary)

- No access to secrets **outside** the active pipeline/job scope.
- Agents **revocable** from the control plane (kill/disable enrollment).
- **Join tokens** scoped (e.g. pipeline, project, or broader group semantics as implemented).
- **OIDC-style** identity patterns for **secrets providers** where applicable (JWT claims / subject), comparable in intent to GitHub Actions OIDC.
- **Per-job** cryptographic handling for secret material (see reference): generate or exchange job keys, encrypt on server, agent decrypts and verifies integrity.
- **NTP** required for sane TLS and audit timelines; **environment validation** (host, OS, network posture) is a first-class concern for join policy.
- **Operator pattern** for Kubernetes: controller/operator-style deployment similar in spirit to GitHub Actions Runner Controller.

## Product expectations

- **Dry / test** modes for developer iteration without full production side effects.
- **Live log streaming** to the control plane for debugging (exact attachment points evolve with code).
- **Long-lived agents** (e.g. Windows/macOS runners) may need **JWT lifetime** and **approval/renewal** workflows.

## Platforms

Target agents: **Linux amd64/arm64**, **macOS arm64**, **Windows amd64**. **BSD** is an explicit future possibility (likely without container execution parity).

## Provisioning flow (high level)

1. Controller issues **join token**.
2. Agent starts with token; builds **security bundle** and **X.509 public key**; sends to controller.
3. Controller validates bundle, stores pubkey with agent metadata.
4. Controller issues **JWT** (renewable or not per policy).
5. Agent connects to **job queue** / messaging fabric after success.

## PKI / secret delivery (high level)

Job published → agent accepts work → **ephemeral keypair** for the job → server validates pubkey → server resolves secrets → **encrypt per key** with agent pubkey, attach integrity digests → agent decrypts and **verifies** digests.

## Full notes

Historical bullets and numbered flows: [references/agent-security-notes.md](references/agent-security-notes.md). Design folder mirror: [design/agents.md](../../design/agents.md) (pointer only; this skill is canonical).

## Keywords

join token, JWT, revocation, per-job PKI, X.509, OIDC, secrets scope, NTP, security bundle, met-agent, met-controller, operator.
