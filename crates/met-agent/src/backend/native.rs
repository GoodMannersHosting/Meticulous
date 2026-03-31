//! Native process execution backend.

use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tracing::{debug, info, warn};

use super::{ExecutionBackend, StepSpec};
use crate::error::{AgentError, Result};

/// Native process execution backend for macOS and Windows.
pub struct NativeBackend {
    /// Default shell to use.
    default_shell: String,
}

impl NativeBackend {
    /// Create a new native backend.
    pub fn new() -> Self {
        let default_shell = if cfg!(windows) {
            "cmd".to_string()
        } else {
            "/bin/sh".to_string()
        };

        Self { default_shell }
    }

    /// Get the shell and arguments for executing a command.
    fn get_shell_command(&self, step: &StepSpec) -> (String, Vec<String>) {
        let shell = if step.shell.is_empty() {
            &self.default_shell
        } else {
            &step.shell
        };

        if cfg!(windows) {
            if shell == "cmd" || shell.ends_with("cmd.exe") {
                (shell.to_string(), vec!["/C".to_string(), step.command.clone()])
            } else if shell == "powershell" || shell.ends_with("powershell.exe") {
                (
                    shell.to_string(),
                    vec!["-Command".to_string(), step.command.clone()],
                )
            } else {
                (shell.to_string(), vec!["-c".to_string(), step.command.clone()])
            }
        } else {
            (shell.to_string(), vec!["-c".to_string(), step.command.clone()])
        }
    }
}

impl Default for NativeBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ExecutionBackend for NativeBackend {
    async fn execute(&self, step: &StepSpec, workspace: &Path) -> Result<i32> {
        let start = Instant::now();

        let (shell, args) = self.get_shell_command(step);

        debug!(
            shell = %shell,
            command = %step.command,
            working_dir = %workspace.display(),
            "executing step"
        );

        // Build command
        let mut command = Command::new(&shell);
        command
            .args(&args)
            .current_dir(workspace)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        // Set working directory override if specified
        if !step.working_dir.is_empty() {
            let work_dir = if Path::new(&step.working_dir).is_absolute() {
                Path::new(&step.working_dir).to_path_buf()
            } else {
                workspace.join(&step.working_dir)
            };
            command.current_dir(work_dir);
        }

        // Set environment - only pass explicitly declared variables
        command.env_clear();

        // Add minimal required environment
        #[cfg(unix)]
        {
            command.env("PATH", std::env::var("PATH").unwrap_or_default());
            command.env("HOME", std::env::var("HOME").unwrap_or_default());
            command.env("USER", std::env::var("USER").unwrap_or_default());
        }
        #[cfg(windows)]
        {
            command.env("PATH", std::env::var("PATH").unwrap_or_default());
            command.env("SYSTEMROOT", std::env::var("SYSTEMROOT").unwrap_or_default());
            command.env("TEMP", std::env::var("TEMP").unwrap_or_default());
        }

        // Add step-specific environment
        for (key, value) in &step.environment {
            command.env(key, value);
        }

        // Spawn the process
        let mut child = command.spawn().map_err(|e| {
            AgentError::ProcessExecution(format!("failed to spawn process: {e}"))
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
            tokio::time::timeout(step.timeout, child.wait())
                .await
                .map_err(|_| {
                    AgentError::Timeout(format!("step {} timed out after {:?}", step.name, step.timeout))
                })?
        };

        // Wait for output tasks
        if let Some(h) = stdout_handle {
            let _ = h.await;
        }
        if let Some(h) = stderr_handle {
            let _ = h.await;
        }

        let status = result.map_err(|e| {
            AgentError::ProcessExecution(format!("process wait failed: {e}"))
        })?;

        let exit_code = status.code().unwrap_or(-1);
        let duration = start.elapsed();

        info!(
            step = %step.name,
            exit_code,
            duration = ?duration,
            "step completed"
        );

        Ok(exit_code)
    }

    fn name(&self) -> &'static str {
        "native"
    }

    async fn is_available(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_native_backend_echo() {
        let backend = NativeBackend::new();
        let temp_dir = std::env::temp_dir();

        let step = StepSpec {
            step_id: "test".to_string(),
            name: "echo test".to_string(),
            command: "echo hello".to_string(),
            image: String::new(),
            working_dir: String::new(),
            shell: String::new(),
            environment: HashMap::new(),
            timeout: Duration::from_secs(10),
        };

        let exit_code = backend.execute(&step, &temp_dir).await.unwrap();
        assert_eq!(exit_code, 0);
    }

    #[tokio::test]
    async fn test_native_backend_exit_code() {
        let backend = NativeBackend::new();
        let temp_dir = std::env::temp_dir();

        let step = StepSpec {
            step_id: "test".to_string(),
            name: "exit 42".to_string(),
            command: if cfg!(windows) {
                "exit /b 42".to_string()
            } else {
                "exit 42".to_string()
            },
            image: String::new(),
            working_dir: String::new(),
            shell: String::new(),
            environment: HashMap::new(),
            timeout: Duration::from_secs(10),
        };

        let exit_code = backend.execute(&step, &temp_dir).await.unwrap();
        assert_eq!(exit_code, 42);
    }
}
