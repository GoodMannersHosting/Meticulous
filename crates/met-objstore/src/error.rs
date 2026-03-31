//! Error types for object storage operations.

use thiserror::Error;

/// Errors that can occur during object storage operations.
#[derive(Debug, Error)]
pub enum ObjectStoreError {
    /// Object not found at the specified key.
    #[error("Object not found: {key}")]
    NotFound { key: String },

    /// Access denied to the object or bucket.
    #[error("Access denied: {message}")]
    AccessDenied { message: String },

    /// The bucket does not exist.
    #[error("Bucket not found: {bucket}")]
    BucketNotFound { bucket: String },

    /// Invalid object key format.
    #[error("Invalid key: {message}")]
    InvalidKey { message: String },

    /// Object already exists (for conditional puts).
    #[error("Object already exists: {key}")]
    AlreadyExists { key: String },

    /// Request timeout.
    #[error("Operation timed out: {operation}")]
    Timeout { operation: String },

    /// Connection error.
    #[error("Connection error: {message}")]
    Connection { message: String },

    /// Multipart upload error.
    #[error("Multipart upload error: {message}")]
    MultipartUpload { message: String },

    /// Presigned URL generation error.
    #[error("Presigned URL error: {message}")]
    PresignedUrl { message: String },

    /// Configuration error.
    #[error("Configuration error: {message}")]
    Configuration { message: String },

    /// S3 SDK error.
    #[error("S3 error: {message}")]
    S3 { message: String },

    /// IO error during streaming.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Generic internal error.
    #[error("Internal error: {message}")]
    Internal { message: String },
}

impl ObjectStoreError {
    pub fn not_found(key: impl Into<String>) -> Self {
        Self::NotFound { key: key.into() }
    }

    pub fn access_denied(message: impl Into<String>) -> Self {
        Self::AccessDenied { message: message.into() }
    }

    pub fn bucket_not_found(bucket: impl Into<String>) -> Self {
        Self::BucketNotFound { bucket: bucket.into() }
    }

    pub fn invalid_key(message: impl Into<String>) -> Self {
        Self::InvalidKey { message: message.into() }
    }

    pub fn already_exists(key: impl Into<String>) -> Self {
        Self::AlreadyExists { key: key.into() }
    }

    pub fn timeout(operation: impl Into<String>) -> Self {
        Self::Timeout { operation: operation.into() }
    }

    pub fn connection(message: impl Into<String>) -> Self {
        Self::Connection { message: message.into() }
    }

    pub fn multipart(message: impl Into<String>) -> Self {
        Self::MultipartUpload { message: message.into() }
    }

    pub fn presigned(message: impl Into<String>) -> Self {
        Self::PresignedUrl { message: message.into() }
    }

    pub fn config(message: impl Into<String>) -> Self {
        Self::Configuration { message: message.into() }
    }

    pub fn s3(message: impl Into<String>) -> Self {
        Self::S3 { message: message.into() }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal { message: message.into() }
    }

    /// Check if this error indicates the object was not found.
    pub fn is_not_found(&self) -> bool {
        matches!(self, Self::NotFound { .. })
    }

    /// Check if this error indicates access was denied.
    pub fn is_access_denied(&self) -> bool {
        matches!(self, Self::AccessDenied { .. })
    }

    /// Check if this error is retryable.
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::Timeout { .. } | Self::Connection { .. })
    }
}

/// Result type for object storage operations.
pub type Result<T> = std::result::Result<T, ObjectStoreError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_constructors() {
        let err = ObjectStoreError::not_found("test/key");
        assert!(err.is_not_found());
        assert!(!err.is_retryable());

        let err = ObjectStoreError::timeout("put_object");
        assert!(err.is_retryable());
    }

    #[test]
    fn test_error_display() {
        let err = ObjectStoreError::not_found("test/key");
        assert_eq!(err.to_string(), "Object not found: test/key");
    }
}
