//! Step model (individual commands within a job).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::ids::{JobId, StepId};

/// A step is an individual command or action within a job.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct Step {
    /// Unique identifier.
    pub id: StepId,
    /// Owning job.
    pub job_id: JobId,
    /// Step name.
    pub name: String,
    /// Type of step.
    pub kind: StepKind,
    /// Command to execute (for Command kind).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    /// Working directory for command execution.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<String>,
    /// Shell to use (defaults to system shell).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shell: Option<String>,
    /// Workflow reference (for WorkflowRef kind).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_ref: Option<String>,
    /// Plugin identifier (for Plugin kind).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugin: Option<String>,
    /// Environment variables for this step.
    #[cfg_attr(feature = "sqlx", sqlx(json))]
    #[serde(default)]
    pub environment: JsonValue,
    /// Execution order within the job (0-indexed).
    pub sequence: i32,
    /// Whether to continue on failure.
    #[serde(default)]
    pub continue_on_error: bool,
    /// Timeout for this step in seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<i32>,
    /// When the step was created.
    pub created_at: DateTime<Utc>,
}

impl Step {
    /// Create a new command step.
    #[must_use]
    pub fn command(
        job_id: JobId,
        name: impl Into<String>,
        command: impl Into<String>,
        sequence: i32,
    ) -> Self {
        Self {
            id: StepId::new(),
            job_id,
            name: name.into(),
            kind: StepKind::Command,
            command: Some(command.into()),
            working_dir: None,
            shell: None,
            workflow_ref: None,
            plugin: None,
            environment: JsonValue::Object(serde_json::Map::new()),
            sequence,
            continue_on_error: false,
            timeout_secs: None,
            created_at: Utc::now(),
        }
    }

    /// Create a workflow reference step.
    #[must_use]
    pub fn workflow(
        job_id: JobId,
        name: impl Into<String>,
        workflow_ref: impl Into<String>,
        sequence: i32,
    ) -> Self {
        Self {
            id: StepId::new(),
            job_id,
            name: name.into(),
            kind: StepKind::WorkflowRef,
            command: None,
            working_dir: None,
            shell: None,
            workflow_ref: Some(workflow_ref.into()),
            plugin: None,
            environment: JsonValue::Object(serde_json::Map::new()),
            sequence,
            continue_on_error: false,
            timeout_secs: None,
            created_at: Utc::now(),
        }
    }
}

/// Type of step execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type))]
#[cfg_attr(
    feature = "sqlx",
    sqlx(type_name = "step_kind", rename_all = "snake_case")
)]
#[serde(rename_all = "snake_case")]
pub enum StepKind {
    /// Execute a shell command.
    #[default]
    Command,
    /// Execute a reusable workflow.
    WorkflowRef,
    /// Execute a plugin.
    Plugin,
}

/// Status of a step execution (reuses JobStatus).
pub type StepStatus = super::JobStatus;
