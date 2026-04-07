//! Best-effort detection of **shell-invoked programs** from the step `run:` script text.
//!
//! True “on exec” telemetry inside an isolated container would need seccomp user-notify, ptrace, or
//! an OCI hook — not available in the default `podman run … sh -c` path. Here we instead record
//! programs **referenced at dispatch** (when Meticulous starts the step) using a conservative
//! regex, merged into footprint metadata with [`SCRIPT_INFERRED_BINARY_SHA256`] so short-lived
//! processes like `curl` in `curl | sh` still appear alongside `/proc`-observed rows.

use std::collections::HashSet;
use std::path::Path;
use std::sync::LazyLock;

use chrono::Utc;
use regex::Regex;

use crate::process_watcher::{
    ExecutedBinaryRecord, JobExecutionMetadata, SCRIPT_INFERRED_BINARY_SHA256,
};

/// Token set: basenames (or explicit `/path/...`) commonly invoked in CI scripts.
static KNOWN_TOOLS: &[&str] = &[
    "curl",
    "wget",
    "git",
    "tar",
    "gzip",
    "gunzip",
    "zip",
    "unzip",
    "docker",
    "podman",
    "trivy",
    "nerdctl",
    "kubectl",
    "helm",
    "kustomize",
    "npm",
    "npx",
    "yarn",
    "pnpm",
    "node",
    "bun",
    "deno",
    "cargo",
    "rustc",
    "rustup",
    "go",
    "python3",
    "python",
    "pip3",
    "pip",
    "pipenv",
    "poetry",
    "uv",
    "ruby",
    "bundle",
    "gem",
    "java",
    "mvn",
    "gradle",
    "cmake",
    "ninja",
    "meson",
    "make",
    "gcc",
    "g++",
    "clang",
    "clang++",
    "ld",
    "ld.lld",
    "patch",
    "applypatch",
    "rsync",
    "scp",
    "ssh",
    "sh",
    "bash",
    "dash",
    "zsh",
    "fish",
    "pwsh",
    "powershell",
];

static TOOL_INVOCATION_RE: LazyLock<Regex> = LazyLock::new(|| {
    let alt = KNOWN_TOOLS.join("|");
    Regex::new(&format!(
        r"(?m)(?:^|[;&|]|\|\||&&|\n)\s*(?:sudo\s+|env\s+-i\s+)?((?:[/\w.-]+/)?(?:{alt}))\b"
    ))
    .expect("tool hint regex")
});

/// Normalize a matched token to an absolute-style path for storage / UI.
fn normalize_invocation(token: &str) -> String {
    let t = token.trim();
    if t.contains('/') {
        t.to_string()
    } else {
        format!("/usr/bin/{t}")
    }
}

/// Return deduped normalized paths for tools that appear to be invoked in `script`.
pub fn hinted_binary_paths_from_command(script: &str) -> Vec<String> {
    let mut seen = HashSet::<String>::new();
    let mut out = Vec::new();
    for cap in TOOL_INVOCATION_RE.captures_iter(script) {
        let Some(m) = cap.get(1) else {
            continue;
        };
        let path = normalize_invocation(m.as_str());
        if seen.insert(path.clone()) {
            out.push(path);
        }
    }
    out
}

#[inline]
fn basename_key(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(path)
        .to_ascii_lowercase()
}

/// True if `observed_path` is the same tool as `hint_path` (basename match).
fn observed_covers_hint(observed_path: &str, hint_path: &str) -> bool {
    basename_key(observed_path) == basename_key(hint_path)
}

/// Add script-derived rows for tools not already present in `meta.executed_binaries`.
pub fn merge_command_hints_into_metadata(
    command: &str,
    step_id: &str,
    step_run_id: &str,
    meta: &mut JobExecutionMetadata,
) {
    let hints = hinted_binary_paths_from_command(command);
    if hints.is_empty() {
        return;
    }

    let now = Utc::now();
    'hint: for path in hints {
        for existing in &meta.executed_binaries {
            if existing.path == path || observed_covers_hint(&existing.path, &path) {
                continue 'hint;
            }
        }
        meta.executed_binaries.push(ExecutedBinaryRecord {
            path,
            sha256: SCRIPT_INFERRED_BINARY_SHA256.to_string(),
            execution_count: 1,
            first_executed_at: now,
            last_executed_at: now,
            step_ids: vec![step_id.to_string()],
            step_run_ids: if step_run_id.is_empty() {
                vec![]
            } else {
                vec![step_run_id.to_string()]
            },
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mar_operator_trivy_install_snippet() {
        let script = r#"set -euo pipefail
WS="${METICULOUS_WORKSPACE:?}"
mkdir -p "${WS}/.local/bin"
if ! command -v curl >/dev/null 2>&1; then
  echo "curl is required" >&2
  exit 1
fi
curl -sfL https://raw.githubusercontent.com/aquasecurity/trivy/main/contrib/install.sh \
  | sh -s -- -b "${WS}/.local/bin" "v${TRIVY_VERSION}"
"#;
        let p = hinted_binary_paths_from_command(script);
        assert!(
            p.iter().any(|x| x.ends_with("/curl")),
            "curl hint missing: {p:?}"
        );
        assert!(
            p.iter().any(|x| x.ends_with("/sh")),
            "sh hint missing: {p:?}"
        );
    }

    #[test]
    fn mar_operator_if_else_branch_still_finds_curl() {
        // Mirrors `.stable/workflows/mar-operator-image.yaml` "Generate SBOM" step: `curl` only
        // appears in the `else` branch. Static hint scan sees the whole `run:` block — `if` does not
        // hide lines from the regex. (At runtime, if `trivy` is on PATH, `curl` never runs — only
        // script inference would list it, not /proc.)
        let script = r#"set -euo pipefail
WS="${METICULOUS_WORKSPACE:?}"
TRIVY_VERSION="${TRIVY_VERSION:-0.58.2}"

if command -v trivy >/dev/null 2>&1; then
  echo "Using trivy from PATH: $(command -v trivy)"
else
  echo "Installing Trivy v${TRIVY_VERSION} into ${WS}/.local/bin ..."
  mkdir -p "${WS}/.local/bin"
  curl -sfL https://raw.githubusercontent.com/aquasecurity/trivy/main/contrib/install.sh \
    | sh -s -- -b "${WS}/.local/bin" "v${TRIVY_VERSION}"
  export PATH="${WS}/.local/bin:${PATH}"
fi

trivy version
"#;
        let p = hinted_binary_paths_from_command(script);
        assert!(
            p.iter().any(|x| x.ends_with("/curl")),
            "curl inside else should still match: {p:?}"
        );
        assert!(p.iter().any(|x| x.ends_with("/sh")), "sh: {p:?}");
        assert!(
            p.iter().any(|x| x.ends_with("/trivy")),
            "trivy after fi should match: {p:?}"
        );
    }

    #[test]
    fn command_v_does_not_count_as_invocation() {
        let script = "if ! command -v curl >/dev/null 2>&1; then exit 1; fi";
        assert!(
            !hinted_binary_paths_from_command(script)
                .iter()
                .any(|p| p.ends_with("/curl")),
            "should not treat `command -v curl` as running curl"
        );
    }

    #[test]
    fn merge_skips_when_observed_same_basename() {
        let mut meta = JobExecutionMetadata {
            executed_binaries: vec![ExecutedBinaryRecord {
                path: "/bin/curl".to_string(),
                sha256: "deadbeef".to_string(),
                execution_count: 1,
                first_executed_at: Utc::now(),
                last_executed_at: Utc::now(),
                step_ids: vec!["s1".into()],
                step_run_ids: vec![],
            }],
            total_processes_spawned: 1,
            execution_tree_depth: 1,
        };
        merge_command_hints_into_metadata(
            "curl -V",
            "s1",
            "srun_00000000-0000-0000-0000-000000000001",
            &mut meta,
        );
        assert_eq!(meta.executed_binaries.len(), 1);
    }
}
