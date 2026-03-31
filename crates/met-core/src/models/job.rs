//! Job model (DAG nodes within a pipeline).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::ids::{JobId, PipelineId};

/// A job is a unit of execution within a pipeline, forming a DAG.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct Job {
    /// Unique identifier.
    pub id: JobId,
    /// Owning pipeline.
    pub pipeline_id: PipelineId,
    /// Job name (unique within pipeline).
    pub name: String,
    /// Jobs that must complete before this one can run.
    pub depends_on: Vec<String>,
    /// Tags for agent selection (e.g., ["linux", "docker"]).
    pub agent_tags: Vec<String>,
    /// Maximum execution time in seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<i32>,
    /// Number of retry attempts on failure.
    #[serde(default)]
    pub retry_count: i32,
    /// Conditional expression (CEL) for when to run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    /// Additional configuration (JSON).
    #[cfg_attr(feature = "sqlx", sqlx(json))]
    #[serde(default)]
    pub config: JsonValue,
    /// When the job definition was created.
    pub created_at: DateTime<Utc>,
}

impl Job {
    /// Create a new job with default values.
    #[must_use]
    pub fn new(pipeline_id: PipelineId, name: impl Into<String>) -> Self {
        Self {
            id: JobId::new(),
            pipeline_id,
            name: name.into(),
            depends_on: Vec::new(),
            agent_tags: Vec::new(),
            timeout_secs: None,
            retry_count: 0,
            condition: None,
            config: JsonValue::Null,
            created_at: Utc::now(),
        }
    }

    /// Check if this job has no dependencies (can start immediately).
    #[must_use]
    pub fn is_root(&self) -> bool {
        self.depends_on.is_empty()
    }
}

/// Status of a job execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type))]
#[cfg_attr(
    feature = "sqlx",
    sqlx(type_name = "run_status", rename_all = "snake_case")
)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    /// Waiting to be scheduled.
    #[default]
    Pending,
    /// In the dispatch queue.
    Queued,
    /// Currently executing on an agent.
    Running,
    /// Completed successfully.
    Succeeded,
    /// Failed to complete.
    Failed,
    /// Cancelled by user or system.
    Cancelled,
    /// Exceeded timeout.
    TimedOut,
    /// Skipped due to condition or dependency failure.
    Skipped,
}

impl JobStatus {
    /// Check if this is a terminal status (no further transitions).
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Failed | Self::Cancelled | Self::TimedOut | Self::Skipped
        )
    }

    /// Check if this status indicates success.
    #[must_use]
    pub const fn is_success(&self) -> bool {
        matches!(self, Self::Succeeded | Self::Skipped)
    }
}
