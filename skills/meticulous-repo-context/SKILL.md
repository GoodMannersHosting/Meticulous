---
name: meticulous-repo-context
description: Use when onboarding to the Meticulous repository, judging stability or production readiness, or explaining what the product is and what it optimizes for.
---

# Meticulous repo context

## Overview

Meticulous is a **CI and release platform** that prioritizes **security and trust boundaries** over raw performance. The codebase implements **outbound-only agents**, a **gRPC controller**, an **HTTP API**, a **SvelteKit web UI**, **PostgreSQL**, and supporting **NATS** and **S3-compatible storage** in typical setups.

## Stability and maturity

Treat the project as **unstable and incomplete** unless proven otherwise for a given release:

- Expect **breaking changes** to APIs, protobuf contracts, migrations, configuration, auth, and deployment layouts.
- **Production use** requires independent review, hardening, and operational discipline; in-repo docs do not certify production readiness.

Canonical wording lives in [.github/readme.md](../../.github/readme.md).

## Related skills

Load these from `skills/` when the task matches:

| Skill | When |
| --- | --- |
| `meticulous-rust-workspace` | Crate layout, builds, migrations path, frontend scripts |
| `meticulous-system-architecture` | Control plane, agents, NATS, domain model, design decisions, plan index |
| `meticulous-agent-security-invariants` | Join tokens, agent revocation, per-job secrets, provisioning, platforms |

## Skill location convention

Repo-local skills live under **`skills/<skill-name>/SKILL.md`** (this directory’s siblings). Prefer loading the narrowest skill that fits the task.

## Keywords

Meticulous, CI/CD, supply chain, breaking changes, agents, controller, security-first, PostgreSQL, NATS.
