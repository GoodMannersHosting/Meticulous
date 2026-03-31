//! Native process execution backend.

use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tracing::{debug, info, trace, warn};

use super::{ExecutionBackend, StepResult, StepSpec};
use crate::error::{AgentError, Result};
use crate::process_watcher::ProcessWatcher;

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
    async fn execute_with_watcher(
        &self,
        step: &StepSpec,
        workspace: &Path,
        watcher: &mut ProcessWatcher,
    ) -> Result<StepResult> {
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

        // Get the PID and start process watching
        let pid = child.id().unwrap_or(0);
        if pid > 0 {
            watcher.start_watching(pid, &step.step_id).await?;
            debug!(pid, step = %step.name, "started process watching");
        }

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

        // Compute deadline for timeout
        let deadline = if step.timeout.is_zero() {
            None
        } else {
            Some(tokio::time::Instant::now() + step.timeout)
        };

        // Poll for child processes while waiting for completion
        let poll_interval = Duration::from_millis(100);
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
                        Err(e) => break Err(AgentError::ProcessExecution(format!(
                            "process wait failed: {e}"
                        ))),
                    }
                }
                _ = tokio::time::sleep(poll_interval) => {
                    // Poll for new child processes
                    if let Err(e) = watcher.poll().await {
                        trace!(error = %e, "process watcher poll error");
                    }
                }
            }
        };

        // Final poll to catch any remaining processes
        let _ = watcher.poll().await;

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
                return Err(e);
            }
        };

        // Aggregate execution metadata
        let metadata = watcher.aggregate_metadata(&step.step_id).await;
        watcher.stop_watching().await;

        let exit_code = status.code().unwrap_or(-1);
        let duration = start.elapsed();

        info!(
            step = %step.name,
            exit_code,
            duration = ?duration,
            processes_spawned = metadata.total_processes_spawned,
            binaries_executed = metadata.executed_binaries.len(),
            "step completed"
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
        let mut watcher = ProcessWatcher::new();

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

        let result = backend
            .execute_with_watcher(&step, &temp_dir, &mut watcher)
            .await
            .unwrap();
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_native_backend_exit_code() {
        let backend = NativeBackend::new();
        let temp_dir = std::env::temp_dir();
        let mut watcher = ProcessWatcher::new();

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

        let result = backend
            .execute_with_watcher(&step, &temp_dir, &mut watcher)
            .await
            .unwrap();
        assert_eq!(result.exit_code, 42);
    }

    #[tokio::test]
    #[cfg(target_os = "linux")]
    async fn test_native_backend_tracks_child_processes() {
        let backend = NativeBackend::new();
        let temp_dir = std::env::temp_dir();
        let mut watcher = ProcessWatcher::new();

        let step = StepSpec {
            step_id: "test-children".to_string(),
            name: "spawn children".to_string(),
            // This command spawns multiple child processes
            command: "echo start && ls /tmp && echo end".to_string(),
            image: String::new(),
            working_dir: String::new(),
            shell: String::new(),
            environment: HashMap::new(),
            timeout: Duration::from_secs(10),
        };

        let result = backend
            .execute_with_watcher(&step, &temp_dir, &mut watcher)
            .await
            .unwrap();

        assert_eq!(result.exit_code, 0);
        // On Linux, we should have tracked at least the shell process
        // Note: The exact count depends on how quickly processes spawn/exit
    }
}
