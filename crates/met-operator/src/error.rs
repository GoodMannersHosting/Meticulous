//! Error types for the Kubernetes operator.

/// Result type for operator operations.
pub type Result<T> = std::result::Result<T, OperatorError>;

/// Errors that can occur in the operator.
#[derive(Debug, thiserror::Error)]
pub enum OperatorError {
    /// Kubernetes API error.
    #[error("kubernetes error: {0}")]
    Kube(#[from] kube::Error),

    /// Resource not found.
    #[error("resource not found: {0}")]
    NotFound(String),

    /// Invalid configuration.
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),

    /// Reconciliation error.
    #[error("reconciliation error: {0}")]
    Reconciliation(String),

    /// NATS error.
    #[error("NATS error: {0}")]
    Nats(#[from] async_nats::Error),

    /// Serialization error.
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Internal error.
    #[error("internal error: {0}")]
    Internal(String),
}
