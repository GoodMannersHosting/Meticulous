# ADR-018: Local execution security model

**Status:** Proposed  
**Date:** 2026-04-11  
**PRDs:** [030](../prd/030-pipeline-authoring-dag-workflows.md)

## Context

Developers cannot currently test pipeline YAML locally. Every iteration requires pushing to the control plane, waiting for agent dispatch, and inspecting remote logs. This slows authoring feedback loops and discourages experimentation with complex DAG structures.

The plan introduces `met run --local`: an in-process execution mode that reuses the parser and engine to run pipeline jobs locally in OCI containers. Because local execution runs on untrusted developer machines without a control plane, the security model must be fundamentally different from production: **no secrets, no network by default, no OIDC tokens, no artifact uploads**.

This ADR depends on OCI environment images (ADR-015) for container execution.

## Decision

### Command surface

```
met run --local [--file pipeline.yaml] [--parallel N] [--network] [--vars-file .met-local-vars.yaml]
```

| Flag | Default | Description |
| --- | --- | --- |
| `--file` | `meticulous.yaml` | Pipeline YAML path |
| `--parallel` | `2` | Max concurrent jobs |
| `--network` | off (`--network=none`) | Allow container network access |
| `--vars-file` | `.met-local-vars.yaml` | Non-secret variable overrides |

### Architecture

Local execution reuses existing crates in-process, with no NATS, no gRPC, no PostgreSQL:

```
met-cli (run_local command)
  ├── met-parser (PipelineParser — unchanged)
  ├── FileSystemWorkflowProvider (new — resolves workflows from disk)
  ├── met-lint (validation — unchanged)
  └── ContainerBackend (from met-agent — reused for image pull + exec)
```

#### `FileSystemWorkflowProvider`

New workflow resolution strategy in `crates/met-cli/src/`:

- Resolves `workflow: project/<name>` by scanning `.meticulous/workflows/` in the current repository.
- Resolves `workflow: global/<name>` by scanning `~/.meticulous/workflows/` (user-level) or a configured directory.
- Returns the parsed workflow YAML to the existing `PipelineParser` — no changes to the parser interface.
- If a referenced workflow cannot be found locally, the command fails with a clear error listing the search paths.

#### Execution flow

For each job in topological order (respecting `needs:` dependencies):

1. Parse and validate the pipeline YAML with `PipelineParser`.
2. Resolve all workflow references via `FileSystemWorkflowProvider`.
3. Run `met lint` validation on the resolved pipeline.
4. For each job (up to `--parallel N` concurrently):
   a. If `environment:` is specified: pull the OCI image (reusing `ContainerBackend`). Registry credentials are **not available** — only public images or locally cached images work.
   b. Run steps inside the container with the local workspace mounted at `/workspace` (read-write).
   c. Capture stdout/stderr to `.met-workspace/logs/<job>/<step>.log`.
   d. If the job declares workspace outputs: archive to `.met-workspace/snapshots/<invocation_id>/` on the local filesystem (no S3).

### Security model: what is deliberately excluded

| Capability | Production | Local | Rationale |
| --- | --- | --- | --- |
| Secret injection | Encrypted per-job PKI | All secrets resolve to empty strings | No control plane to resolve or encrypt secrets; prevents accidental credential exposure on dev machines |
| OIDC tokens | Minted by controller | Unavailable (`met id-token` returns error) | No signing key; developer machines are not trusted IdPs |
| Network access | Per-org policy | `--network=none` by default | Prevents steps from making external calls during local testing; opt-in via `--network` flag |
| Artifact upload | S3 presigned URLs | Written to `.met-workspace/` | No S3 or control plane |
| Workspace snapshots | S3 upload/download | Local filesystem | No S3 |
| Registry credentials | Encrypted REGISTRY_AUTH | Not available | No secret resolution; public images only |
| Approval gates | Pause + API approval | Skipped | No control plane or RBAC |
| Telemetry | OTel traces + metrics | None | No collector configured by default |

### Environment variables

Local mode sets two marker variables:

- `MET_LOCAL=true` — steps can branch on this for local-only behavior.
- `MET_DRY_RUN=true` — steps that check this should skip side effects (deploy, publish, notify).

These are injected into every container. Steps should not rely on their absence in production (they are simply not set).

### Variable overrides

Non-secret variables can be provided via `.met-local-vars.yaml`:

```yaml
variables:
  DEPLOY_TARGET: local
  LOG_LEVEL: debug
```

These override pipeline-level and project-level variables but do not override step-level or job-level variables. The file is `.gitignore`-recommended to prevent committing local overrides.

### Container isolation

Containers run with:

- `--network=none` by default (overridden by `--network` flag).
- `--read-only` root filesystem (workspace mount is read-write).
- No privileged mode, no host PID/network namespace.
- `--security-opt=no-new-privileges` to prevent setuid escalation.
- CPU and memory limits configurable via `--cpus` and `--memory` flags (default: no limit, matching host Docker behavior).

### What local execution validates

| Aspect | Validated | Notes |
| --- | --- | --- |
| YAML structure | Yes | Parser catches syntax and schema errors |
| DAG correctness | Yes | Dependency cycles and missing `needs:` references detected |
| Workflow resolution | Yes | Missing workflows produce clear errors |
| Lint rules | Yes | All `met lint` rules run (SC-004 etc.) |
| Shell script execution | Yes | Steps actually run in containers |
| Environment image compatibility | Yes | Image pull + step execution proves the image works |
| Output file declarations | Partial | Files written to `.met-workspace/` but not verified against schema |
| Secret availability | No | All secrets are empty; steps that require real secrets will fail |
| OIDC federation | No | No tokens minted |
| Network-dependent steps | Only with `--network` | Explicit opt-in required |

### Exit behavior

- Exit code 0 if all jobs succeed.
- Exit code 1 if any job fails, with a summary table of job statuses.
- Exit code 2 for pipeline validation errors (parse, lint, missing workflows).
- `SIGINT` (Ctrl-C) stops running containers and cleans up `.met-workspace/`.

## Consequences

### Positive

- Developers get fast feedback on pipeline structure, DAG correctness, and step execution without pushing to the control plane.
- No risk of leaking production secrets during local development.
- Reuses existing parser and container backend — minimal new code.
- Network isolation by default prevents accidental external side effects during testing.

### Negative

- Steps that depend on secrets or OIDC tokens will fail locally (by design, but requires developers to handle `MET_LOCAL` gracefully).
- Private registry images are inaccessible without manual `docker login` (registry credentials are not resolved locally).
- Local execution cannot validate approval gates, RBAC, or multi-agent scheduling.
- `.met-workspace/` directory can grow large; no automatic cleanup (developers must manage).

### Migration

- No DB migrations.
- No proto changes.
- New `run_local` module in `crates/met-cli/src/`.
- New `FileSystemWorkflowProvider` trait implementation.
- Reuses `ContainerBackend` from `crates/met-agent/src/backend/container.rs` — may require extracting it into a shared crate if `met-cli` cannot depend on `met-agent`.

## Threat model

- **Assets:** Local workspace files; developer machine resources (CPU, memory, disk); network access from containers.
- **Adversaries:** Malicious pipeline YAML (from a cloned repository or untrusted branch); malicious OCI image; container escape.
- **Mitigations:**
  - `--network=none` by default prevents data exfiltration from malicious steps.
  - `--security-opt=no-new-privileges` prevents privilege escalation inside containers.
  - Read-only root filesystem limits container write surface.
  - No secrets injected — a malicious pipeline cannot extract credentials that don't exist.
  - Workspace mount is scoped to the project directory; no host root access.
  - `met lint` runs before execution, catching known dangerous patterns.
- **Residual risk:** Container escape vulnerabilities in the Docker runtime are outside Meticulous's control. Developers running untrusted pipeline YAML should use a VM or dedicated development environment. Local mode does not provide the same isolation guarantees as production agents.

**Certificates:** Not applicable; no TLS connections established by the local execution runtime. Container images may be pulled over HTTPS using the system trust store.

## References

- [ADR-012](012-custom-execution-engine.md) — custom engine; local mode reuses the parser and DAG resolver
- [ADR-015](015-oci-environment-images.md) — OCI environments; local mode reuses `ContainerBackend`
- [ADR-009](009-pipeline-linter-architecture.md) — lint rules run during local validation
- [`crates/met-parser/src/parser.rs`](../../crates/met-parser/src/parser.rs) — pipeline parser
- [`crates/met-agent/src/backend/container.rs`](../../crates/met-agent/src/backend/container.rs) — container backend
- [Platform evolution plan](../../.cursor/plans/platform_evolution_plan_a66c44a0.plan.md) — Feature 6
