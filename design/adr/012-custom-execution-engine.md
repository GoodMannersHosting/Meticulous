# ADR-012: Custom execution engine (Tekton rejection and criteria)

**Status:** Accepted
**Date:** 2026-03-31
**PRDs:** [030](../prd/030-pipeline-authoring-dag-workflows.md), [040](../prd/040-scheduling-and-nats-dispatch.md), [110](../prd/110-kubernetes-operator-and-agent-fleet.md)

## Context

Early architecture evaluation considered **Tekton Pipelines** as the execution engine rather than building a custom DAG runner. This decision was discussed informally but never formally recorded. As the design has matured (custom NATS dispatch, per-job PKI, egress-only agents, supply-chain telemetry), the implicit rejection of Tekton needs to be explicit so future contributors do not re-evaluate without context.

## Options considered

### Option A: Tekton Pipelines (Kubernetes-native)

Tekton provides CRD-driven `Pipeline`, `Task`, and `TaskRun` resources with Kubernetes-native scheduling, workspace volumes, and a mature operator community.

**Pros:**
- Kubernetes-native; well-understood by platform teams.
- Rich plugin ecosystem (Tekton Catalog, Chains for SLSA attestations).
- Proven at scale inside large orgs.

**Cons:**
- **Kubernetes-only.** macOS, Windows, and bare-metal agents are first-class requirements (PRD-110). Tekton has no native concept of non-Kubernetes agents.
- **NATS-hostile.** Tekton uses Kubernetes events and reconciler loops, not NATS JetStream. Bolting NATS dispatch onto Tekton requires a shim that defeats Tekton's scheduling model.
- **Secret delivery model incompatible.** Per-job PKI (ADR-004) requires the agent to generate an ephemeral X25519 keypair and perform `ExchangeJobKeys` over gRPC before job start. Tekton's secret injection model uses Kubernetes `Secret` objects mounted as volumes — plaintext secrets are visible to the node, violating the no-plaintext-secrets-on-node invariant.
- **Egress-only agent model impossible.** Tekton pods receive work via Kubernetes scheduler; agents cannot initiate outbound connections to a control plane and pull work. An egress-only Tekton agent would require a substantial custom controller.
- **Supply-chain telemetry coupling.** Meticulous's binary execution tracking (ADR-006), custom `ExecutionMetadata` schema, and seccomp-notif collection are incompatible with Tekton's TaskRun status model without significant upstream patches.
- **Fork PR trust model.** The three-tier fork trust model (ADR-005) and its per-run secret policy cannot be expressed as Tekton pipeline parameters without custom admission webhooks.

### Option B: Custom DAG engine + NATS dispatch (selected)

Build a minimal DAG engine in Rust (resolves dependencies, emits `JobDispatch` messages to NATS), with agents as pure consumers of NATS subjects.

**Pros:**
- Egress-only agent model is a first-class design constraint, not a bolt-on.
- Per-job PKI, fork trust, secret delivery, and binary telemetry designed in from the start.
- No Kubernetes dependency for the data plane; macOS, Windows, and bare-metal agents are treated equally.
- NATS JetStream provides durable dispatch, replay, and DLQ without reconciler loops.

**Cons:**
- Engineering investment: no off-the-shelf DAG engine; must build and maintain.
- Less battle-tested than Tekton at very large scale initially.

## Decision

**Select Option B.** The custom DAG engine with NATS dispatch is the only option compatible with the platform's non-negotiable constraints:

1. Egress-only agent model (security invariant — see [security.md](../security.md))
2. Cross-platform agent support (Linux, macOS, Windows — see [agents.md](../agents.md))
3. Per-job PKI secret delivery (no plaintext on node — ADR-004)
4. Supply-chain telemetry schema ownership (ADR-006)

Tekton is not rejected because it is a bad product; it is rejected because its Kubernetes-native, in-cluster model is structurally incompatible with these constraints.

## Criteria for revisiting this decision

This ADR should be reopened if **all** of the following become true:

1. Tekton (or a Tekton fork) ships native support for egress-only, non-Kubernetes agents over a NATS or gRPC pull model.
2. The per-job PKI secret delivery model can be implemented without Kubernetes `Secret` volume mounts (e.g., via a Tekton custom secret resolver that calls `ExchangeJobKeys`).
3. Platform requirements change so that Linux/Kubernetes is the only required execution environment (macOS and Windows support dropped).

Short of these criteria, Tekton-based alternatives should be declined in design review.

## Consequences

- The DAG engine lives in `crates/met-engine` (or equivalent). PRD-030 owns its YAML surface.
- Scheduling and NATS dispatch is owned by `crates/met-scheduler` per ADR-002.
- No Tekton CRDs, controllers, or Catalog tasks are introduced in the codebase.
- If SLSA Tekton Chains attestation is desired, the supply-chain attestation path is built natively via `met-attestation` using the same SLSA provenance schema, not via Tekton Chains.

## References

- [ADR-002](002-nats-subjects-and-envelopes.md) — NATS dispatch model
- [ADR-004](004-secrets-and-per-job-pki.md) — per-job PKI (incompatible with Tekton Secrets model)
- [ADR-005](005-scm-webhook-security.md) — fork trust (would require custom Tekton admission)
- [ADR-006](006-execution-telemetry-schema.md) — binary telemetry schema
- [design/agents.md](../agents.md) — cross-platform agent requirements
- [design/open-questions.md](../open-questions.md) — original custom engine question (resolved)
