//! Job assignment models for tracking agent-job relationships.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{AgentId, JobAssignmentId, JobRunId};

/// Status of a job assignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type))]
#[cfg_attr(
    feature = "sqlx",
    sqlx(type_name = "job_assignment_status", rename_all = "snake_case")
)]
#[serde(rename_all = "snake_case")]
pub enum JobAssignmentStatus {
    /// Job has been accepted by the agent.
    #[default]
    Accepted,
    /// Job is currently running.
    Running,
    /// Job completed successfully.
    Succeeded,
    /// Job failed.
    Failed,
    /// Job was cancelled.
    Cancelled,
    /// Job timed out.
    TimedOut,
}

impl JobAssignmentStatus {
    /// Check if the assignment is in a terminal state.
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Failed | Self::Cancelled | Self::TimedOut
        )
    }
}

/// A job assignment mapping an agent to a job run.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct JobAssignment {
    /// Unique identifier.
    pub id: JobAssignmentId,
    /// The job run being executed.
    pub job_run_id: JobRunId,
    /// The agent executing the job.
    pub agent_id: AgentId,
    /// Current status of the assignment.
    pub status: JobAssignmentStatus,
    /// When the agent accepted the job.
    pub accepted_at: DateTime<Utc>,
    /// When the agent started executing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,
    /// When execution completed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
    /// Exit code (if applicable).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    /// Failure reason (if failed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_reason: Option<String>,
    /// Retry attempt number.
    #[serde(default = "default_attempt")]
    pub attempt: i32,
}

fn default_attempt() -> i32 {
    1
}

impl JobAssignment {
    /// Create a new job assignment.
    #[must_use]
    pub fn new(job_run_id: JobRunId, agent_id: AgentId) -> Self {
        Self {
            id: JobAssignmentId::new(),
            job_run_id,
            agent_id,
            status: JobAssignmentStatus::Accepted,
            accepted_at: Utc::now(),
            started_at: None,
            completed_at: None,
            exit_code: None,
            failure_reason: None,
            attempt: 1,
        }
    }

    /// Create a new assignment for a retry attempt.
    #[must_use]
    pub fn new_retry(job_run_id: JobRunId, agent_id: AgentId, attempt: i32) -> Self {
        Self {
            id: JobAssignmentId::new(),
            job_run_id,
            agent_id,
            status: JobAssignmentStatus::Accepted,
            accepted_at: Utc::now(),
            started_at: None,
            completed_at: None,
            exit_code: None,
            failure_reason: None,
            attempt,
        }
    }

    /// Mark the assignment as started.
    pub fn mark_started(&mut self) {
        self.status = JobAssignmentStatus::Running;
        self.started_at = Some(Utc::now());
    }

    /// Mark the assignment as succeeded.
    pub fn mark_succeeded(&mut self, exit_code: i32) {
        self.status = JobAssignmentStatus::Succeeded;
        self.completed_at = Some(Utc::now());
        self.exit_code = Some(exit_code);
    }

    /// Mark the assignment as failed.
    pub fn mark_failed(&mut self, exit_code: Option<i32>, reason: impl Into<String>) {
        self.status = JobAssignmentStatus::Failed;
        self.completed_at = Some(Utc::now());
        self.exit_code = exit_code;
        self.failure_reason = Some(reason.into());
    }

    /// Mark the assignment as cancelled.
    pub fn mark_cancelled(&mut self, reason: impl Into<String>) {
        self.status = JobAssignmentStatus::Cancelled;
        self.completed_at = Some(Utc::now());
        self.failure_reason = Some(reason.into());
    }

    /// Mark the assignment as timed out.
    pub fn mark_timed_out(&mut self) {
        self.status = JobAssignmentStatus::TimedOut;
        self.completed_at = Some(Utc::now());
        self.failure_reason = Some("Job execution timed out".to_string());
    }
}

/// Agent heartbeat record for diagnostics.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct AgentHeartbeat {
    /// Unique identifier.
    pub id: crate::ids::AgentHeartbeatId,
    /// Agent that sent this heartbeat.
    pub agent_id: AgentId,
    /// Agent status at the time.
    pub status: super::AgentStatus,
    /// CPU utilization (0.0 to 1.0).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_percent: Option<f32>,
    /// Memory utilization (0.0 to 1.0).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_percent: Option<f32>,
    /// Disk utilization (0.0 to 1.0).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disk_percent: Option<f32>,
    /// Current job being executed (if any).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_job_id: Option<uuid::Uuid>,
    /// When this heartbeat was recorded.
    pub recorded_at: DateTime<Utc>,
}

impl AgentHeartbeat {
    /// Create a new heartbeat record.
    #[must_use]
    pub fn new(agent_id: AgentId, status: super::AgentStatus) -> Self {
        Self {
            id: crate::ids::AgentHeartbeatId::new(),
            agent_id,
            status,
            cpu_percent: None,
            memory_percent: None,
            disk_percent: None,
            current_job_id: None,
            recorded_at: Utc::now(),
        }
    }

    /// Set resource utilization.
    pub fn with_resources(mut self, cpu: f32, memory: f32, disk: f32) -> Self {
        self.cpu_percent = Some(cpu);
        self.memory_percent = Some(memory);
        self.disk_percent = Some(disk);
        self
    }

    /// Set current job.
    pub fn with_current_job(mut self, job_id: uuid::Uuid) -> Self {
        self.current_job_id = Some(job_id);
        self
    }
}
