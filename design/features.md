# Features

Planned and in-progress capabilities grouped by theme. Implementation order follows the phased roadmap in repo skills (foundation through release management).

## Triggers and integrations

- Webhooks from SCM (GitHub, GitLab, and similar) with authenticated, replay-safe handling.
- Manual runs, tag/release triggers, and scheduled pipelines.
- Notification channels for run completion and release events: webhooks, Slack, Teams, Webex, Discord (extensible list).

## Pipeline definition and execution

- **Parsers** — YAML as the primary authoring format; TypeScript and Python parsers for workflows where programmatic generation is required.
- **DAG jobs and steps** — Dependencies, parallelism, and failure propagation rules.
- **Reusable workflows** — `global/` vs `project/` scope with versioning; inputs/outputs between composed workflows.
- **Runner selection** — Pool tags (e.g. arch, GPU, OS); alignment with agent capabilities.
- **Caching** — Multi-layer cache keys, immutability of published artifacts, and tenant-safe cache isolation (details TBD; see [open-questions.md](open-questions.md)).
- **Artifacts and logs** — Upload to object storage; live log streaming to UI; retention policies per deployment.

## Security and compliance-oriented

- Per-job PKI and encrypted secret delivery to agents.
- Integrations: Vault, OpenBao, AWS Secrets Manager, GCP Secret Manager, Kubernetes secrets.
- Pre-run validation: required secrets present before execution.
- Process/binary execution metadata and optional network flow metadata (IPs only, not payloads) for audit and blast-radius views.
- Syscall or binary auditing where platform supports it (phased).

## Observability

- OpenTelemetry metrics and traces export (Prometheus-compatible backends).
- SBOM generation and diff, tool inventory, and attestation storage where applicable to supply-chain goals.

## Release management (later phase)

- Release workflows: scheduling, communication templates (“comms generator”), coordinated notifications.
- Integration with external alerting: e.g. downtime windows and alert silencing hooks where supported.
- Rollback or promotion hooks **where applicable** (artifact/registry/model dependent; not all targets support automatic rollback).

## Operator and fleet

- Kubernetes operator for agent pools: scaling, join token provisioning patterns, CRD-driven configuration.
- Join token issuance, agent revocation, and pool health visible to operators.

## Build tool caching (future consideration)

- **Pre-populated build tool volumes:** Maintain a versioned OCI archive of common build tools (compilers, package managers, linters) mounted read-only into job containers. Layout: `/buildtools/<binary>/<version>/<arch>/<binary>`, symlinked to `/usr/local/bin/` for the active version. Reduces step startup time by eliminating per-job tool downloads. Requires a separate tool-volume provisioning pipeline and a tool version index. Not scheduled for v1; requires ADR before implementation.

## Pipeline quality

- `met lint`: deterministic rule engine for pipeline YAML; runs offline; blocking gate at dispatch. See [adr/009-pipeline-linter-architecture.md](adr/009-pipeline-linter-architecture.md).
- `met suggest`: AI-assisted pipeline suggestions; non-blocking; opt-in; separate from `met lint`.
