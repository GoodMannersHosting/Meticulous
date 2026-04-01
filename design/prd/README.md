# Product requirements (PRDs)

Use this folder for **product requirements documents**: one feature or initiative per file, written **before** major implementation when scope, success criteria, or cross-team alignment matter.

## Conventions

- **Naming:** `NNN-short-slug.md` (e.g. `010-tenancy-rbac-api-tokens.md`).
- **Start from** [TEMPLATE.md](TEMPLATE.md); delete sections that do not apply.
- **Link** to [../architecture.md](../architecture.md), [../constraints.md](../constraints.md), and [../open-questions.md](../open-questions.md) when relevant.
- **Status** in each PRD: `Draft` | `Review` | `Approved` | `Superseded` (point to replacement).

PRDs complement design notes in `design/*.md`: those files stay high-level; PRDs nail **who, what, done-when, and how we know it worked**.

**Run and release notifications** (Slack, Teams, outbound webhooks, etc.) are specified in **PRD 100** only, even though [../features.md](../features.md) mentions channels under Triggers at a high level.

Overlap glossary (webhook vs schedule, cache, telemetry): [OVERLAP-RESOLUTION.md](OVERLAP-RESOLUTION.md). Definition of done: [VERIFICATION.md](VERIFICATION.md). Architecture decisions: [../adr/README.md](../adr/README.md).

## Index (traceability)

| PRD | Purpose | Design sources |
| --- | --- | --- |
| [010-tenancy-rbac-api-tokens.md](010-tenancy-rbac-api-tokens.md) | Orgs, projects, users/groups, RBAC, API tokens, profiles | [../architecture.md](../architecture.md) domain; [../user-interface.md](../user-interface.md) (Group/User, API tokens, Profiles) |
| [020-scm-webhooks-triggers.md](020-scm-webhooks-triggers.md) | SCM webhooks, manual/tag/schedule triggers, GitHub App webhook setup | [../features.md](../features.md) Triggers; [../user-interface.md](../user-interface.md) (GitHub App) |
| [030-pipeline-authoring-dag-workflows.md](030-pipeline-authoring-dag-workflows.md) | YAML/TS/Python parsers, DAG, reusable workflows, runner tags | [../features.md](../features.md) Pipeline definition; [../pipelines.md](../pipelines.md) |
| [040-scheduling-and-nats-dispatch.md](040-scheduling-and-nats-dispatch.md) | Scheduler, NATS JetStream dispatch, durability, idempotency | [../architecture.md](../architecture.md); [../constraints.md](../constraints.md) |
| [050-agent-execution-logs-artifacts.md](050-agent-execution-logs-artifacts.md) | Agent execution, log streaming, artifacts, retention, caching hooks | [../features.md](../features.md) execution + artifacts + caching |
| [060-secrets-providers-and-per-job-pki.md](060-secrets-providers-and-per-job-pki.md) | External secret backends, per-job PKI, pre-run checks, redaction | [../features.md](../features.md) Security; [../security.md](../security.md); [../agents.md](../agents.md); [../pipelines.md](../pipelines.md) |
| [070-execution-telemetry-and-audit.md](070-execution-telemetry-and-audit.md) | Binary execution metadata, network metadata, syscall auditing | [../features.md](../features.md) Security; [../security.md](../security.md) |
| [080-observability-opentelemetry.md](080-observability-opentelemetry.md) | OTel metrics and traces, Prometheus-compatible export | [../features.md](../features.md) Observability; [../architecture.md](../architecture.md) |
| [090-supply-chain-sbom-tools-blast-radius.md](090-supply-chain-sbom-tools-blast-radius.md) | SBOM, tool DB, attestations, blast radius, flaky steps, related UI | [../features.md](../features.md); [../security.md](../security.md); [../user-interface.md](../user-interface.md) (SBOM, tool search, blast radius, flaky) |
| [100-release-management-and-notifications.md](100-release-management-and-notifications.md) | Release workflows, comms templates, notifications, hooks | [../features.md](../features.md) Release + notification channels |
| [110-kubernetes-operator-and-agent-fleet.md](110-kubernetes-operator-and-agent-fleet.md) | Operator, join tokens, JWT lifecycle, revocation, platforms, join policy | [../features.md](../features.md) Operator and fleet; [../agents.md](../agents.md); agent security skill |
| [120-web-ui-platform.md](120-web-ui-platform.md) | Run-focused UI: logs, log diff, run variables, grouping, DAG, scheduling UX | [../user-interface.md](../user-interface.md) (items not in 010/020/090) |
| [130-developer-debug-cli.md](130-developer-debug-cli.md) | Developer debug CLI: threat model, allowlist, `met lint`/`met suggest` architecture | [../open-questions.md](../open-questions.md), [../adr/009-pipeline-linter-architecture.md](../adr/009-pipeline-linter-architecture.md) |

**Not separate PRDs** (remain source docs): [../vision.md](../vision.md), [../constraints.md](../constraints.md), [../open-questions.md](../open-questions.md), [../pipelines.md](../pipelines.md) (referenced by PRDs), [../random-thoughts.md](../random-thoughts.md).
