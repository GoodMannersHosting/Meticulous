//! Container execution backend for Linux.

use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tracing::{debug, info, warn};

use super::{ExecutionBackend, StepSpec};
use crate::error::{AgentError, Result};

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
                    .arg("--network=none") // Isolated by default
                    .arg("-w")
                    .arg("/workspace");

                // Mount workspace
                cmd.arg("-v")
                    .arg(format!("{}:/workspace", workspace.display()));

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
    async fn execute(&self, step: &StepSpec, workspace: &Path) -> Result<i32> {
        if step.image.is_empty() {
            return Err(AgentError::ContainerRuntime(
                "container image is required for container backend".to_string(),
            ));
        }

        let start = Instant::now();

        debug!(
            runtime = ?self.runtime,
            image = %step.image,
            command = %step.command,
            "executing step in container"
        );

        // Pull image first (ignore errors - it might already exist)
        let _ = self.pull_image(&step.image).await;

        // Build and run command
        let mut cmd = self.build_run_command(step, workspace);

        let mut child = cmd.spawn().map_err(|e| {
            AgentError::ContainerRuntime(format!("failed to spawn container: {e}"))
        })?;

        // Get stdout and stderr
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        // Spawn tasks to read output
        let stdout_handle = if let Some(stdout) = stdout {
            let step_name = step.name.clone();
            Some(tokio::spawn(async move {
                let reader = BufReader::new(stdout);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    info!(step = %step_name, "[stdout] {}", line);
                }
            }))
        } else {
            None
        };

        let stderr_handle = if let Some(stderr) = stderr {
            let step_name = step.name.clone();
            Some(tokio::spawn(async move {
                let reader = BufReader::new(stderr);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    warn!(step = %step_name, "[stderr] {}", line);
                }
            }))
        } else {
            None
        };

        // Wait for completion with timeout
        let result = if step.timeout.is_zero() {
            child.wait().await
        } else {
            match tokio::time::timeout(step.timeout, child.wait()).await {
                Ok(r) => r,
                Err(_) => {
                    // Kill the container on timeout
                    let _ = child.kill().await;
                    return Err(AgentError::Timeout(format!(
                        "step {} timed out after {:?}",
                        step.name, step.timeout
                    )));
                }
            }
        };

        // Wait for output tasks
        if let Some(h) = stdout_handle {
            let _ = h.await;
        }
        if let Some(h) = stderr_handle {
            let _ = h.await;
        }

        let status = result.map_err(|e| {
            AgentError::ContainerRuntime(format!("container wait failed: {e}"))
        })?;

        let exit_code = status.code().unwrap_or(-1);
        let duration = start.elapsed();

        info!(
            step = %step.name,
            exit_code,
            duration = ?duration,
            "container step completed"
        );

        Ok(exit_code)
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
