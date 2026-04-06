//! Container execution backend for Linux.

use std::collections::HashSet;
use std::path::Path;
use std::process::Stdio;
use std::time::Instant;

use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tracing::{debug, info, warn};

use super::{poll_watcher_emit_telemetry, ExecutionBackend, StepResult, StepSpec};
use crate::error::{AgentError, Result};
use crate::process_watcher::ProcessWatcher;
use crate::step_log::StepLogPipe;

/// Container runtime type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerRuntime {
    Docker,
    Podman,
    Containerd,
}

impl ContainerRuntime {
    /// Get the command name for this runtime.
    fn command(&self) -> &'static str {
        match self {
            Self::Docker => "docker",
            Self::Podman => "podman",
            Self::Containerd => "ctr",
        }
    }
}

/// Container execution backend for Linux.
pub struct ContainerBackend {
    runtime: ContainerRuntime,
}

impl ContainerBackend {
    /// Create a new container backend.
    pub fn new() -> Self {
        // Detect available runtime
        let runtime = Self::detect_runtime();
        Self { runtime }
    }

    /// Create a backend with a specific runtime.
    pub fn with_runtime(runtime: ContainerRuntime) -> Self {
        Self { runtime }
    }

    /// Detect available container runtime.
    fn detect_runtime() -> ContainerRuntime {
        // Check for Docker
        if std::process::Command::new("docker")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            return ContainerRuntime::Docker;
        }

        // Check for Podman
        if std::process::Command::new("podman")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            return ContainerRuntime::Podman;
        }

        // Check for containerd
        if std::process::Command::new("ctr")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            return ContainerRuntime::Containerd;
        }

        // Default to Docker
        ContainerRuntime::Docker
    }

    /// Build the docker/podman run command.
    fn build_run_command(&self, step: &StepSpec, workspace: &Path) -> Command {
        let mut cmd = Command::new(self.runtime.command());

        match self.runtime {
            ContainerRuntime::Docker | ContainerRuntime::Podman => {
                cmd.arg("run")
                    .arg("--rm")
                    // Default bridge network (outbound NAT) — required for git clone, curl, etc.
                    // Opt-in isolation can be added later via pipeline/step settings.
                    .arg("-w")
                    .arg("/workspace");

                // Mount workspace
                cmd.arg("-v")
                    .arg(format!("{}:/workspace", workspace.display()));

                // Non-interactive git/SSH in CI (avoid credential-helper prompts blocking forever).
                cmd.arg("-e").arg("GIT_TERMINAL_PROMPT=0");

                // Set environment variables
                for (key, value) in &step.environment {
                    cmd.arg("-e").arg(format!("{}={}", key, value));
                }

                // Set working directory override
                if !step.working_dir.is_empty() {
                    cmd.arg("-w").arg(&step.working_dir);
                }

                // Image
                cmd.arg(&step.image);

                // Shell and command
                let shell = if step.shell.is_empty() {
                    "/bin/sh"
                } else {
                    &step.shell
                };
                cmd.args([shell, "-c", &step.command]);
            }
            ContainerRuntime::Containerd => {
                // containerd via ctr has different syntax
                cmd.arg("run")
                    .arg("--rm")
                    .arg("--mount")
                    .arg(format!(
                        "type=bind,src={},dst=/workspace,options=rbind:rw",
                        workspace.display()
                    ))
                    .arg(&step.image)
                    .arg(format!("step-{}", step.step_id));

                let shell = if step.shell.is_empty() {
                    "/bin/sh"
                } else {
                    &step.shell
                };
                cmd.args([shell, "-c", &step.command]);
            }
        }

        cmd.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        cmd
    }
}

impl Default for ContainerBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ExecutionBackend for ContainerBackend {
    async fn execute_with_watcher(
        &self,
        step: &StepSpec,
        workspace: &Path,
        watcher: &mut ProcessWatcher,
        logs: Option<&StepLogPipe>,
    ) -> Result<StepResult> {
        if step.image.is_empty() {
            return Err(AgentError::ContainerRuntime(
                "container image is required for container backend".to_string(),
            ));
        }

        let start = Instant::now();

        let workspace_canon = tokio::fs::canonicalize(workspace)
            .await
            .unwrap_or_else(|_| workspace.to_path_buf());
        let mut runtime_budget = crate::telemetry::MAX_RUNTIME_SCRIPT_BYTES_PER_STEP;
        let mut runtime_seen = HashSet::new();

        info!(
            runtime = ?self.runtime,
            step = %step.name,
            image = %step.image,
            "container backend: begin (pull if needed, then run)"
        );

        let pull_start = Instant::now();
        let pull_res = self.pull_image(&step.image).await;
        info!(
            step = %step.name,
            image = %step.image,
            elapsed = ?pull_start.elapsed(),
            pull_ok = pull_res.is_ok(),
            "container backend: image pull attempt finished"
        );


        // Build and run command
        let mut cmd = self.build_run_command(step, workspace);

        let mut child = cmd.spawn().map_err(|e| {
            AgentError::ContainerRuntime(format!("failed to spawn container: {e}"))
        })?;

        // Get the PID and start process watching
        // Note: For containers, we watch the container runtime process and its children
        let pid = child.id().unwrap_or(0);
        if pid > 0 {
            watcher.start_watching(pid, &step.step_id).await?;
            debug!(pid, step = %step.name, "started process watching for container");
        }

        // Get stdout and stderr
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        let stdout_handle = if let Some(stdout) = stdout {
            if let Some(pipe) = logs.cloned() {
                Some(tokio::spawn(async move {
                    let reader = BufReader::new(stdout);
                    let mut lines = reader.lines();
                    while let Ok(Some(line)) = lines.next_line().await {
                        if pipe.send_stdout_line(&line).await.is_err() {
                            break;
                        }
                    }
                }))
            } else {
                Some(tokio::spawn(async move {
                    let reader = BufReader::new(stdout);
                    let mut lines = reader.lines();
                    while let Ok(Some(_line)) = lines.next_line().await {}
                }))
            }
        } else {
            None
        };

        let stderr_handle = if let Some(stderr) = stderr {
            if let Some(pipe) = logs.cloned() {
                Some(tokio::spawn(async move {
                    let reader = BufReader::new(stderr);
                    let mut lines = reader.lines();
                    while let Ok(Some(line)) = lines.next_line().await {
                        if pipe.send_stderr_line(&line).await.is_err() {
                            break;
                        }
                    }
                }))
            } else {
                Some(tokio::spawn(async move {
                    let reader = BufReader::new(stderr);
                    let mut lines = reader.lines();
                    while let Ok(Some(_line)) = lines.next_line().await {}
                }))
            }
        } else {
            None
        };

        // Compute deadline for timeout
        let deadline = if step.timeout.is_zero() {
            None
        } else {
            Some(tokio::time::Instant::now() + step.timeout)
        };

        // Poll for child processes while waiting for completion
        let poll_interval = super::PROCESS_WATCHER_POLL_INTERVAL;
        let result: std::result::Result<std::process::ExitStatus, AgentError> = loop {
            // Check if we've exceeded the timeout
            if let Some(deadline) = deadline {
                if tokio::time::Instant::now() >= deadline {
                    // Kill the child process on timeout
                    let _ = child.kill().await;
                    watcher.stop_watching().await;
                    return Err(AgentError::Timeout(format!(
                        "step {} timed out after {:?}",
                        step.name, step.timeout
                    )));
                }
            }

            tokio::select! {
                status = child.wait() => {
                    match status {
                        Ok(s) => break Ok(s),
                        Err(e) => break Err(AgentError::ContainerRuntime(format!(
                            "container wait failed: {e}"
                        ))),
                    }
                }
                _ = tokio::time::sleep(poll_interval) => {
                    poll_watcher_emit_telemetry(
                        watcher,
                        logs,
                        step,
                        &workspace_canon,
                        &mut runtime_budget,
                        &mut runtime_seen,
                    )
                    .await?;
                }
            }
        };

        // Final poll to catch any remaining processes
        poll_watcher_emit_telemetry(
            watcher,
            logs,
            step,
            &workspace_canon,
            &mut runtime_budget,
            &mut runtime_seen,
        )
        .await?;

        // Wait for output tasks
        if let Some(h) = stdout_handle {
            let _ = h.await;
        }
        if let Some(h) = stderr_handle {
            let _ = h.await;
        }

        // Handle error case
        let status = match result {
            Ok(s) => s,
            Err(e) => {
                watcher.stop_watching().await;
                // Kill the container on error
                let _ = child.kill().await;
                return Err(e);
            }
        };

        // Aggregate execution metadata
        let metadata = watcher
            .aggregate_metadata(&step.step_id, &step.step_run_id)
            .await;
        watcher.stop_watching().await;

        let exit_code = status.code().unwrap_or(-1);
        let duration = start.elapsed();

        info!(
            step = %step.name,
            exit_code,
            duration = ?duration,
            processes_spawned = metadata.total_processes_spawned,
            binaries_executed = metadata.executed_binaries.len(),
            "container step completed"
        );

        Ok(StepResult {
            exit_code,
            duration,
            executed_binaries: metadata.executed_binaries,
            processes_spawned: metadata.total_processes_spawned,
            execution_tree_depth: metadata.execution_tree_depth,
        })
    }

    fn name(&self) -> &'static str {
        match self.runtime {
            ContainerRuntime::Docker => "docker",
            ContainerRuntime::Podman => "podman",
            ContainerRuntime::Containerd => "containerd",
        }
    }

    async fn is_available(&self) -> bool {
        Command::new(self.runtime.command())
            .arg("--version")
            .output()
            .await
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}

impl ContainerBackend {
    /// Pull an image if needed.
    async fn pull_image(&self, image: &str) -> Result<()> {
        let mut cmd = Command::new(self.runtime.command());

        match self.runtime {
            ContainerRuntime::Docker | ContainerRuntime::Podman => {
                cmd.arg("pull").arg(image);
            }
            ContainerRuntime::Containerd => {
                cmd.arg("image").arg("pull").arg(image);
            }
        }

        let output = cmd.output().await.map_err(|e| {
            AgentError::ContainerRuntime(format!("failed to pull image: {e}"))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!(image = %image, error = %stderr, "image pull failed (may already exist)");
        }

        Ok(())
    }
}
