# Constraints

Non-functional requirements and boundaries that shape implementation. Security and correctness take precedence over raw throughput unless explicitly relaxed for a deployment.

## Security

- **Trust boundary** — Treat pipeline code and PR-triggered workflows as **untrusted** unless running under explicit policies (e.g. no secrets, maintainer approval). Design SCM and RBAC accordingly.
- **Secrets** — Never log or echo secret values; redact base64-wrapped secrets the same as plaintext (see [pipelines.md](pipelines.md)). Prefer external secret stores over platform-native vaulting for production.
- **Agent surface** — Agents must not expose a control-plane listener; all coordination is outbound or subscription-based.
- **Supply chain** — Product direction includes SBOM, attestations, and visibility of executed binaries; exact scope is phased (see [features.md](features.md)).

## Availability and operations

- **Control plane** — Expect Postgres, NATS JetStream, and API/controller processes to define **backup, restore, and upgrade** procedures; multi-instance API/controller should be a goal for production layouts.
- **Queues** — Job dispatch must tolerate **restarts and duplicates**; define idempotency and stale-job cleanup relative to heartbeats and lease timeouts.
- **Object storage** — Required for durable artifacts and optional log archival; define retention and lifecycle policies per tenant where needed.

## Platform and runtime

- **Kubernetes-first** — Agent pools and operator-driven runners are a primary deployment pattern; bare-metal or VM agents remain supported for macOS/Windows and specialized hardware.
- **Containers on Linux** — Default execution environment for steps; non-Linux may use native execution with clear isolation tradeoffs documented per platform.

## Compatibility and scope

- **Not a drop-in GitHub Actions clone** — Workflow syntax and actions catalog parity are non-goals; **migration aids** may map common patterns over time.
- **Language stack** — Core services in Rust (Tokio, Axum, sqlx, tonic); web UI SvelteKit/Svelte 5; protobuf for gRPC contracts.

## Explicit deferrals

Items intentionally left to product phases or [open-questions.md](open-questions.md): multi-region active/active, full billing metering, exhaustive GH Actions parity. Debug-CLI threat model is now resolved — see [prd/130](prd/130-developer-debug-cli.md).
