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

    #[error("affinity scheduling failed for job {job}: {reason}")]
    AffinityScheduling { job: String, reason: String },

    #[error(
        "workspace snapshot from predecessor job {predecessor_job_id} is not available; producer may have failed to upload"
    )]
    WorkspaceSnapshotMissing { predecessor_job_id: JobId },

    #[error("job {job} timed out after {timeout_secs}s")]
    JobTimeout { job: String, timeout_secs: u64 },

    #[error("run {run_id} was cancelled")]
    RunCancelled { run_id: RunId },

    #[error("secret resolution failed: {0}")]
    SecretResolution(String),

    #[error("missing or unresolved secrets: {0:?}")]
    MissingSecrets(Vec<String>),

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

impl From<met_secret_resolve::ResolveError> for EngineError {
    fn from(e: met_secret_resolve::ResolveError) -> Self {
        match e {
            met_secret_resolve::ResolveError::MissingSecrets(names) => Self::MissingSecrets(names),
            met_secret_resolve::ResolveError::MissingProjectId => {
                Self::SecretResolution("pipeline is missing project_id".into())
            }
            met_secret_resolve::ResolveError::MissingMasterKey => {
                Self::SecretResolution("built-in secrets master key is not configured".into())
            }
            met_secret_resolve::ResolveError::ExternalNotConfigured(msg) => {
                Self::SecretResolution(format!("external secret provider not available: {msg}"))
            }
            other => Self::SecretResolution(other.to_string()),
        }
    }
}

impl EngineError {
    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Internal(msg.into())
    }

    pub fn is_retriable(&self) -> bool {
        matches!(
            self,
            Self::NoAvailableAgents { .. } | Self::Nats(_) | Self::Database(_)
        )
    }
}
