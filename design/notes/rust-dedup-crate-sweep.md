# Rust dedup audit — crate sweep (2026)

Ordered pass: **met-api → met-store → met-controller / met-agent / met-secrets → met-parser / met-engine → met-cli → smaller crates**, using `cargo clippy --workspace --all-targets` plus targeted greps (`parse_*`, `split_once`, duplicate comments).

## Consolidated into `met-core`

| Helper | Module / export | Consumers |
| --- | --- | --- |
| SQL offset cursor (`trim`, non-negative `i64`) | `met_api::extractors::parse_sql_offset_cursor` (shared helper lives next to pagination; not duplicated in `met-core` to avoid pulling Axum-only docs into core) | `runs`, `workflows_catalog` |
| `/proc/net/tcp*` hex `ip:port` | `met_core::proc_net_tcp::parse_hex_ip_port` → `(String, u16)` | `met-agent` telemetry, `met-secrets` syscall audit; secrets also reads `/proc/net/tcp6` for parity with the agent |
| `key=value` (first `=`, non-empty key) | `met_core::split_key_value` | `met-cli` (`parse_key_value`), OAuth callback query parsing, `output_ipc::parse_key_value_arg` |

**Note:** Offset pagination stayed in `met-api` because it is HTTP-cursor semantics tied to the API extractor; the duplicate private `fn`s were merged there.

## `met-store`

- **`ProjectRole` / `PipelineRole`:** `std::str::FromStr` with `type Err = ()`, `max_*_role` unchanged. Beware `crate::error::Result` alias: use `std::result::Result` in `from_str` bodies and in tests when asserting `Err(())`.

## Worth keeping local (not lifted)

- **met-parser / met-engine:** Domain-specific YAML/IR validation, DAG, semver; Levenshtein in `met-core::fuzzy` stays intentional (ADR-019).
- **met-api:** Route-specific filters (`parse_run_status_filter`, etc.), OpenAPI wiring.
- **met-controller / met-operator:** K8s and dispatch logic; no new cross-crate string copies found in this pass beyond proc-net (done).
- **met-objstore, met-logging, met-telemetry, met-proto, met-lint, met-secret-resolve:** Small surface; clippy runs clean aside from workspace-wide warning noise.

## Recurring clippy themes (optional follow-ups)

- `collapsible_if`, `field_reassign_with_default` in large route files (`met-api`).
- `met-engine` integration tests: nested `if let` chains flagged as collapsible.

## Tests

- `met-parser` DB tests (`providers::database::*`) require **`--features database`**; they do not run in default `cargo test -p met-parser --lib`.
