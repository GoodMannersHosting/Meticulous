//! Agent telemetry policy (see ADR-006 / PRD 070).
//!
//! - Environment variable **names** may appear in dispatched `StepSpec`; **values** must never be
//!   shipped as log or exec telemetry.
//! - Default exec telemetry is **resolved path + SHA-256 + pid/ppid** only; full `argv` is not
//!   streamed (flags often embed bearer tokens and signed URLs).
//! - **`curl … | bash`:** bodies fetched over TLS are not visible to the agent; rely on pipeline IR
//!   server-side, exec telemetry, and optional bounded runtime capture.

mod stream;

#[cfg(target_os = "linux")]
mod net_flow;

pub use stream::{
    emit_for_discovered_processes, syscall_exec_observe_enabled, MAX_RUNTIME_SCRIPT_BYTES_PER_FILE,
    MAX_RUNTIME_SCRIPT_BYTES_PER_STEP,
};

#[cfg(target_os = "linux")]
pub(crate) use net_flow::emit_new_network_flows;

#[cfg(not(target_os = "linux"))]
pub(crate) async fn emit_new_network_flows(
    _pipe: &crate::step_log::StepLogPipe,
    _step_sequence: i32,
    _watcher: &crate::process_watcher::ProcessWatcher,
) -> crate::error::Result<()> {
    Ok(())
}

/// Redact high-PII path prefixes before emitting JSON telemetry (aligns with controller ingestion).
pub fn redact_path(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("/home/") {
        if let Some(idx) = rest.find('/') {
            return format!("/home/<redacted>{}", &rest[idx..]);
        }
        return "/home/<redacted>".to_string();
    }
    path.to_string()
}
