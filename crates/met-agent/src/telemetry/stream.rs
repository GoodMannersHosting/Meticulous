//! Streaming exec / syscall / runtime-script chunks on the `StreamLogs` path.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use met_proto::agent::v1::LogStream;
use serde_json::json;
use sha2::{Digest, Sha256};
use tokio::fs::File;
use tokio::io::AsyncReadExt;

use super::redact_path;
use crate::error::Result;
use crate::process_watcher::ExecutedBinary;
use crate::step_log::StepLogPipe;

/// Max bytes read from a single interpreted file under the workspace per step.
pub const MAX_RUNTIME_SCRIPT_BYTES_PER_FILE: u64 = 65_536;
/// Max aggregated bytes of runtime script capture per step (storage DoS mitigation).
pub const MAX_RUNTIME_SCRIPT_BYTES_PER_STEP: u64 = 262_144;

/// When set, emit a `LOG_STREAM_SYSCALL` row for each exec discovery (proc poll), not kernel audit.
#[must_use]
pub fn syscall_exec_observe_enabled() -> bool {
    std::env::var("MET_AGENT_SYSCALL_EXEC_OBSERVE")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

async fn emit_exec_binary(
    pipe: &StepLogPipe,
    step_sequence: i32,
    b: &ExecutedBinary,
) -> Result<()> {
    let path = redact_path(&b.path.to_string_lossy());
    let v = json!({
        "path": path,
        "sha256": b.sha256,
        "step_sequence": step_sequence,
        "pid": b.pid,
        "ppid": b.parent_pid,
    });
    let s = v.to_string();
    pipe.send_telemetry(LogStream::ExecBinary, &s).await?;
    Ok(())
}

async fn maybe_emit_syscall_observe(pipe: &StepLogPipe, b: &ExecutedBinary) -> Result<()> {
    if !syscall_exec_observe_enabled() {
        return Ok(());
    }
    let v = json!({
        "nr": 59,
        "name": "execve",
        "outcome": "observed",
        "return_code": serde_json::Value::Null,
        "pid": b.pid,
        "tid": b.pid,
        "metadata": { "via": "proc_poll" }
    });
    pipe.send_telemetry(LogStream::Syscall, &v.to_string())
        .await?;
    Ok(())
}

async fn maybe_emit_runtime_script(
    pipe: &StepLogPipe,
    exe_path: &Path,
    workspace_canon: &Path,
    budget: &mut u64,
    seen: &mut HashSet<PathBuf>,
) -> Result<()> {
    if *budget == 0 {
        return Ok(());
    }
    if !exe_path.starts_with(workspace_canon) {
        return Ok(());
    }
    let owned = exe_path.to_path_buf();
    if seen.contains(&owned) {
        return Ok(());
    }

    let meta = match tokio::fs::metadata(exe_path).await {
        Ok(m) => m,
        Err(_) => return Ok(()),
    };
    if !meta.is_file() {
        return Ok(());
    }
    let len = meta.len();
    if len > 10_000_000 {
        return Ok(());
    }

    let mut file = File::open(exe_path).await.map_err(|e| {
        crate::error::AgentError::Workspace(format!(
            "runtime script open {}: {e}",
            exe_path.display()
        ))
    })?;

    let per_file_cap = MAX_RUNTIME_SCRIPT_BYTES_PER_FILE.min(*budget) as usize;
    let mut buf = vec![0u8; per_file_cap.saturating_add(1)];
    let got = file
        .read(&mut buf)
        .await
        .map_err(|e| crate::error::AgentError::Workspace(format!("runtime script read: {e}")))?;
    if got >= 4 && buf[..4] == [0x7f, b'E', b'L', b'F'] {
        return Ok(());
    }
    let truncated = got > per_file_cap;
    let use_len = got.min(per_file_cap);
    buf.truncate(use_len);

    let mut hasher = Sha256::new();
    hasher.update(&buf);
    let sha = hex::encode(hasher.finalize());

    *budget -= use_len as u64;
    seen.insert(owned);

    let v = json!({
        "sha256_hex": sha,
        "byte_length": use_len as i64,
        "truncated": truncated,
        "path_redacted": redact_path(&exe_path.to_string_lossy()),
        "object_key": serde_json::Value::Null,
        "audit": "runtime_script_capture"
    });

    pipe.send_telemetry(LogStream::RuntimeScript, &v.to_string())
        .await?;
    Ok(())
}

/// For each newly discovered process, stream exec telemetry and optional script/syscall records.
pub async fn emit_for_discovered_processes(
    pipe: &StepLogPipe,
    step_sequence: i32,
    binaries: &[ExecutedBinary],
    workspace_canon: &Path,
    runtime_budget: &mut u64,
    runtime_seen: &mut HashSet<PathBuf>,
) -> Result<()> {
    for b in binaries {
        emit_exec_binary(pipe, step_sequence, b).await?;
        maybe_emit_syscall_observe(pipe, b).await?;
        maybe_emit_runtime_script(pipe, &b.path, workspace_canon, runtime_budget, runtime_seen)
            .await?;
    }
    Ok(())
}
