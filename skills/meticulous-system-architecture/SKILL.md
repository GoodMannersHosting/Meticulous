---
name: meticulous-system-architecture
description: Use when reasoning about control plane vs agents, NATS or gRPC boundaries, org/project/pipeline domain model, phased roadmap, or locating detailed design plans under .cursor/plans.
---

# Meticulous system architecture

## Overview

Operators use the **web UI** and **REST API** against **PostgreSQL**. **Agents** do not accept inbound control-plane connections; they **dial out** to the **controller** over **gRPC**. The controller uses **NATS** (often JetStream) for work dispatch and **S3-compatible storage** for artifacts and related binary data.

## Core ideas

1. **Pub/sub job dispatch** — Agents subscribe to NATS subjects (e.g. by pool tags). Egress-only networking for agents.
2. **Per-job PKI for secrets** — Job-scoped key material; server encrypts secrets for the agent; no plaintext secrets on the wire for that hop. Details: `meticulous-agent-security-invariants`.
3. **Custom execution engine** — In-repo engine (DAG, steps, caching, artifacts); not Tekton-centric.
4. **Reusable workflows** — Pipelines compose versioned workflows at **global** (platform) or **project** scope.
5. **External secrets preferred** — First-class Vault/OpenBao, AWS Secrets Manager, Kubernetes secrets; built-in storage discouraged by product direction.

## Domain hierarchy (summary)

Organization (tenant) → Project → Pipelines (Jobs as DAG, Steps) with scoped Secrets, Variables, Triggers; plus global or project **reusable workflows**. Pipeline YAML references workflows such as `workflow: global/...` or `workflow: project/...`.

## Deeper reference

| File                                                                                                                             | Contents                                                                                                                |
| -------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------- |
| [references/system-diagram-and-domain.md](references/system-diagram-and-domain.md)                                               | Mermaid diagram, hierarchy text, key decisions                                                                          |
| [references/phased-roadmap.md](references/phased-roadmap.md)                                                                     | Phased build order (0–7)                                                                                                |
| [references/plan-index.md](references/plan-index.md)                                                                             | Index of `.cursor/plans/*.plan.md`                                                                                      |
| [../../design/adr/README.md](../../design/adr/README.md)                                                                         | ADRs (run/job lifecycle, NATS, gRPC, secrets); check **Status** in each file—**Accepted** is normative for implementers |
| [../../design/prd/README.md](../../design/prd/README.md)                                                                         | Product requirements index, overlap resolution, verification matrix                                                     |
| [../../design/adr/013-project-webhook-multi-pipeline-routing.md](../../design/adr/013-project-webhook-multi-pipeline-routing.md) | Project webhook multi-pipeline routing                                                                                  |

Frontend stack for the web UI is **SvelteKit** and **Svelte 5** under `frontend/` (not React).

## Keywords

NATS, JetStream, gRPC, controller, met-agent, Axum, PostgreSQL, S3, DAG, reusable workflows, control plane, egress-only.
