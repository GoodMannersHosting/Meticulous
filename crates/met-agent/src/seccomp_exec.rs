//! Linux `seccomp(2)` user-notify **exec** capture (planned) vs `/proc` polling (default).
//!
//! ## Current behavior (all platforms)
//!
//! - Process discovery uses [`crate::process_watcher`] (~100ms poll). Very short-lived processes may
//!   be missed between polls. This is the supported fallback documented in ADR-006.
//!
//! ## Planned: `SECCOMP_RET_USER_NOTIF` (Linux only)
//!
//! - Opt-in env: `MET_AGENT_SECCOMP_EXEC_NOTIFY=1` (reserved for a future implementation that
//!   installs a listener). Today this flag **does not** change behavior; it only signals intent for
//!   operators and tests.
//! - Typically requires elevated capability / permissive container policy (often `CAP_SYS_ADMIN`,
//!   or an equivalent seccomp profile) and is **incompatible** with many locked-down runners.
//! - When not available, operators should rely on polling plus [`crate::script_exec_hints`]
//!   (conservative token scan of `StepSpec.command` at dispatch) for common CI binaries such as
//!   `curl` that may exit too quickly for `/proc` sampling inside `podman run`.
//!
//! ## Syscall audit stream (proc-derived)
//!
//! - Set `MET_AGENT_SYSCALL_EXEC_OBSERVE=1` to emit sanitized `LOG_STREAM_SYSCALL` rows alongside
//!   exec-binary telemetry for each `/proc`-observed exec. This is **not** kernel syscall audit;
//!   it mirrors discoveries already made by the watcher.

/// Whether seccomp-based exec notification is enabled via environment (Linux only).
#[must_use]
pub fn user_notify_enabled() -> bool {
    #[cfg(target_os = "linux")]
    {
        std::env::var("MET_AGENT_SECCOMP_EXEC_NOTIFY")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
    }
    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}
