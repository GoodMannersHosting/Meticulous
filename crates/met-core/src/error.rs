//! Unified error types for Meticulous.

use std::fmt;

/// The primary error type for Meticulous operations.
#[derive(Debug, thiserror::Error)]
pub enum MetError {
    /// Database operation failed.
    #[error("database error: {0}")]
    #[cfg(feature = "sqlx")]
    Database(#[from] sqlx::Error),

    /// JSON serialization/deserialization failed.
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// YAML parsing failed.
    #[error("yaml error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    /// Configuration loading failed.
    #[error("configuration error: {0}")]
    Config(String),

    /// Requested entity not found.
    #[error("not found: {entity} with id {id}")]
    NotFound {
        /// The type of entity that was not found.
        entity: &'static str,
        /// The ID that was looked up.
        id: String,
    },

    /// Authentication failed or missing.
    #[error("unauthorized: {0}")]
    Unauthorized(String),

    /// Authenticated but not permitted.
    #[error("forbidden: {0}")]
    Forbidden(String),

    /// Input validation failed.
    #[error("validation error: {0}")]
    Validation(String),

    /// Internal/unexpected error.
    #[error("internal error: {0}")]
    Internal(String),

    /// I/O operation failed.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// UUID parsing failed.
    #[error("uuid parse error: {0}")]
    UuidParse(#[from] uuid::Error),
}

impl MetError {
    /// Create a not found error for a specific entity type and ID.
    #[must_use]
    pub fn not_found(entity: &'static str, id: impl fmt::Display) -> Self {
        Self::NotFound {
            entity,
            id: id.to_string(),
        }
    }

    /// Create a configuration error.
    #[must_use]
    pub fn config(msg: impl Into<String>) -> Self {
        Self::Config(msg.into())
    }

    /// Create a validation error.
    #[must_use]
    pub fn validation(msg: impl Into<String>) -> Self {
        Self::Validation(msg.into())
    }

    /// Create an internal error.
    #[must_use]
    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Internal(msg.into())
    }

    /// Create an unauthorized error.
    #[must_use]
    pub fn unauthorized(msg: impl Into<String>) -> Self {
        Self::Unauthorized(msg.into())
    }

    /// Create a forbidden error.
    #[must_use]
    pub fn forbidden(msg: impl Into<String>) -> Self {
        Self::Forbidden(msg.into())
    }

    /// Returns true if this is a not-found error.
    #[must_use]
    pub const fn is_not_found(&self) -> bool {
        matches!(self, Self::NotFound { .. })
    }

    /// Returns true if this is an auth-related error.
    #[must_use]
    pub const fn is_auth_error(&self) -> bool {
        matches!(self, Self::Unauthorized(_) | Self::Forbidden(_))
    }
}

/// A type alias for `Result<T, MetError>`.
pub type Result<T> = std::result::Result<T, MetError>;

/// Extension trait for adding context to errors.
pub trait ResultExt<T> {
    /// Add context to an error, converting it to a MetError.
    fn with_context<F, S>(self, f: F) -> Result<T>
    where
        F: FnOnce() -> S,
        S: Into<String>;
}

impl<T, E: std::error::Error> ResultExt<T> for std::result::Result<T, E> {
    fn with_context<F, S>(self, f: F) -> Result<T>
    where
        F: FnOnce() -> S,
        S: Into<String>,
    {
        self.map_err(|e| MetError::Internal(format!("{}: {e}", f().into())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_not_found_error() {
        let err = MetError::not_found("pipeline", "pipe_123");
        assert!(err.is_not_found());
        assert_eq!(err.to_string(), "not found: pipeline with id pipe_123");
    }

    #[test]
    fn test_auth_errors() {
        let unauth = MetError::unauthorized("missing token");
        assert!(unauth.is_auth_error());

        let forbidden = MetError::forbidden("insufficient permissions");
        assert!(forbidden.is_auth_error());
    }
}
