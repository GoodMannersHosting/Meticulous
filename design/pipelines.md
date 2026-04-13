# General Notes

- Secrets must be checked before pipeline run to make sure they've been loaded in; don't run if the secrets don't exist
- Secret fields MUST ALWAYS be hashed (\*\*\*) when printing to stdout or logs
- Secret fields which have been base64 encoded MUST ALWAYS be hashed (\*\*\*) when printing to stdout or logs

## Basic Docker Build / Push Workflow

1. `git clone https://${scm_url}/${path}.git --depth 1 `
2. `echo "${registry_password}" | docker login --username ${registry_username} --password-stdin ${registry_url}`
3. `docker buildx build --sbom=true --provenance=max,version=v1 ${build_params} -t ${container_tag} ${container_file:=.}`
   3a. <https://docs.docker.com/build/metadata/attestations/>
4. `docker push ${container_tag}`

## Example Yaml

```yaml
# .stable/pipeline-1.yaml
name: pipeline demo
triggers:
  manual: {}
  release:
    tag:
      - "semver bump"
      - "v*"
runs-on:
  tags:
    - amd64: true
    - gpu: false
secrets:
  DOCKER_USERNAME:
    aws:
      arn: arn:aws:secretsmanager:us-east-1:123456789012:secret:prod/docker_username-AbCdEf
  DOCKER_PASSWORD:
    aws:
      arn: arn:aws:secretsmanager:us-east-1:123456789012:secret:prod/docker_password-gHiJkL
vars:
  GIT_REPO: https://github.com/example-org/smol-repo.git
  REGISTRY: ghcr.io/example-org
  CONTAINER: project
workflows:
  # Docker Build Reusable Workflow
  - name: Docker Build
    id: dbap
    workflow: global/docker-build
    version: v0.1
    inputs:
      image_tag: "${REGISTRY}/${CONTAINER}"
      repo: $GIT_REPO
    depends-on: []
  # Docker Push Reusable Workflow
  - name: Docker Push
    id: dp
    workflow: global/docker-push
    version: v0.2
    inputs:
      docker_password: "${DOCKER_PASSWORD}"
      docker_username: "${DOCKER_USERNAME}"
      image_tag: "${REGISTRY}/${CONTAINER}"
    depends-on: [dbap]
```

## Agent affinity and shared workspace (checkout → build)

### What this feature does

Pipeline-level **`agent-affinity`** with **`share-workspace: true`** lets a **chain** of workflow invocations (e.g. git checkout then docker build) share filesystem state. The parser enforces a **strict serial order** inside a shared-workspace group: every pair of jobs in the group must be ordered by **`depends-on`** (no parallel jobs in the same group).

**`share_workspace` is only enabled** when each participating invocation sets an **explicit `affinity-group`** (using only `default-group` pins the agent but does **not** turn on shared workspace—see parser tests in `met-parser`).

### Two ways the workspace is carried across jobs

| Mode | When it applies | What happens |
|------|-----------------|--------------|
| **Shared directory (legacy)** | Object storage is **not** used for snapshots **or** `MET_WORKSPACE_SNAPSHOTS_DISABLED` is set | Engine sets a stable **`workspace_root_id`** per affinity group; jobs on the pinned agent reuse the **same directory** on disk. |
| **Passive snapshots** | met-api has S3 (or compatible) configured **and** snapshots are **not** disabled | Each job run gets its **own** workspace directory; before steps the agent **restores** a `tar.zst` from object storage (if there is an in-group predecessor), and after success **uploads** a new snapshot. See [ADR-014](adr/014-workspace-snapshots.md). |

Explicit per-invocation **`workspace:`** blocks (`from` / `outputs`) are **optional** and separate: they are intended for **subset** restores or special naming, not for the default affinity chain (ADR-014).

### Operator / platform settings (met-api)

| Environment variable | Effect |
|---------------------|--------|
| `MET_WORKSPACE_SNAPSHOTS_DISABLED` | If `1`, `true`, or `yes`, passive snapshots are off; shared-directory behavior is used when `share-workspace` applies. |
| `MET_WORKSPACE_SNAPSHOT_TTL_HOURS` | Optional hint (default **24**, max **168**) stored in the engine for operators; **configure S3 lifecycle** on the `workspace-snapshots/` prefix to match your retention policy. |

Presigned URLs are short-lived (on the order of **one hour**); the engine issues fresh URLs when it dispatches.

### Authoring checklist

1. Set **`agent-affinity.share-workspace: true`** on the pipeline when you need checkout → build style chaining.
2. Give **every** job in the chain the **same** **`affinity-group`** string (not just `default-group`).
3. Order jobs with **`depends-on`** so the DAG is a **total order** within that group.
4. Use paths relative to **`METICULOUS_WORKSPACE`** (or checkout to `.`) so restore sees the same layout.
5. Keep secrets out of the workspace tree when possible; snapshots can contain **any file** that was written under the workspace (`.gitignore` helps but is not a security boundary—see ADR-014 threat model).

### Scheduling expectations

The engine **pins** an affinity group to the **first agent** that runs a job in that group. Later jobs target that agent. If the pinned agent later becomes unavailable, dispatch can fail with an affinity error—**snapshots do not automatically move work to another agent** in the current scheduler. They **do** make each job’s directory self-contained after restore.

Ephemeral agents using **`MET_AGENT_EXIT_AFTER_JOBS=1`**: the engine sets **`suppress_exit_after_jobs_increment`** on dispatches until the last job in the affinity group completes, so the process stays up for the chained jobs.

### Example

```yaml
name: ci
agent-affinity:
  share-workspace: true
workflows:
  - id: checkout
    workflow: global/git-checkout
    version: "1.0.0"
    affinity-group: linux-build
  - id: image
    workflow: global/docker-build
    version: "2.0.0"
    affinity-group: linux-build
    depends-on: [checkout]
```

Workflows should use paths compatible with a shared root (e.g. checkout to `.` or honor `METICULOUS_WORKSPACE`).

## Workflow invocation outputs

For `met-output`, IPC limits, `${{ workflows.<id>.outputs.<name> }}`, and when secret outputs may flow into dependent job environment, see [workflow-invocation-outputs.md](workflow-invocation-outputs.md).
