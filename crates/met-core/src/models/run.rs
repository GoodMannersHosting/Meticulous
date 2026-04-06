//! Run models (pipeline execution records).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{AgentId, JobId, JobRunId, PipelineId, RunId, StepId, StepRunId, TriggerId};

use super::JobStatus;

/// A run is an instance of pipeline execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct Run {
    /// Unique identifier.
    pub id: RunId,
    /// The pipeline being executed.
    pub pipeline_id: PipelineId,
    /// When this run was created with **Retry** from another run; `None` for a fresh trigger (Run Pipeline, webhook, etc.).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_run_id: Option<RunId>,
    /// The trigger that initiated this run (if any).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_id: Option<TriggerId>,
    /// Current status.
    pub status: RunStatus,
    /// Run number within the pipeline (1, 2, 3...).
    pub run_number: i64,
    /// Git commit SHA (if applicable).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_sha: Option<String>,
    /// Git branch (if applicable).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// Who or what triggered the run.
    pub triggered_by: String,
    /// When the run was created.
    pub created_at: DateTime<Utc>,
    /// When execution started.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,
    /// When execution finished.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<DateTime<Utc>>,
}

impl Run {
    /// Create a new run in pending status.
    #[must_use]
    pub fn new(pipeline_id: PipelineId, run_number: i64, triggered_by: impl Into<String>) -> Self {
        Self {
            id: RunId::new(),
            pipeline_id,
            parent_run_id: None,
            trigger_id: None,
            status: RunStatus::Pending,
            run_number,
            commit_sha: None,
            branch: None,
            triggered_by: triggered_by.into(),
            created_at: Utc::now(),
            started_at: None,
            finished_at: None,
        }
    }

    /// Get the duration of the run (if finished).
    #[must_use]
    pub fn duration(&self) -> Option<chrono::Duration> {
        match (self.started_at, self.finished_at) {
            (Some(start), Some(end)) => Some(end - start),
            _ => None,
        }
    }
}

/// Status of a pipeline run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type))]
#[cfg_attr(
    feature = "sqlx",
    sqlx(type_name = "run_status", rename_all = "snake_case")
)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    /// Waiting to be scheduled.
    #[default]
    Pending,
    /// In the execution queue.
    Queued,
    /// Currently executing.
    Running,
    /// Completed successfully.
    Succeeded,
    /// Failed to complete.
    Failed,
    /// Cancelled by user or system.
    Cancelled,
    /// Exceeded timeout.
    TimedOut,
}

impl RunStatus {
    /// Check if this is a terminal status.
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Failed | Self::Cancelled | Self::TimedOut
        )
    }

    /// Check if this status indicates success.
    #[must_use]
    pub const fn is_success(&self) -> bool {
        matches!(self, Self::Succeeded)
    }
}

/// A job run is an instance of job execution within a run.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct JobRun {
    /// Unique identifier.
    pub id: JobRunId,
    /// Parent run.
    pub run_id: RunId,
    /// The job being executed.
    pub job_id: JobId,
    /// Job name for display.
    pub job_name: String,
    /// The agent executing the job (if assigned).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<AgentId>,
    /// Current status.
    pub status: JobStatus,
    /// Retry attempt number (0 for first attempt).
    #[serde(default)]
    pub attempt: i32,
    /// Exit code from job execution.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    /// Error message if job failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    /// Whether the job was restored from cache.
    #[serde(default)]
    pub cache_hit: bool,
    /// Path to log storage.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_path: Option<String>,
    /// Cache key when restored from cache.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "sqlx", sqlx(default))]
    pub cache_key: Option<String>,
    /// Job outputs JSON (when recorded).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "sqlx", sqlx(json, default))]
    pub outputs: Option<serde_json::Value>,
    /// SHA-256 of the pipeline definition JSON snapshot (`definition_snapshots`) at job_run creation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pipeline_definition_sha256: Option<Vec<u8>>,
    /// SHA-256 of the reusable workflow definition JSON when this job was expanded from one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_definition_sha256: Option<Vec<u8>>,
    /// Resolved reusable workflow reference (`scope`, `name`, `version`) when applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_workflow: Option<serde_json::Value>,
    /// When execution started.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,
    /// When execution finished.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<DateTime<Utc>>,
    /// When the record was created.
    pub created_at: DateTime<Utc>,
    /// Agent identity and host/security fields captured when this job entered `running` on an agent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_snapshot: Option<serde_json::Value>,
    /// When [`Self::agent_snapshot`] was recorded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_snapshot_captured_at: Option<DateTime<Utc>>,
}

impl JobRun {
    /// Create a new job run in pending status.
    #[must_use]
    pub fn new(run_id: RunId, job_id: JobId, job_name: impl Into<String>) -> Self {
        Self {
            id: JobRunId::new(),
            run_id,
            job_id,
            job_name: job_name.into(),
            agent_id: None,
            status: JobStatus::Pending,
            attempt: 0,
            exit_code: None,
            error_message: None,
            cache_hit: false,
            log_path: None,
            cache_key: None,
            outputs: None,
            pipeline_definition_sha256: None,
            workflow_definition_sha256: None,
            source_workflow: None,
            started_at: None,
            finished_at: None,
            created_at: Utc::now(),
            agent_snapshot: None,
            agent_snapshot_captured_at: None,
        }
    }

    /// Get the duration of the job run.
    #[must_use]
    pub fn duration(&self) -> Option<chrono::Duration> {
        match (self.started_at, self.finished_at) {
            (Some(start), Some(end)) => Some(end - start),
            _ => None,
        }
    }
}

/// A step run is an instance of step execution within a job run.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct StepRun {
    /// Unique identifier.
    pub id: StepRunId,
    /// Parent job run.
    pub job_run_id: JobRunId,
    /// The step being executed.
    pub step_id: StepId,
    /// Step name for display.
    pub step_name: String,
    /// Current status.
    pub status: JobStatus,
    /// Exit code (if applicable).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    /// Error message if step failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    /// Path to log storage.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_path: Option<String>,
    /// When execution started.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,
    /// When execution finished.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<DateTime<Utc>>,
    /// When the record was created.
    pub created_at: DateTime<Utc>,
}

impl StepRun {
    /// Create a new step run in pending status.
    #[must_use]
    pub fn new(job_run_id: JobRunId, step_id: StepId, step_name: impl Into<String>) -> Self {
        Self {
            id: StepRunId::new(),
            job_run_id,
            step_id,
            step_name: step_name.into(),
            status: JobStatus::Pending,
            exit_code: None,
            error_message: None,
            log_path: None,
            started_at: None,
            finished_at: None,
            created_at: Utc::now(),
        }
    }

    /// Get the duration of the step run.
    #[must_use]
    pub fn duration(&self) -> Option<chrono::Duration> {
        match (self.started_at, self.finished_at) {
            (Some(start), Some(end)) => Some(end - start),
            _ => None,
        }
    }
}
