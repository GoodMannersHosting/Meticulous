# ADR-009: Pipeline linter architecture

**Status:** Proposed
**Date:** 2026-03-31
**PRDs:** [030](../prd/030-pipeline-authoring-dag-workflows.md), [130](../prd/130-developer-debug-cli.md)

## Context

Pipeline definitions contain security-critical decisions (secret scope, fork trust, container image pins). A lint step that runs at authoring time catches misconfiguration before a run is ever dispatched. Two categories of feedback exist: deterministic rule violations and context-dependent best-practice suggestions. Mixing them in a single tool conflates their trust levels and deployment requirements.

## Decision

1. **Two separate tools, never one binary:**
   - **`met lint`** — deterministic rule engine. Runs offline (no network calls, no external APIs). Can be used in pre-commit hooks, CI pipelines that validate pipeline YAML itself, and as a blocking gate before a run is dispatched. Produces structured JSON output: `{rule_id, severity, file, line, col, message, remediation_url}`. Source of truth for correctness and security posture.
   - **`met suggest`** — AI-assisted suggestion layer. Takes the `met lint` JSON output and the pipeline YAML as context; calls an LLM API; returns natural-language improvement suggestions. Non-blocking. Never auto-applies changes. Always labeled "AI-generated — verify before applying." Never invoked as part of a run dispatch or CI gate.

2. **Rule taxonomy for `met lint` (minimum viable rule set):**

   | Category | Rules |
   | --- | --- |
   | *Secret hygiene* | Plaintext credential regex in YAML (AWS key, GCP SA JSON, PEM header); secret value in `vars:` matching known prefixes (`ghp_`, `AKIA`, `ya29.`); base64-decodable value in `vars:`; secret passed as CLI argument (ps-visible) rather than env var |
   | *Fork/PR trust* | Step injects secrets when trigger could be `EXTERNAL` tier without an approval gate declared; `fork_policy: allow_secrets` without a `# risk-acknowledged` comment |
   | *Supply chain* | Reusable workflow reference pinned to branch name rather than commit SHA; `curl \| sh` in run step; container image without digest pin (`image: foo:latest` vs `image: foo@sha256:...`) |
   | *Structural* | `depends-on` cycle; unreachable job (impossible dependency combination); missing secret provider config for declared secret reference; duplicate job ID |
   | *Security posture* | `clone_depth: null` (full history clone) when no step requires git history; step with `debug: true` without a comment justifying it |

3. **Severity levels:** `error` (blocks dispatch when lint gate is enabled), `warning` (visible in output, does not block), `info` (style/best-practice).

4. **Integration points:**
   - `met lint` is called by the pipeline engine during YAML ingestion as a server-side validation pass before storing a pipeline definition.
   - `met lint` is also a standalone CLI command for local authoring.
   - `met suggest` is a separate CLI command and an opt-in UI panel; never called server-side.
   - The lint gate for dispatch is configurable per project: `lint_gate: none | warnings | errors` (default: `errors`).

5. **Output format:** JSON Lines (`\n`-delimited JSON objects) for machine parsing; a human-readable summary on stderr for interactive use. Compatible with standard editor diagnostic protocols (LSP via a future language server wrapper).

## Consequences

- Rule definitions live in `crates/met-lint/src/rules/` as individual modules; each rule is independently testable with golden YAML fixtures.
- The `met-lint` crate must have no async dependencies and no network I/O — it must be `Send + Sync` and usable in a blocking context.
- `met suggest` is a thin client that calls the Anthropic API (or a configured LLM endpoint); it is not part of the Rust workspace's core crates and has no stability guarantee.
- Adding a new lint rule requires: the rule module, a golden test, a `remediation_url` pointing to docs, and a severity assignment. No rule ships without all four.

## Threat model

- **Assets:** Pipeline YAML may contain sensitive patterns that lint should detect and report without echoing back the matched value in error messages (avoid leaking credentials in lint output).
- **Adversaries:** A pipeline author crafting YAML designed to evade regex rules (e.g., splitting a credential across YAML anchors). Mitigations: rules operate on the parsed AST, not raw text, where possible.
- **Residual risk:** Regex-based secret detection has false negatives. Lint is defense-in-depth, not a substitute for the per-job PKI and NATS no-cleartext rules.

## References

- [open-questions.md](../open-questions.md) Pipeline quality section (resolved)
- [PRD-030](../prd/030-pipeline-authoring-dag-workflows.md) FR-2 (schema validation)
- [PRD-130](../prd/130-developer-debug-cli.md) FR-6, FR-7 (`met lint`, `met suggest`)
- [ADR-005](005-scm-webhook-security.md) fork/PR trust tier model (lint rules reference this)
