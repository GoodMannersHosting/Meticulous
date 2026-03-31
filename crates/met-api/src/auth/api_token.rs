//! API token validation.
//!
//! API tokens are prefixed with `met_` and can be used for programmatic access.
//! They are validated against the database and have associated permissions.
//!
//! Token format: `met_<token_id>_<secret>`
//! - `met_` - fixed prefix for identification
//! - `token_id` - first 8 chars of the UUID (for identification)
//! - `secret` - 32 random characters (the actual secret)
//!
//! The full token is hashed using SHA-256 for storage comparison.

use crate::extractors::CurrentUser;
use met_core::hash_join_token;
use met_core::ids::ProjectId;
use met_store::repos::{ApiTokenRepo, UserRepo};
use met_store::PgPool;
use std::collections::HashSet;
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

    /// User account is disabled.
    #[error("user account is disabled")]
    UserDisabled,

    /// Database error during validation.
    #[error("database error: {0}")]
    Database(#[from] met_store::StoreError),
}

/// API token validator.
pub struct ApiTokenValidator<'a> {
    db: &'a PgPool,
}

impl<'a> ApiTokenValidator<'a> {
    /// Create a new API token validator.
    pub fn new(db: &'a PgPool) -> Self {
        Self { db }
    }

    /// Validate an API token and return the current user.
    ///
    /// Token format: `met_<prefix>_<secret>`
    pub async fn validate(&self, token: &str) -> Result<CurrentUser, ApiTokenError> {
        // Check token prefix
        if !token.starts_with("met_") {
            return Err(ApiTokenError::InvalidFormat);
        }

        // Hash the full token for lookup
        let token_hash = hash_join_token(token);

        // Look up the token by hash
        let api_token_repo = ApiTokenRepo::new(self.db);
        let api_token = api_token_repo
            .get_by_hash(&token_hash)
            .await?
            .ok_or(ApiTokenError::NotFound)?;

        // Check if token is valid (not expired or revoked)
        if !api_token.is_valid() {
            if api_token.revoked_at.is_some() {
                return Err(ApiTokenError::NotFound);
            }
            return Err(ApiTokenError::Expired);
        }

        // Get the owning user
        let user_repo = UserRepo::new(self.db);
        let user = user_repo.get(api_token.user_id).await?;

        // Check if user is active
        if !user.is_active {
            return Err(ApiTokenError::UserDisabled);
        }

        // Update last used timestamp (fire and forget)
        let token_id = api_token.id;
        let pool = self.db.clone();
        tokio::spawn(async move {
            let repo = ApiTokenRepo::new(&pool);
            if let Err(e) = repo.touch(token_id).await {
                tracing::warn!(token_id = %token_id, error = %e, "failed to update token last_used_at");
            }
        });

        // Build permissions from scopes
        let permissions: HashSet<String> = api_token.scopes.iter().cloned().collect();

        // Convert project_ids if present
        let project_ids: Option<Vec<ProjectId>> = api_token.project_ids;

        tracing::debug!(
            token_id = %api_token.id,
            user_id = %user.id,
            scopes = ?api_token.scopes,
            "API token validated successfully"
        );

        Ok(CurrentUser {
            user_id: user.id,
            org_id: user.org_id,
            email: user.email,
            name: user.display_name,
            permissions,
            is_api_token: true,
            project_ids,
            password_must_change: user.password_must_change,
        })
    }
}

/// Hash an API token for storage/lookup.
///
/// Uses SHA-256 for fast, secure hashing.
/// API tokens are already high-entropy random strings, so we don't need
/// the computational cost of Argon2 like we do for passwords.
pub fn hash_token(token: &str) -> String {
    hash_join_token(token)
}

/// Generate a new API token.
///
/// Returns (full_token, prefix, hash) tuple.
/// - full_token: The complete token to give to the user (only shown once)
/// - prefix: First few chars for display (stored in DB)
/// - hash: SHA-256 hash of the full token (stored in DB for validation)
pub fn generate_token() -> (String, String, String) {
    use rand::Rng;
    
    let mut rng = rand::thread_rng();
    
    // Generate 32 random bytes and encode as hex (64 chars)
    let secret: [u8; 32] = rng.r#gen();
    let secret_hex = hex::encode(secret);
    
    // Token format: met_<random_secret>
    let full_token = format!("met_{secret_hex}");
    
    // Prefix is first 12 chars for display (met_ + 8 chars of secret)
    let prefix = full_token[..12].to_string();
    
    // Hash the full token for storage
    let hash = hash_token(&full_token);
    
    (full_token, prefix, hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_invalid_prefix() {
        assert!(!("invalid_token".starts_with("met_")));
        assert!("met_abc123".starts_with("met_"));
    }

    #[test]
    fn test_hash_token() {
        let token = "met_abc123def456";
        let hash1 = hash_token(token);
        let hash2 = hash_token(token);
        
        // Same input produces same hash
        assert_eq!(hash1, hash2);
        
        // Hash is a hex string (64 chars for SHA-256)
        assert_eq!(hash1.len(), 64);
        assert!(hash1.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_generate_token() {
        let (token, prefix, hash) = generate_token();
        
        // Token starts with met_
        assert!(token.starts_with("met_"));
        
        // Prefix is first 12 chars
        assert_eq!(prefix.len(), 12);
        assert!(token.starts_with(&prefix));
        
        // Hash matches
        assert_eq!(hash, hash_token(&token));
        
        // Token is correct length (met_ + 64 hex chars)
        assert_eq!(token.len(), 68);
    }

    #[test]
    fn test_unique_tokens() {
        let (token1, _, _) = generate_token();
        let (token2, _, _) = generate_token();
        
        assert_ne!(token1, token2);
    }
}
