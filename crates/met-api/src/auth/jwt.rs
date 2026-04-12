//! JWT token creation and validation.
//!
//! Creates and validates JWT tokens for user authentication.

use crate::config::JwtConfig;
use crate::extractors::CurrentUser;
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use met_core::{OrganizationId, UserId};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

/// Errors that can occur during JWT validation.
#[derive(Debug, Error)]
pub enum JwtError {
    /// Token is malformed or has invalid signature.
    #[error("invalid token: {0}")]
    Invalid(#[from] jsonwebtoken::errors::Error),

    /// Token has expired.
    #[error("token expired")]
    Expired,

    /// Token is missing required claims.
    #[error("missing required claim: {0}")]
    MissingClaim(&'static str),

    /// Token issuer doesn't match.
    #[error("invalid issuer")]
    InvalidIssuer,

    /// Token audience doesn't match.
    #[error("invalid audience")]
    InvalidAudience,
}

/// JWT claims structure.
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    /// Subject (user ID).
    pub sub: String,
    /// Issuer.
    pub iss: String,
    /// Audience.
    pub aud: String,
    /// Expiration time (Unix timestamp).
    pub exp: i64,
    /// Issued at (Unix timestamp).
    pub iat: i64,
    /// Organization ID.
    pub org_id: String,
    /// User email.
    pub email: String,
    /// User display name.
    #[serde(default)]
    pub name: Option<String>,
    /// Permissions.
    #[serde(default)]
    pub permissions: Vec<String>,
}

/// JWT token validator.
pub struct JwtValidator {
    decoding_key: DecodingKey,
    validation: Validation,
}

impl JwtValidator {
    /// Create a new JWT validator with the given configuration.
    pub fn new(config: &JwtConfig) -> Self {
        let decoding_key = DecodingKey::from_secret(config.secret.as_bytes());

        let mut validation = Validation::new(Algorithm::HS256);
        validation.set_issuer(&[&config.issuer]);
        validation.set_audience(&[&config.audience]);
        validation.validate_exp = true;

        Self {
            decoding_key,
            validation,
        }
    }

    /// Validate a JWT token and extract the current user.
    pub fn validate(&self, token: &str) -> Result<CurrentUser, JwtError> {
        let token_data = decode::<Claims>(token, &self.decoding_key, &self.validation)?;
        let claims = token_data.claims;

        let user_id = Uuid::parse_str(&claims.sub)
            .map(UserId::from_uuid)
            .map_err(|_| JwtError::MissingClaim("sub"))?;

        let org_id = Uuid::parse_str(&claims.org_id)
            .map(OrganizationId::from_uuid)
            .map_err(|_| JwtError::MissingClaim("org_id"))?;

        Ok(CurrentUser {
            user_id,
            org_id,
            email: claims.email,
            name: claims.name,
            permissions: claims.permissions.into_iter().collect(),
            is_api_token: false,
            project_ids: None, // JWT tokens have access to all projects
            pipeline_ids: None,
            password_must_change: false, // refreshed from DB in `finalize_authenticated_user`
            api_token_id: None,
        })
    }
}

/// Create a JWT token for a user.
pub fn create_jwt(
    config: &JwtConfig,
    user_id: UserId,
    org_id: OrganizationId,
    email: &str,
    name: Option<&str>,
    permissions: Vec<String>,
) -> Result<String, JwtError> {
    let now = chrono::Utc::now().timestamp();
    let exp = now + config.expiration.as_secs() as i64;

    let claims = Claims {
        sub: user_id.as_uuid().to_string(),
        iss: config.issuer.clone(),
        aud: config.audience.clone(),
        exp,
        iat: now,
        org_id: org_id.as_uuid().to_string(),
        email: email.to_string(),
        name: name.map(String::from),
        permissions,
    };

    let key = EncodingKey::from_secret(config.secret.as_bytes());
    encode(&Header::default(), &claims, &key).map_err(JwtError::from)
}

/// Create a JWT token for testing purposes.
#[cfg(test)]
pub fn create_test_token(config: &JwtConfig, user_id: UserId, org_id: OrganizationId) -> String {
    create_jwt(
        config,
        user_id,
        org_id,
        "test@example.com",
        Some("Test User"),
        vec!["*".to_string()],
    )
    .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jwt_validation() {
        let config = JwtConfig::default();
        let user_id = UserId::new();
        let org_id = OrganizationId::new();

        let token = create_test_token(&config, user_id, org_id);
        let validator = JwtValidator::new(&config);

        let user = validator.validate(&token).unwrap();
        assert_eq!(user.user_id, user_id);
        assert_eq!(user.org_id, org_id);
        assert_eq!(user.email, "test@example.com");
        assert!(!user.is_api_token);
    }

    #[test]
    fn test_invalid_token() {
        let config = JwtConfig::default();
        let validator = JwtValidator::new(&config);

        let result = validator.validate("invalid.token.here");
        assert!(result.is_err());
    }

    #[test]
    fn test_wrong_secret() {
        let mut config = JwtConfig::default();
        let user_id = UserId::new();
        let org_id = OrganizationId::new();

        let token = create_test_token(&config, user_id, org_id);

        config.secret = "different-secret".to_string();
        let validator = JwtValidator::new(&config);

        let result = validator.validate(&token);
        assert!(result.is_err());
    }
}
