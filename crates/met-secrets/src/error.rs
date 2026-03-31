//! Error types for the secrets subsystem.
//!
//! This module defines all error types that can occur when interacting with
//! secret providers, validating tokens, or checking permissions.

use std::fmt;

/// Result type alias for secrets operations.
pub type Result<T> = std::result::Result<T, SecretsError>;

/// Errors that can occur in the secrets subsystem.
#[derive(Debug, thiserror::Error)]
pub enum SecretsError {
    /// Secret was not found at the specified path.
    #[error("secret not found: {path}")]
    NotFound {
        /// The path that was requested.
        path: String,
    },

    /// Access to the secret was denied.
    #[error("access denied to secret: {path}")]
    AccessDenied {
        /// The path that was denied.
        path: String,
        /// Optional reason for the denial.
        reason: Option<String>,
    },

    /// The secret provider is not available or not configured.
    #[error("provider unavailable: {provider}")]
    ProviderUnavailable {
        /// The provider that is unavailable.
        provider: String,
        /// The underlying error message.
        message: String,
    },

    /// Failed to connect to the secret provider.
    #[error("connection failed to {provider}: {message}")]
    ConnectionFailed {
        /// The provider that failed.
        provider: String,
        /// The error message.
        message: String,
    },

    /// Authentication with the provider failed.
    #[error("authentication failed with {provider}: {message}")]
    AuthenticationFailed {
        /// The provider that failed.
        provider: String,
        /// The error message.
        message: String,
    },

    /// The secret value could not be parsed or decoded.
    #[error("invalid secret format: {message}")]
    InvalidFormat {
        /// Description of what went wrong.
        message: String,
    },

    /// Rate limit exceeded on the provider.
    #[error("rate limit exceeded for {provider}")]
    RateLimited {
        /// The provider that rate limited the request.
        provider: String,
        /// Optional retry-after duration in seconds.
        retry_after_secs: Option<u64>,
    },

    /// The provider returned an unexpected error.
    #[error("provider error from {provider}: {message}")]
    ProviderError {
        /// The provider that errored.
        provider: String,
        /// The error message.
        message: String,
    },

    /// Configuration error.
    #[error("configuration error: {0}")]
    Configuration(String),

    /// Encryption or decryption failed.
    #[error("crypto error: {0}")]
    Crypto(String),

    /// Timeout waiting for provider response.
    #[error("timeout waiting for {provider} after {timeout_secs}s")]
    Timeout {
        /// The provider that timed out.
        provider: String,
        /// How long we waited.
        timeout_secs: u64,
    },
}

impl SecretsError {
    /// Create a not-found error for the given path.
    pub fn not_found(path: impl Into<String>) -> Self {
        Self::NotFound { path: path.into() }
    }

    /// Create an access denied error.
    pub fn access_denied(path: impl Into<String>, reason: impl Into<Option<String>>) -> Self {
        Self::AccessDenied {
            path: path.into(),
            reason: reason.into(),
        }
    }

    /// Create a provider unavailable error.
    pub fn provider_unavailable(provider: impl Into<String>, message: impl Into<String>) -> Self {
        Self::ProviderUnavailable {
            provider: provider.into(),
            message: message.into(),
        }
    }

    /// Create a connection failed error.
    pub fn connection_failed(provider: impl Into<String>, message: impl Into<String>) -> Self {
        Self::ConnectionFailed {
            provider: provider.into(),
            message: message.into(),
        }
    }

    /// Create an authentication failed error.
    pub fn auth_failed(provider: impl Into<String>, message: impl Into<String>) -> Self {
        Self::AuthenticationFailed {
            provider: provider.into(),
            message: message.into(),
        }
    }

    /// Check if this error indicates the secret doesn't exist.
    pub fn is_not_found(&self) -> bool {
        matches!(self, Self::NotFound { .. })
    }

    /// Check if this error indicates a permission issue.
    pub fn is_access_denied(&self) -> bool {
        matches!(self, Self::AccessDenied { .. })
    }

    /// Check if this error is retryable.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::RateLimited { .. }
                | Self::Timeout { .. }
                | Self::ConnectionFailed { .. }
                | Self::ProviderUnavailable { .. }
        )
    }
}

/// Errors that can occur during OIDC token validation.
#[derive(Debug, thiserror::Error)]
pub enum OidcError {
    /// The token has expired.
    #[error("token expired")]
    TokenExpired,

    /// The token signature is invalid.
    #[error("invalid token signature")]
    InvalidSignature,

    /// The token issuer is not trusted.
    #[error("untrusted issuer: {0}")]
    UntrustedIssuer(String),

    /// The token audience doesn't match.
    #[error("invalid audience: expected {expected}, got {actual}")]
    InvalidAudience {
        /// Expected audience.
        expected: String,
        /// Actual audience in token.
        actual: String,
    },

    /// The token is malformed.
    #[error("malformed token: {0}")]
    MalformedToken(String),

    /// Required claim is missing.
    #[error("missing claim: {0}")]
    MissingClaim(String),

    /// Failed to fetch JWKS from the issuer.
    #[error("failed to fetch JWKS: {0}")]
    JwksFetchFailed(String),

    /// No matching key found in JWKS.
    #[error("no matching key found for kid: {0}")]
    KeyNotFound(String),

    /// The token is not yet valid (nbf claim).
    #[error("token not yet valid")]
    TokenNotYetValid,

    /// Generic validation error.
    #[error("validation error: {0}")]
    Validation(String),
}

impl OidcError {
    /// Check if this error indicates the token should not be retried.
    pub fn is_permanent(&self) -> bool {
        matches!(
            self,
            Self::TokenExpired
                | Self::InvalidSignature
                | Self::UntrustedIssuer(_)
                | Self::InvalidAudience { .. }
                | Self::MalformedToken(_)
        )
    }
}

/// Errors that can occur during RBAC permission checks.
#[derive(Debug, thiserror::Error)]
pub enum RbacError {
    /// The actor does not have the required permission.
    #[error("permission denied: {actor} cannot {action} on {resource}")]
    PermissionDenied {
        /// Who attempted the action.
        actor: String,
        /// What action was attempted.
        action: String,
        /// What resource was targeted.
        resource: String,
    },

    /// The role is not recognized.
    #[error("unknown role: {0}")]
    UnknownRole(String),

    /// The permission is not recognized.
    #[error("unknown permission: {0}")]
    UnknownPermission(String),

    /// The resource type is not recognized.
    #[error("unknown resource type: {0}")]
    UnknownResourceType(String),

    /// Policy evaluation failed.
    #[error("policy evaluation error: {0}")]
    PolicyError(String),
}

/// Errors that can occur during audit logging.
#[derive(Debug)]
pub struct AuditError {
    /// What operation failed.
    pub operation: String,
    /// The error message.
    pub message: String,
    /// Whether the error is retryable.
    pub retryable: bool,
}

impl fmt::Display for AuditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "audit error during {}: {}", self.operation, self.message)
    }
}

impl std::error::Error for AuditError {}

impl AuditError {
    /// Create a new audit error.
    pub fn new(operation: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            operation: operation.into(),
            message: message.into(),
            retryable: false,
        }
    }

    /// Mark this error as retryable.
    pub fn retryable(mut self) -> Self {
        self.retryable = true;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secrets_error_not_found() {
        let err = SecretsError::not_found("secret/myapp/api-key");
        assert!(err.is_not_found());
        assert!(!err.is_retryable());
        assert!(err.to_string().contains("secret/myapp/api-key"));
    }

    #[test]
    fn test_secrets_error_retryable() {
        let err = SecretsError::RateLimited {
            provider: "vault".into(),
            retry_after_secs: Some(30),
        };
        assert!(err.is_retryable());

        let err = SecretsError::Timeout {
            provider: "aws".into(),
            timeout_secs: 5,
        };
        assert!(err.is_retryable());
    }

    #[test]
    fn test_oidc_error_permanent() {
        assert!(OidcError::TokenExpired.is_permanent());
        assert!(OidcError::InvalidSignature.is_permanent());
        assert!(!OidcError::JwksFetchFailed("network".into()).is_permanent());
    }
}
