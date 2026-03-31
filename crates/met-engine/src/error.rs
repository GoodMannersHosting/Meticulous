//! Engine error types.

use met_core::ids::{JobId, JobRunId, RunId};
use thiserror::Error;

/// Engine result type.
pub type Result<T> = std::result::Result<T, EngineError>;

/// Errors that can occur during pipeline execution.
#[derive(Debug, Error)]
pub enum EngineError {
    #[error("run not found: {0}")]
    RunNotFound(RunId),

    #[error("job not found: {0}")]
    JobNotFound(JobId),

    #[error("job run not found: {0}")]
    JobRunNotFound(JobRunId),

    #[error("pipeline has no jobs")]
    EmptyPipeline,

    #[error("cycle detected in DAG")]
    CycleDetected,

    #[error("invalid DAG: {0}")]
    InvalidDag(String),

    #[error("condition evaluation failed for job {job}: {reason}")]
    ConditionEvaluation { job: String, reason: String },

    #[error("no available agents for job {job} with tags {tags:?}")]
    NoAvailableAgents { job: String, tags: Vec<String> },

    #[error("job {job} timed out after {timeout_secs}s")]
    JobTimeout { job: String, timeout_secs: u64 },

    #[error("run {run_id} was cancelled")]
    RunCancelled { run_id: RunId },

    #[error("secret resolution failed: {0}")]
    SecretResolution(String),

    #[error("cache operation failed: {0}")]
    Cache(String),

    #[error("artifact operation failed: {0}")]
    Artifact(String),

    #[error("NATS error: {0}")]
    Nats(String),

    #[error("database error: {0}")]
    Database(#[from] met_store::StoreError),

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("internal error: {0}")]
    Internal(String),
}

impl EngineError {
    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Internal(msg.into())
    }

    pub fn is_retriable(&self) -> bool {
        matches!(
            self,
            Self::NoAvailableAgents { .. }
                | Self::Nats(_)
                | Self::Database(_)
        )
    }
}
