//! API token validation.
//!
//! API tokens are prefixed with `met_` and can be used for programmatic access.
//! They are validated against the database and have associated permissions.

use crate::extractors::CurrentUser;
use met_store::PgPool;
use thiserror::Error;

/// Errors that can occur during API token validation.
#[derive(Debug, Error)]
pub enum ApiTokenError {
    /// Token format is invalid.
    #[error("invalid token format")]
    InvalidFormat,

    /// Token not found or revoked.
    #[error("token not found or revoked")]
    NotFound,

    /// Token has expired.
    #[error("token expired")]
    Expired,

    /// Database error during validation.
    #[error("database error: {0}")]
    Database(#[from] met_store::StoreError),
}

/// API token validator.
pub struct ApiTokenValidator<'a> {
    _db: &'a PgPool,
}

impl<'a> ApiTokenValidator<'a> {
    /// Create a new API token validator.
    pub fn new(db: &'a PgPool) -> Self {
        Self { _db: db }
    }

    /// Validate an API token and return the current user.
    ///
    /// Token format: `met_<base64_encoded_token_id>_<token_secret>`
    pub async fn validate(&self, token: &str) -> Result<CurrentUser, ApiTokenError> {
        // Check token prefix
        if !token.starts_with("met_") {
            return Err(ApiTokenError::InvalidFormat);
        }

        // TODO: Implement actual token validation against database
        //
        // A production implementation would:
        // 1. Parse the token format: met_<token_id>_<secret>
        // 2. Look up the token in the database by token_id
        // 3. Verify the secret hash matches
        // 4. Check expiration and revocation status
        // 5. Load the associated user/service account and permissions
        //
        // Example query:
        // ```sql
        // SELECT t.*, u.email, u.name, u.org_id,
        //        array_agg(p.permission) as permissions
        // FROM api_tokens t
        // JOIN users u ON t.user_id = u.id
        // LEFT JOIN token_permissions p ON t.id = p.token_id
        // WHERE t.id = $1
        //   AND t.revoked_at IS NULL
        //   AND (t.expires_at IS NULL OR t.expires_at > now())
        // GROUP BY t.id, u.id
        // ```

        // Stub implementation for development
        // In production, this would query the database
        tracing::warn!(
            token_prefix = &token[..8.min(token.len())],
            "API token validation not yet implemented, rejecting"
        );

        Err(ApiTokenError::NotFound)
    }
}

/// Hash an API token secret for storage.
///
/// Uses Argon2id for secure password hashing.
#[allow(dead_code)]
pub fn hash_token_secret(_secret: &str) -> String {
    // TODO: Implement with argon2 crate
    // argon2::hash_encoded(secret.as_bytes(), salt, &config)
    unimplemented!("token hashing not yet implemented")
}

/// Verify a token secret against its hash.
#[allow(dead_code)]
pub fn verify_token_secret(_secret: &str, _hash: &str) -> bool {
    // TODO: Implement with argon2 crate
    // argon2::verify_encoded(hash, secret.as_bytes())
    unimplemented!("token verification not yet implemented")
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_invalid_prefix() {
        assert!(!("invalid_token".starts_with("met_")));
        assert!("met_abc123".starts_with("met_"));
    }
}
