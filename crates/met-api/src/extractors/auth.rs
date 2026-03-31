//! Authentication extractor for JWT and API token validation.
//!
//! Supports two authentication schemes:
//! - `Authorization: Bearer <jwt>` - JWT tokens for user sessions
//! - `Authorization: Token met_<token>` - API tokens for programmatic access

use crate::auth::{ApiTokenValidator, JwtValidator};
use crate::error::ApiError;
use crate::state::AppState;
use axum::{
    extract::{FromRef, FromRequestParts},
    http::{header::AUTHORIZATION, request::Parts},
};
use met_core::{OrganizationId, UserId};
use met_store::PgPool;
use std::collections::HashSet;

/// Authenticated user information extracted from the request.
#[derive(Debug, Clone)]
pub struct CurrentUser {
    /// User ID.
    pub user_id: UserId,
    /// Organization ID the user belongs to.
    pub org_id: OrganizationId,
    /// User's email address.
    pub email: String,
    /// User's display name.
    pub name: Option<String>,
    /// Permissions granted to this user.
    pub permissions: HashSet<String>,
    /// Whether this is an API token (vs JWT).
    pub is_api_token: bool,
}

impl CurrentUser {
    /// Check if the user has a specific permission.
    pub fn has_permission(&self, permission: &str) -> bool {
        self.permissions.contains(permission) || self.permissions.contains("*")
    }

    /// Check if the user has any of the specified permissions.
    pub fn has_any_permission(&self, permissions: &[&str]) -> bool {
        permissions.iter().any(|p| self.has_permission(p))
    }

    /// Check if the user has all of the specified permissions.
    pub fn has_all_permissions(&self, permissions: &[&str]) -> bool {
        permissions.iter().all(|p| self.has_permission(p))
    }
}

/// Authentication extractor that validates JWT or API tokens.
///
/// Use this as an extractor in handler functions:
/// ```ignore
/// async fn my_handler(Auth(user): Auth) -> impl IntoResponse {
///     // user is a CurrentUser
/// }
/// ```
#[derive(Debug, Clone)]
pub struct Auth(pub CurrentUser);

impl std::ops::Deref for Auth {
    type Target = CurrentUser;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<S> FromRequestParts<S> for Auth
where
    S: Send + Sync,
    AppState: FromRef<S>,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let app_state = AppState::from_ref(state);

        let auth_header = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| ApiError::unauthorized("missing authorization header"))?;

        // Try Bearer token (JWT) first
        if let Some(token) = auth_header.strip_prefix("Bearer ") {
            let validator = JwtValidator::new(&app_state.config.jwt);
            let user = validator
                .validate(token)
                .map_err(|e| ApiError::unauthorized(e.to_string()))?;
            
            // Verify user is still active in the database
            // This ensures deleted or locked users can't continue using existing tokens
            let is_valid = verify_user_session(app_state.db(), &user).await;
            if !is_valid {
                return Err(ApiError::unauthorized("session invalidated"));
            }
            
            return Ok(Auth(user));
        }

        // Try API token
        if let Some(token) = auth_header.strip_prefix("Token ") {
            let validator = ApiTokenValidator::new(app_state.db());
            let user = validator
                .validate(token)
                .await
                .map_err(|e| ApiError::unauthorized(e.to_string()))?;
            return Ok(Auth(user));
        }

        Err(ApiError::unauthorized(
            "invalid authorization header format, expected 'Bearer <jwt>' or 'Token met_<token>'",
        ))
    }
}

/// Verify that the user's session is still valid.
/// 
/// This checks:
/// - User exists and is not deleted
/// - User is active (not locked)
/// 
/// Returns false if the session should be invalidated.
async fn verify_user_session(db: &sqlx::PgPool, user: &CurrentUser) -> bool {
    let result: Option<(bool, bool)> = sqlx::query_as(
        r#"
        SELECT is_active, (deleted_at IS NULL) as not_deleted
        FROM users
        WHERE id = $1
        "#,
    )
    .bind(user.user_id.as_uuid())
    .fetch_optional(db)
    .await
    .ok()
    .flatten();

    match result {
        Some((is_active, not_deleted)) => is_active && not_deleted,
        None => false, // User not found
    }
}

/// Optional authentication extractor.
///
/// Returns `None` if no auth header is present, or `Some(CurrentUser)` if valid.
/// Returns an error only if auth is present but invalid.
#[derive(Debug, Clone)]
pub struct OptionalAuth(pub Option<CurrentUser>);

impl<S> FromRequestParts<S> for OptionalAuth
where
    S: Send + Sync,
    AppState: FromRef<S>,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        if parts.headers.get(AUTHORIZATION).is_none() {
            return Ok(OptionalAuth(None));
        }

        let Auth(user) = Auth::from_request_parts(parts, state).await?;
        Ok(OptionalAuth(Some(user)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_current_user_permissions() {
        let user = CurrentUser {
            user_id: UserId::new(),
            org_id: OrganizationId::new(),
            email: "test@example.com".to_string(),
            name: Some("Test User".to_string()),
            permissions: ["pipelines:read", "pipelines:write"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            is_api_token: false,
        };

        assert!(user.has_permission("pipelines:read"));
        assert!(user.has_permission("pipelines:write"));
        assert!(!user.has_permission("pipelines:delete"));

        assert!(user.has_any_permission(&["pipelines:read", "admin"]));
        assert!(!user.has_any_permission(&["admin", "superuser"]));

        assert!(user.has_all_permissions(&["pipelines:read", "pipelines:write"]));
        assert!(!user.has_all_permissions(&["pipelines:read", "admin"]));
    }

    #[test]
    fn test_wildcard_permission() {
        let admin = CurrentUser {
            user_id: UserId::new(),
            org_id: OrganizationId::new(),
            email: "admin@example.com".to_string(),
            name: None,
            permissions: ["*"].iter().map(|s| s.to_string()).collect(),
            is_api_token: false,
        };

        assert!(admin.has_permission("pipelines:read"));
        assert!(admin.has_permission("anything:at:all"));
    }
}
