//! Execution backends for running steps.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use async_trait::async_trait;

use crate::config::ExecutionRuntime;
use crate::error::Result;
use crate::process_watcher::{ExecutedBinary, ExecutedBinaryRecord, ProcessWatcher};
use crate::step_log::StepLogPipe;
use tracing::warn;

#[cfg(target_os = "linux")]
mod container;
pub(crate) mod native;

#[cfg(target_os = "linux")]
pub use container::ContainerBackend;
pub use native::NativeBackend;

/// Specification for a step to execute.
#[derive(Debug, Clone)]
pub struct StepSpec {
    pub step_id: String,
    pub step_run_id: String,
    pub step_sequence: i32,
    pub name: String,
    pub command: String,
    pub image: String,
    pub working_dir: String,
    pub shell: String,
    pub environment: HashMap<String, String>,
    pub timeout: Duration,
    /// Secret values that must be redacted from log output. Contains only the
    /// values (not the keys), so any occurrence in stdout/stderr is masked.
    pub secret_values: std::sync::Arc<Vec<String>>,
}

/// Result of step execution.
#[derive(Debug)]
pub struct StepResult {
    pub exit_code: i32,
    pub duration: Duration,
    pub executed_binaries: Vec<ExecutedBinaryRecord>,
    pub processes_spawned: u64,
    pub execution_tree_depth: u32,
    /// Raw `met-output` IPC bytes captured for this step (native Unix with `METICULOUS_OUTPUT_FD`).
    pub output_ipc_bytes: Vec<u8>,
}

impl StepResult {
    /// Create a minimal result without execution metadata.
    pub fn simple(exit_code: i32, duration: Duration) -> Self {
        Self {
            exit_code,
            duration,
            executed_binaries: Vec::new(),
            processes_spawned: 0,
            execution_tree_depth: 0,
            output_ipc_bytes: Vec::new(),
        }
    }
}

/// Wakeup interval between `/proc` scans while a step runs (native or container CLI on host).
pub(crate) const PROCESS_WATCHER_POLL_INTERVAL: Duration = Duration::from_millis(50);

/// After any newly seen descendant, sample a few more times quickly so very short-lived children
/// (typical `curl` in `curl | sh` install snippets) are still captured for footprint / exec rows.
const SHORT_LIVED_CHILD_BURST_SAMPLES: u32 = 25;
const SHORT_LIVED_CHILD_BURST_SLEEP: Duration = Duration::from_millis(4);

async fn emit_poll_batch(
    watcher: &ProcessWatcher,
    logs: Option<&StepLogPipe>,
    step: &StepSpec,
    workspace_canon: &Path,
    runtime_budget: &mut u64,
    runtime_seen: &mut HashSet<PathBuf>,
    discovered: &[ExecutedBinary],
) -> Result<()> {
    if let Some(pipe) = logs {
        crate::telemetry::emit_for_discovered_processes(
            pipe,
            step.step_sequence,
            discovered,
            workspace_canon,
            runtime_budget,
            runtime_seen,
        )
        .await?;
        crate::telemetry::emit_new_network_flows(pipe, step.step_sequence, watcher).await?;
    }
    Ok(())
}

pub(crate) async fn poll_watcher_emit_telemetry(
    watcher: &ProcessWatcher,
    logs: Option<&StepLogPipe>,
    step: &StepSpec,
    workspace_canon: &Path,
    runtime_budget: &mut u64,
    runtime_seen: &mut HashSet<PathBuf>,
) -> Result<()> {
    let mut discovered = watcher.poll().await?;
    emit_poll_batch(
        watcher,
        logs,
        step,
        workspace_canon,
        runtime_budget,
        runtime_seen,
        &discovered,
    )
    .await?;

    if !discovered.is_empty() {
        for _ in 0..SHORT_LIVED_CHILD_BURST_SAMPLES {
            tokio::time::sleep(SHORT_LIVED_CHILD_BURST_SLEEP).await;
            discovered = watcher.poll().await?;
            if discovered.is_empty() {
                continue;
            }
            emit_poll_batch(
                watcher,
                logs,
                step,
                workspace_canon,
                runtime_budget,
                runtime_seen,
                &discovered,
            )
            .await?;
        }
    }
    Ok(())
}

/// Trait for step execution backends.
#[async_trait]
pub trait ExecutionBackend: Send + Sync {
    /// Execute a step and return the result including execution metadata.
    async fn execute_with_watcher(
        &self,
        step: &StepSpec,
        workspace: &Path,
        watcher: &mut ProcessWatcher,
        logs: Option<&StepLogPipe>,
    ) -> Result<StepResult>;

    /// Execute a step and return the exit code (simple interface without process watching).
    async fn execute(&self, step: &StepSpec, workspace: &Path) -> Result<i32> {
        let mut watcher = ProcessWatcher::new();
        let result = self
            .execute_with_watcher(step, workspace, &mut watcher, None)
            .await?;
        Ok(result.exit_code)
    }

    /// Get the backend name.
    fn name(&self) -> &'static str;

    /// Check if the backend is available.
    async fn is_available(&self) -> bool;
}

/// Build an execution backend from config / explicit [`ExecutionRuntime`].
pub async fn create_execution_backend(runtime: ExecutionRuntime) -> Box<dyn ExecutionBackend> {
    match runtime {
        ExecutionRuntime::Native => Box::new(NativeBackend::new()),
        ExecutionRuntime::Container => container_backend_or_warn_and_native().await,
        ExecutionRuntime::Auto => auto_detect_backend().await,
    }
}

async fn auto_detect_backend() -> Box<dyn ExecutionBackend> {
    #[cfg(target_os = "linux")]
    {
        let container = ContainerBackend::new();
        if container.is_available().await {
            return Box::new(container);
        }
    }

    Box::new(NativeBackend::new())
}

async fn container_backend_or_warn_and_native() -> Box<dyn ExecutionBackend> {
    #[cfg(target_os = "linux")]
    {
        let container = ContainerBackend::new();
        if container.is_available().await {
            return Box::new(container);
        }
        warn!(
            "execution_runtime=container requested but no Docker/Podman-style CLI is available; \
             using native (host) execution"
        );
        return Box::new(NativeBackend::new());
    }
    #[cfg(not(target_os = "linux"))]
    {
        warn!(
            "execution_runtime=container is only supported on Linux; using native (host) execution"
        );
        Box::new(NativeBackend::new())
    }
}
