# Architecture Decision Records (ADRs)

ADRs capture **one decision** each: context, the choice, consequences, and (for security-sensitive areas) a short **threat model**. They turn PRDs into implementable contracts (API shapes, migrations, protobuf, NATS, TLS).

## When to write an ADR

- The decision affects multiple crates or teams, or is hard to reverse (schema, wire format, subject taxonomy).
- A PRD says “TBD” on something that must be fixed before coding.

## Status

| Status | Meaning |
| --- | --- |
| **Proposed** | Under review; do not treat as frozen. |
| **Accepted** | Team agrees; implement and link from [skills/meticulous-system-architecture/SKILL.md](../../skills/meticulous-system-architecture/SKILL.md). |
| **Deprecated** | Superseded; link to replacement ADR. |

## Index

| ADR | Title | Status | PRDs |
| --- | --- | --- | --- |
| [001-run-and-job-lifecycle.md](001-run-and-job-lifecycle.md) | Run and job lifecycle in Postgres | Proposed | 030, 040, 050 |
| [002-nats-subjects-and-envelopes.md](002-nats-subjects-and-envelopes.md) | NATS subjects and job envelopes | Proposed | 040, 110 |
| [003-grpc-agent-control-plane.md](003-grpc-agent-control-plane.md) | gRPC agent control plane | Proposed | 050, 060, 110 |
| [004-secrets-and-per-job-pki.md](004-secrets-and-per-job-pki.md) | Secrets delivery and per-job PKI | Proposed | 060 |
| [005-scm-webhook-security.md](005-scm-webhook-security.md) | Inbound SCM webhook verification | Proposed | 020 |
| [006-execution-telemetry-schema.md](006-execution-telemetry-schema.md) | Executed binaries and network telemetry schema | Proposed | 070, 090 |
| [007-observability-opentelemetry.md](007-observability-opentelemetry.md) | OTel metric names and trace propagation | Proposed | 080 |
| [008-tenancy-rbac-api-tokens.md](008-tenancy-rbac-api-tokens.md) | RBAC scopes and API tokens | Proposed | 010 |
| [009-pipeline-linter-architecture.md](009-pipeline-linter-architecture.md) | `met lint` rule engine vs `met suggest` AI layer | Proposed | 030, 130 |
| [010-project-and-scm-data-model.md](010-project-and-scm-data-model.md) | Project membership, secret scope hierarchy, SCM repo attachment | Proposed | 010, 020, 060 |
| [011-remote-cache-key-derivation.md](011-remote-cache-key-derivation.md) | Remote cache key derivation and tenant isolation | Proposed | 030, 050 |
| [012-custom-execution-engine.md](012-custom-execution-engine.md) | Custom execution engine (Tekton rejection and criteria) | Accepted | 030, 040, 110 |
| [013-project-webhook-multi-pipeline-routing.md](013-project-webhook-multi-pipeline-routing.md) | Project-level SCM webhooks → one or many pipelines | Proposed | 020, 010 |
| [014-workspace-snapshots.md](014-workspace-snapshots.md) | Workspace snapshots and soft affinity | Proposed | 030, 040 |
| [015-oci-environment-images.md](015-oci-environment-images.md) | OCI environment images, registry credentials, and integrity chain | Proposed | 030, 060 |
| [016-pipeline-environments.md](016-pipeline-environments.md) | Pipeline environments and approval gates | Proposed | 060, 010 |
| [017-oidc-workload-identity.md](017-oidc-workload-identity.md) | OIDC workload identity provider | Proposed | 060, 110 |
| [018-local-execution.md](018-local-execution.md) | Local execution security model | Proposed | 030 |
| [019-remote-pipeline-validation.md](019-remote-pipeline-validation.md) | Remote pipeline validation and diagnostic codes | Proposed | 030, 130 |
| [020-external-secret-providers.md](020-external-secret-providers.md) | External secret providers and dual-mode resolution | Proposed | 060 |
| [021-resource-visibility-pipeline-acl.md](021-resource-visibility-pipeline-acl.md) | Resource visibility, pipeline ACL, and admin role split | Proposed | 010, 060 |

Further ADRs to write: Bitbucket signature verification implementation, encrypted-at-rest webhook secrets (column-level KMS), pre-populated build-tool volume strategy (from random-thoughts.md).

## Template

Copy [000-template.md](000-template.md) when adding `NNN-title.md`.
