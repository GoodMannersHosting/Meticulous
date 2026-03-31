//! Execution backends for running steps.

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use async_trait::async_trait;

use crate::error::Result;

#[cfg(target_os = "linux")]
mod container;
mod native;

#[cfg(target_os = "linux")]
pub use container::ContainerBackend;
pub use native::NativeBackend;

/// Specification for a step to execute.
#[derive(Debug, Clone)]
pub struct StepSpec {
    pub step_id: String,
    pub name: String,
    pub command: String,
    pub image: String,
    pub working_dir: String,
    pub shell: String,
    pub environment: HashMap<String, String>,
    pub timeout: Duration,
}

/// Result of step execution.
#[derive(Debug)]
pub struct StepResult {
    pub exit_code: i32,
    pub duration: Duration,
}

/// Trait for step execution backends.
#[async_trait]
pub trait ExecutionBackend: Send + Sync {
    /// Execute a step and return the exit code.
    async fn execute(&self, step: &StepSpec, workspace: &Path) -> Result<i32>;

    /// Get the backend name.
    fn name(&self) -> &'static str;

    /// Check if the backend is available.
    async fn is_available(&self) -> bool;
}

/// Create the default execution backend for the current platform.
pub fn default_backend() -> Box<dyn ExecutionBackend> {
    #[cfg(target_os = "linux")]
    {
        // Try container backend first
        let container = ContainerBackend::new();
        if futures::executor::block_on(container.is_available()) {
            return Box::new(container);
        }
    }

    // Fall back to native backend
    Box::new(NativeBackend::new())
}
