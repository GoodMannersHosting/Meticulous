//! Error types for the agent controller.

use met_store::StoreError;

/// Result type for controller operations.
pub type Result<T> = std::result::Result<T, ControllerError>;

/// Errors that can occur in the agent controller.
#[derive(Debug, thiserror::Error)]
pub enum ControllerError {
    /// Database error.
    #[error("database error: {0}")]
    Store(#[from] StoreError),

    /// Invalid join token.
    #[error("invalid join token")]
    InvalidJoinToken,

    /// Join token expired.
    #[error("join token expired")]
    JoinTokenExpired,

    /// Join token exhausted (max uses reached).
    #[error("join token exhausted")]
    JoinTokenExhausted,

    /// Join token revoked.
    #[error("join token revoked")]
    JoinTokenRevoked,

    /// Agent not found.
    #[error("agent not found: {0}")]
    AgentNotFound(String),

    /// Agent already registered.
    #[error("agent already registered: {0}")]
    AgentAlreadyRegistered(String),

    /// Agent revoked.
    #[error("agent revoked: {0}")]
    AgentRevoked(String),

    /// Invalid JWT token.
    #[error("invalid JWT: {0}")]
    InvalidJwt(String),

    /// JWT expired.
    #[error("JWT expired")]
    JwtExpired,

    /// Environment validation failed.
    #[error("environment validation failed: {0}")]
    ValidationFailed(String),

    /// NTP not synchronized.
    #[error("NTP not synchronized on agent")]
    NtpNotSynchronized,

    /// NATS error.
    #[error("NATS error: {0}")]
    Nats(String),

    /// gRPC transport error.
    #[error("gRPC error: {0}")]
    Grpc(#[from] tonic::transport::Error),

    /// Serialization error.
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// JWT encoding error.
    #[error("JWT encoding error: {0}")]
    JwtEncode(#[from] jsonwebtoken::errors::Error),

    /// Internal error.
    #[error("internal error: {0}")]
    Internal(String),
}

impl From<ControllerError> for tonic::Status {
    fn from(err: ControllerError) -> Self {
        match err {
            ControllerError::InvalidJoinToken
            | ControllerError::JoinTokenExpired
            | ControllerError::JoinTokenExhausted
            | ControllerError::JoinTokenRevoked => tonic::Status::unauthenticated(err.to_string()),
            ControllerError::AgentNotFound(_) => tonic::Status::not_found(err.to_string()),
            ControllerError::AgentAlreadyRegistered(_) => {
                tonic::Status::already_exists(err.to_string())
            }
            ControllerError::AgentRevoked(_) => tonic::Status::permission_denied(err.to_string()),
            ControllerError::InvalidJwt(_) | ControllerError::JwtExpired => {
                tonic::Status::unauthenticated(err.to_string())
            }
            ControllerError::ValidationFailed(_) | ControllerError::NtpNotSynchronized => {
                tonic::Status::failed_precondition(err.to_string())
            }
            _ => tonic::Status::internal(err.to_string()),
        }
    }
}
