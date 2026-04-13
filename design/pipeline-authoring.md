# Pipeline authoring guide

This document is for people writing **Meticulous pipeline YAML**: how workflows compose, how workspace snapshots behave, and when to choose **global** vs **project** reusable workflows. For protocol and implementation detail, see [ADR-014: Workspace snapshots](adr/014-workspace-snapshots.md) and [PRD 030](prd/030-pipeline-authoring-dag-workflows.md).

## Pipeline shape

A pipeline is a **DAG of workflow invocations**:

- Root keys: `name`, `triggers`, optional `vars`, `secrets`, `runs-on`, `agent-affinity`, `workflows`, etc.
- Each entry under **`workflows`** invokes a **reusable workflow** by reference, with a stable **`id`** (used in `depends-on`, outputs, and `workspace.from`).
- **`depends-on`** lists **`workflows[].id`** values that must finish before this invocation expands and runs.

The parser expands each invocation into one or more **jobs** (from the workflow definition). Cross-invocation `depends-on` becomes edges from **all** jobs of the producer invocation to **all** jobs of the consumer invocation (after placeholder resolution).

## Global vs project workflows

| Scope | Reference | Typical use |
|-------|-----------|-------------|
| **Global** | `workflow: global/<name>` | Org-wide, reviewed building blocks (checkout, lint, catalog buildx, security scans). Curated by platform admins; version pins are especially important. |
| **Project** | `workflow: project/<name>` | Repository- or team-specific logic (deploy to your cluster, project test matrix). Lives with the project; changes track with the same repo as the pipeline. |

**When to use global:** steps that should stay **consistent across many repos** (compliance, standard tooling, approved images). **When to use project:** anything that encodes **your** environments, URLs, or branching policy that would fork unnecessarily if forced into a shared catalog.

**Versioning:** pin an explicit **`version`** (or semver constraint as supported by your provider) for `global/` references so pipelines do not float on unexpected catalog changes.

## Agent affinity and shared workspace

**`agent-affinity.default-group`** (optional) pins jobs to the same agent **without** sharing a workspace directory.

**`agent-affinity.share-workspace: true`** plus an explicit **`affinity-group`** on each participating invocation enables **shared workspace** semantics: later jobs see the tree produced by earlier jobs in the same group.

**`agent-affinity.allow-parallel-shared-workspace-jobs: true`** disables the parser check that forbids concurrent jobs in the same shared-workspace group. Use only when you rely on **S3-backed passive snapshots** and per-job workspace dirs so parallel branches do not corrupt a single disk directory. Default remains **serial-only** within the group.

For checkout → build examples and operator env vars, see [pipelines.md](pipelines.md#agent-affinity-and-shared-workspace-checkout--build).

## Workspace snapshots (passive and explicit)

When object storage is enabled and snapshots are not disabled, jobs with **`share_workspace`** and an explicit **`affinity-group`** use **passive snapshots**: restore before steps, upload after success. See [ADR-014](adr/014-workspace-snapshots.md).

### Passive predecessor selection

Among dependencies that share the same **`affinity-group`** and **`share_workspace`**, the engine picks the **maximal** predecessor (the one that runs “after” every other in-group dependency in the DAG). If that candidate is **not unique** (e.g. fan-in from parallel branches), there is **no** passive restore; the job starts from an empty workspace unless you set **`workspace.from`**.

### Explicit `workspace:` on an invocation

```yaml
workflows:
  - id: compile
    workflow: project/rust-build
    affinity-group: ci
  - id: audit
    workflow: global/cargo-audit
    affinity-group: ci
    depends-on: [compile]
  - id: seed
    workflow: project/db-seed
    affinity-group: ci
    depends-on: [compile, audit]
    workspace:
      from: compile   # restore compile’s terminal job snapshot, not ambiguous fan-in
```

- **`from`:** must equal some **`workflows[].id`**. The parser resolves the **terminal job** of that invocation (unique sink in that invocation’s job subgraph). You must still **`depends-on`** the producer workflow so scheduling and snapshot registration are valid. Errors use **`E5006`** (unknown id, ambiguous terminal, or missing dependency).
- **`outputs`:** non-empty list of **relative path prefixes** included in the upload tarball; omit or leave empty for a **full** tree (subject to `.gitignore` rules and `.git/` exclusion on the agent). Reduces bandwidth when only `target/` or a cache dir matters.

Dispatch prefers **`workspace.from`** when set; otherwise it uses passive **`workspace_snapshot_predecessor`**.

## Design practices

1. **Keep invocations coarse-grained** — prefer a reusable workflow with a clear contract (inputs/outputs) over dozens of one-off jobs in the pipeline root.
2. **Declare dependencies explicitly** — `depends-on` should reflect **data and workspace** needs, not just human reading order.
3. **Avoid fan-in on shared workspace without `workspace.from`** — if multiple in-group branches feed one job, decide which snapshot tree wins and set **`from`** accordingly.
4. **Use `outputs` for large trees** — shrink snapshots when only build outputs must flow to the next stage.
5. **Do not rely on parallel writers to the same workspace** unless **`allow-parallel-shared-workspace-jobs`** is set and snapshots isolate each `job_run_id` directory.
6. **Secrets** — snapshots are file copies; keep sensitive material out of the workspace or use sealed outputs (see [workflow-invocation-outputs.md](workflow-invocation-outputs.md)).

## Related links

- [ADR-014 — Workspace snapshots and soft affinity](adr/014-workspace-snapshots.md)
- [pipelines.md — Examples and operator settings](pipelines.md)
- [architecture.md — `global/` vs `project/` layout](architecture.md)
- [workflow-invocation-outputs.md](workflow-invocation-outputs.md)
