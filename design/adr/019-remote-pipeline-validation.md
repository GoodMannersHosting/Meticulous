# ADR-019: Remote pipeline validation and diagnostic codes

**Status:** Proposed  
**Date:** 2026-04-11  
**PRDs:** [030](../prd/030-pipeline-authoring-dag-workflows.md), [130](../prd/130-developer-debug-cli.md)

## Context

`met lint` (ADR-009) and `met pipeline validate` run offline against local YAML. They catch structural errors and security anti-patterns but cannot verify server-side state:

1. **Secret existence.** A pipeline references `secrets.DB_PASSWORD` with `provider: vault`, but the secret or provider config may not exist on the server.
2. **Variable definitions.** A pipeline uses `${{ vars.DEPLOY_REGION }}`, but the variable may not be defined at the project or environment level.
3. **Workflow trust.** A pipeline calls `workflow: global/deploy`, but the workflow may not be approved for use in this project.
4. **Environment validity.** A job targets `environment: production`, but that environment may not exist or may have branch restrictions that would block the current ref.
5. **Typos.** A reference to `secrets.DB_PASWORD` (missing 's') fails silently at dispatch time. Fuzzy matching at validation time can suggest corrections.

A server-side validation endpoint fills this gap: it parses the pipeline using the same engine as dispatch, resolves references against real project state, and returns structured diagnostics. The offline `met pipeline validate` command is unchanged and remains available for network-free use.

## Decision

### API endpoint

```
POST /api/v1/projects/{project_id}/pipelines/check
```

**Request body:**

```json
{
    "definition": "... pipeline YAML string ...",
    "ref": "refs/heads/main",
    "environment": "staging"
}
```

- `definition`: Raw pipeline YAML. Required.
- `ref`: Simulated trigger ref for branch restriction checks. Optional; defaults to the project's default branch.
- `environment`: Specific environment to validate against. Optional; when omitted, validates all environments referenced in the definition.

**Response:**

```json
{
    "valid": false,
    "diagnostics": [
        {
            "code": "CHK-003",
            "severity": "error",
            "message": "Secret 'DB_PASWORD' not found in project scope",
            "location": { "line": 12, "column": 5 },
            "suggestion": "Did you mean 'DB_PASSWORD'?",
            "doc_url": "https://docs.meticulous.example.com/diagnostics/CHK-003"
        }
    ],
    "summary": {
        "errors": 1,
        "warnings": 0,
        "info": 0
    }
}
```

**HTTP status:** Always `200` if the request is well-formed (even when diagnostics contain errors). `400` for malformed requests (missing `definition`, invalid JSON). `401`/`403` for auth failures.

### Authorization

Requires an authenticated user with at least `readonly` on the target project (per ADR-021 permission model). The endpoint reads project state (secrets metadata, variables, environments, workflows) but never returns secret values — only names and existence checks. `met pipeline validate` remains offline-only and requires no auth.

### Server-side validation pipeline

The check endpoint executes the following validation stages in order. Each stage accumulates diagnostics; all stages run even if earlier stages produce errors (to give a complete report).

#### Stage 1: Parse and lint

Parse the YAML using `met-parser` with `DatabaseWorkflowProvider` (to resolve `workflow: global/...` and `workflow: project/...` references). Run the `met-lint` rule set (ADR-009) against the parsed AST. Lint diagnostics are included with their existing rule IDs (e.g. `SC-004`), not re-coded as `CHK-*`.

#### Stage 2: Workflow trust and approval

For each reusable workflow reference:

1. Verify the workflow exists at the specified scope (global or project).
2. Check the workflow's trust status:
   - **Global workflows:** Always trusted (managed by platform operators).
   - **Project workflows:** Trusted if committed to the project's default branch (verified by comparing the SHA in the workflow reference against the HEAD of the default branch).
   - **External workflow references** (future): Require explicit approval in the project settings.
3. If the workflow reference is pinned to a branch name instead of a commit SHA, emit `CHK-001` (see diagnostic codes).

#### Stage 3: Secret reference validation

For each secret reference in the pipeline:

1. Look up the secret by name in the project's scope hierarchy (environment → project → org, per ADR-016 resolution order).
2. If the secret references an external provider, verify that a matching `secret_provider_configs` entry exists (ADR-020).
3. If not found, emit `CHK-003` with fuzzy match suggestions.
4. For external provider references: validate that the provider config's `resolution_mode` is compatible with the YAML-level `resolution` field (if specified).

#### Stage 4: Variable validation

For each variable reference (`${{ vars.NAME }}`):

1. Check against the project's defined variables, pipeline-level variables, and (if `environment` is specified) environment-scoped variables (ADR-016).
2. If not found, emit `CHK-005` with fuzzy match suggestions.
3. If found at a scope that would be overridden by a narrower scope, emit `CHK-006` as informational.

#### Stage 5: Environment validation

For each `environment:` reference in workflow invocations:

1. Verify the environment exists in the project.
2. If `ref` is provided, check against `allowed_branches`. Emit `CHK-007` if the ref would be blocked.
3. If `require_approval` is true, emit `CHK-008` as informational (so the developer knows the pipeline will pause).

#### Stage 6: Structural cross-checks

1. Verify that all `needs:` references point to existing job or invocation IDs.
2. Detect dependency cycles (re-run of the DAG cycle check from `met-lint`, but against the fully expanded workflow graph).
3. Verify that `workspace.from` references (ADR-014) point to invocations that declare `outputs`.

### Diagnostic codes

| Code | Severity | Stage | Description |
| --- | --- | --- | --- |
| `CHK-001` | Warning | 2 | Workflow reference pinned to branch name, not commit SHA |
| `CHK-002` | Error | 2 | Workflow not found at specified scope |
| `CHK-003` | Error | 3 | Secret not found in scope hierarchy |
| `CHK-004` | Error | 3 | External provider config not found for secret reference |
| `CHK-005` | Error | 4 | Variable not defined in any scope |
| `CHK-006` | Info | 4 | Variable shadowed by narrower scope |
| `CHK-007` | Error | 5 | Environment branch restriction blocks the given ref |
| `CHK-008` | Info | 5 | Environment requires approval (pipeline will pause) |
| `CHK-009` | Error | 5 | Environment not found |
| `CHK-010` | Error | 6 | `needs` references non-existent job or invocation ID |
| `CHK-011` | Error | 6 | Dependency cycle detected in expanded workflow graph |
| `CHK-012` | Warning | 6 | `workspace.from` references invocation with no declared outputs |

All diagnostic codes are stable. New codes are appended, never reused. Each code has a permanent `doc_url` that explains the diagnostic and remediation steps.

### Fuzzy matching

For `CHK-003` (missing secret) and `CHK-005` (missing variable), the endpoint computes suggestions using edit distance:

1. Collect all valid names at the applicable scope (secrets visible to the project, variables defined at project/environment/pipeline level).
2. Compute Levenshtein distance between the reference and each valid name.
3. Include names with distance ≤ 2, or distance ≤ 3 for names longer than 10 characters, sorted by distance ascending.
4. Maximum 3 suggestions per diagnostic.
5. Suggestions appear in the `suggestion` field as a human-readable string: `"Did you mean 'DB_PASSWORD'?"` or `"Did you mean one of: 'DB_PASSWORD', 'DB_PASS'?"`.

### CLI command

```
met pipeline check [path] --project <slug> [--environment <name>] [--ref <ref>]
```

- `path`: Path to pipeline YAML file (default: `.meticulous/pipeline.yaml`).
- `--project`: Project slug (required; used to look up server-side state).
- `--environment`: Optional environment to validate against.
- `--ref`: Simulated trigger ref (default: current git branch).

**Output:** Structured diagnostics on stdout (JSON Lines format, same as `met lint`). Human-readable summary on stderr. Exit code `0` if no errors, `1` if any error-severity diagnostics.

**Pre-commit hook usage:**

```yaml
# .pre-commit-config.yaml
- repo: local
  hooks:
    - id: met-pipeline-check
      name: Pipeline validation
      entry: met pipeline check --project my-project
      files: '\.meticulous/.*\.ya?ml$'
      language: system
      pass_filenames: true
```

### Workflow trust gating

Beyond validation, the check endpoint enforces a trust model for reusable workflows:

1. **Untrusted workflow detection:** If a workflow reference resolves to a file that is not on the project's default branch (e.g. introduced in a PR), the check reports `CHK-001` as a warning and marks the workflow as untrusted.
2. **Trust override:** Project admins can approve specific workflow SHAs via the API (`POST /projects/{id}/workflows/{workflow_id}/approve`). Approved SHAs are stored in a `workflow_approvals` table and consulted during validation.
3. **Dispatch integration (future):** When the engine dispatches a run, it re-runs Stage 2 validation. If an untrusted workflow is detected and the project's `workflow_trust_policy` is `block`, the run is rejected. This is not implemented in the check endpoint itself — the endpoint only reports diagnostics.

### Implementation location

- **Validation logic:** New module `crates/met-engine/src/pipeline_check.rs`. Reuses the existing parser, lint engine, and `DatabaseWorkflowProvider`. Each validation stage is a separate function returning `Vec<Diagnostic>`.
- **API route:** `crates/met-api/src/routes/pipeline_check.rs`. Thin handler that deserializes the request, calls the validation module, and serializes the response.
- **CLI:** `crates/met-cli/src/commands/pipeline_check.rs`. Reads the YAML file, authenticates to the server, POSTs to the check endpoint, and formats the response.
- **Fuzzy matching:** Utility function in `crates/met-core/src/fuzzy.rs` (reusable for future features like variable autocomplete).

## Consequences

### Positive

- Developers catch misconfigured secrets, variables, and environments before dispatch, reducing failed runs.
- Fuzzy matching turns cryptic "not found" errors into actionable suggestions.
- Structured diagnostic codes enable programmatic integration (IDE extensions, pre-commit hooks, CI gates).
- Workflow trust gating provides visibility into untrusted workflow usage without blocking development workflows.
- The check endpoint reuses the same parser and resolver as the engine, so validation fidelity matches actual dispatch behavior.

### Negative

- The check endpoint reads project state (secrets metadata, variables, environments, workflows), adding load to the database. Rate limiting per user/project is recommended.
- Fuzzy matching over large secret/variable namespaces adds latency. Bounded by collecting names only (not values) and limiting to edit distance ≤ 3.
- The endpoint cannot validate runtime-only conditions (agent availability, resource quotas, dynamic variables set by prior jobs). Diagnostics are limited to static, pre-dispatch validation.
- Workflow trust status depends on git state (default branch HEAD), which may change between validation and dispatch. The check result is advisory, not a guarantee.

### Migration notes

- No database migrations required for the core check functionality. Secret, variable, environment, and workflow state is read from existing tables.
- The optional `workflow_approvals` table (for trust overrides) is deferred to implementation and will be a separate migration if needed.
- New CLI command; no breaking changes to existing `met pipeline validate` or `met lint` commands.
- New API route; no changes to existing routes.

## Threat model

- **Assets:** Project secret names (not values) and variable names are exposed in diagnostics and fuzzy suggestions. Workflow definitions and environment configurations are read during validation.
- **Adversaries:** Unauthenticated user attempting to enumerate project secrets via the check endpoint; authenticated user with `readonly` probing for secret names in projects they should not access; crafted YAML designed to cause excessive server-side processing (ReDoS, workflow expansion bomb).
- **Mitigations:**
  - Authentication required (`readonly` minimum, per ADR-021). Unauthenticated requests receive `401`.
  - The endpoint returns secret and variable **names** only in diagnostics, never values. Name enumeration is bounded by the caller's existing project read access — they could already list secrets/variables via dedicated API routes.
  - Fuzzy suggestions are computed from the same name set the caller can already access; no information leak beyond what `GET /secrets` or `GET /variables` would reveal.
  - YAML parsing has existing depth and node count limits (from `met-parser`). Workflow expansion is bounded by a configurable max depth (default 10 levels) and max total jobs (default 1000).
  - Rate limiting: 10 requests per minute per user per project (configurable).
  - The check endpoint is read-only; it never mutates state, dispatches runs, or resolves secret values.
- **Residual risk:** A `readonly` user can confirm the existence of secrets and variables by name. This is acceptable because `readonly` already grants access to pipeline definitions that contain these references. If stricter name hiding is needed, a future enhancement could filter diagnostics based on a more granular permission.

**Certificates:** Not directly applicable. The check endpoint is served over HTTPS; TLS certificate health should be verified per workspace certificate rules.

## References

- [ADR-009](009-pipeline-linter-architecture.md) — lint rule engine; check endpoint includes lint as Stage 1
- [ADR-010](010-project-and-scm-data-model.md) — secret scope hierarchy; check validates against this model
- [ADR-014](014-workspace-snapshots.md) — workspace snapshots; `CHK-012` validates `workspace.from` references
- [ADR-016](016-pipeline-environments.md) — pipeline environments; Stage 5 validates environment references
- [ADR-020](020-external-secret-providers.md) — external providers; `CHK-004` validates provider config existence
- [ADR-021](021-resource-visibility-pipeline-acl.md) — permission model; check requires `readonly` on project
- [`crates/met-parser/src/`](../../crates/met-parser/src/) — YAML parser and workflow provider
- [`crates/met-engine/src/`](../../crates/met-engine/src/) — engine; new `pipeline_check.rs` module
- [`crates/met-api/src/routes/`](../../crates/met-api/src/routes/) — API routes; new `pipeline_check.rs`
- [`crates/met-cli/src/`](../../crates/met-cli/src/) — CLI; new `pipeline_check.rs` command
