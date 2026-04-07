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
    http::{Method, StatusCode, header::AUTHORIZATION, request::Parts},
};
use met_core::ids::ProjectId;
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
    /// Project IDs this token can access (None = all projects).
    /// Only set for API tokens with project scope restrictions.
    pub project_ids: Option<Vec<ProjectId>>,
    /// When true (from DB), only auth self-service routes are allowed until the password is changed.
    pub password_must_change: bool,
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

    /// Check if the user can access a specific project.
    ///
    /// Returns true if:
    /// - `project_ids` is None (user has access to all projects)
    /// - `project_ids` contains the specified project
    pub fn can_access_project(&self, project_id: ProjectId) -> bool {
        self.project_ids
            .as_ref()
            .map_or(true, |ids| ids.contains(&project_id))
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

        let method = parts.method.clone();
        let path = parts.uri.path().to_string();

        // Try Bearer token (JWT) first
        if let Some(token) = auth_header.strip_prefix("Bearer ") {
            let validator = JwtValidator::new(&app_state.config.jwt);
            let user = validator
                .validate(token)
                .map_err(|e| ApiError::unauthorized(e.to_string()))?;

            let user = finalize_authenticated_user(app_state.db(), user, &method, &path).await?;
            return Ok(Auth(user));
        }

        // Try API token
        if let Some(token) = auth_header.strip_prefix("Token ") {
            let validator = ApiTokenValidator::new(app_state.db());
            let user = validator
                .validate(token)
                .await
                .map_err(|e| ApiError::unauthorized(e.to_string()))?;
            let user = finalize_authenticated_user(app_state.db(), user, &method, &path).await?;
            return Ok(Auth(user));
        }

        Err(ApiError::unauthorized(
            "invalid authorization header format, expected 'Bearer <jwt>' or 'Token met_<token>'",
        ))
    }
}

/// Load session state from the database and enforce the forced password-change gate.
async fn finalize_authenticated_user(
    db: &sqlx::PgPool,
    mut user: CurrentUser,
    method: &Method,
    path: &str,
) -> Result<CurrentUser, ApiError> {
    let result: Option<(bool, bool, bool)> = sqlx::query_as(
        r#"
        SELECT is_active, (deleted_at IS NULL) AS not_deleted, password_must_change
        FROM users
        WHERE id = $1
        "#,
    )
    .bind(user.user_id.as_uuid())
    .fetch_optional(db)
    .await
    .map_err(|e| ApiError::internal(e.to_string()))?;

    let Some((is_active, not_deleted, password_must_change)) = result else {
        return Err(ApiError::unauthorized("session invalidated"));
    };

    if !is_active || !not_deleted {
        return Err(ApiError::unauthorized("session invalidated"));
    }

    user.password_must_change = password_must_change;

    if user.password_must_change && !is_password_change_exempt(method, path) {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "password_change_required",
            "you must change your password before continuing",
        ));
    }

    Ok(user)
}

/// Routes allowed while `password_must_change` is true (JWT and API token).
fn is_password_change_exempt(method: &Method, path: &str) -> bool {
    matches!(
        (method, path),
        (&Method::GET, "/auth/me")
            | (&Method::POST, "/auth/logout")
            | (&Method::POST, "/auth/change-password")
    )
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
            project_ids: None,
            password_must_change: false,
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
            project_ids: None,
            password_must_change: false,
        };

        assert!(admin.has_permission("pipelines:read"));
        assert!(admin.has_permission("anything:at:all"));
    }

    #[test]
    fn test_project_access() {
        let project1 = ProjectId::new();
        let project2 = ProjectId::new();
        let project3 = ProjectId::new();

        // User with no project restrictions
        let unrestricted = CurrentUser {
            user_id: UserId::new(),
            org_id: OrganizationId::new(),
            email: "user@example.com".to_string(),
            name: None,
            permissions: HashSet::new(),
            is_api_token: false,
            project_ids: None,
            password_must_change: false,
        };
        assert!(unrestricted.can_access_project(project1));
        assert!(unrestricted.can_access_project(project2));

        // User with specific project access
        let restricted = CurrentUser {
            user_id: UserId::new(),
            org_id: OrganizationId::new(),
            email: "user@example.com".to_string(),
            name: None,
            permissions: HashSet::new(),
            is_api_token: true,
            project_ids: Some(vec![project1, project2]),
            password_must_change: false,
        };
        assert!(restricted.can_access_project(project1));
        assert!(restricted.can_access_project(project2));
        assert!(!restricted.can_access_project(project3));
    }
}
