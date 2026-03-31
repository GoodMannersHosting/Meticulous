//! RBAC authorization middleware for the API.
//!
//! Provides the `authorize()` function and `RequirePermission` extractor
//! that checks RBAC permissions before handler execution.

use axum::{
    extract::{FromRef, FromRequestParts},
    http::request::Parts,
};
use met_core::{OrganizationId, ProjectId};
use tracing::{debug, warn};

use crate::error::ApiError;
use crate::extractors::auth::{Auth, CurrentUser};
use crate::state::AppState;

/// Five-tier role hierarchy matching met-secrets RBAC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ApiRole {
    PlatformAdmin = 100,
    OrgAdmin = 80,
    ProjectAdmin = 60,
    Developer = 40,
    Viewer = 20,
}

impl ApiRole {
    pub fn from_permissions(perms: &std::collections::HashSet<String>) -> Self {
        if perms.contains("*") || perms.contains("platform_admin") {
            return Self::PlatformAdmin;
        }
        if perms.contains("org_admin") { return Self::OrgAdmin; }
        if perms.contains("project_admin") { return Self::ProjectAdmin; }
        if perms.contains("developer") { return Self::Developer; }
        Self::Viewer
    }

    pub fn has_at_least(&self, required: ApiRole) -> bool {
        (*self as u8) >= (required as u8)
    }
}

/// Check if a user has the required role level.
pub fn authorize(user: &CurrentUser, required_role: ApiRole) -> Result<(), ApiError> {
    let user_role = ApiRole::from_permissions(&user.permissions);

    if user_role.has_at_least(required_role) {
        debug!(
            user_id = %user.user_id,
            user_role = ?user_role,
            required = ?required_role,
            "Authorization granted"
        );
        Ok(())
    } else {
        warn!(
            user_id = %user.user_id,
            user_role = ?user_role,
            required = ?required_role,
            "Authorization denied: insufficient permissions"
        );
        Err(ApiError::forbidden(format!(
            "requires {:?} role, you have {:?}",
            required_role, user_role
        )))
    }
}

/// Check if a user can access a specific project.
pub fn authorize_project(user: &CurrentUser, project_id: ProjectId) -> Result<(), ApiError> {
    if !user.can_access_project(project_id) {
        warn!(
            user_id = %user.user_id,
            project_id = %project_id,
            "Authorization denied: no project access"
        );
        return Err(ApiError::forbidden("no access to this project"));
    }
    Ok(())
}

/// Extractor that requires a specific permission string.
///
/// Usage:
/// ```ignore
/// async fn admin_handler(
///     RequirePermission("org_admin"): RequirePermission<"org_admin">,
/// ) -> impl IntoResponse { ... }
/// ```
pub struct RequireRole<const ROLE: u8>;

// Convenience type aliases
pub type RequirePlatformAdmin = RequireRole<100>;
pub type RequireOrgAdmin = RequireRole<80>;
pub type RequireProjectAdmin = RequireRole<60>;
pub type RequireDeveloper = RequireRole<40>;
pub type RequireViewer = RequireRole<20>;

/// Extractor that validates the user has a minimum role and extracts the user.
pub struct Authorized<const MIN_ROLE: u8>(pub CurrentUser);

impl<S, const MIN_ROLE: u8> FromRequestParts<S> for Authorized<MIN_ROLE>
where
    S: Send + Sync,
    AppState: FromRef<S>,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let Auth(user) = Auth::from_request_parts(parts, state).await?;

        let required = match MIN_ROLE {
            100 => ApiRole::PlatformAdmin,
            80 => ApiRole::OrgAdmin,
            60 => ApiRole::ProjectAdmin,
            40 => ApiRole::Developer,
            _ => ApiRole::Viewer,
        };

        authorize(&user, required)?;
        Ok(Authorized(user))
    }
}

impl<const N: u8> std::ops::Deref for Authorized<N> {
    type Target = CurrentUser;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use met_core::{OrganizationId, UserId};
    use std::collections::HashSet;

    fn make_user(perms: &[&str]) -> CurrentUser {
        CurrentUser {
            user_id: UserId::new(),
            org_id: OrganizationId::new(),
            email: "test@example.com".to_string(),
            name: None,
            permissions: perms.iter().map(|s| s.to_string()).collect(),
            is_api_token: false,
            project_ids: None,
            password_must_change: false,
        }
    }

    #[test]
    fn test_authorize_platform_admin() {
        let user = make_user(&["*"]);
        assert!(authorize(&user, ApiRole::PlatformAdmin).is_ok());
        assert!(authorize(&user, ApiRole::Viewer).is_ok());
    }

    #[test]
    fn test_authorize_developer() {
        let user = make_user(&["developer"]);
        assert!(authorize(&user, ApiRole::Developer).is_ok());
        assert!(authorize(&user, ApiRole::Viewer).is_ok());
        assert!(authorize(&user, ApiRole::ProjectAdmin).is_err());
    }

    #[test]
    fn test_authorize_viewer_insufficient() {
        let user = make_user(&["viewer"]);
        assert!(authorize(&user, ApiRole::Viewer).is_ok());
        assert!(authorize(&user, ApiRole::Developer).is_err());
        assert!(authorize(&user, ApiRole::OrgAdmin).is_err());
    }

    #[test]
    fn test_project_access() {
        let project1 = ProjectId::new();
        let project2 = ProjectId::new();

        let user = CurrentUser {
            user_id: UserId::new(),
            org_id: OrganizationId::new(),
            email: "test@example.com".to_string(),
            name: None,
            permissions: HashSet::new(),
            is_api_token: true,
            project_ids: Some(vec![project1]),
            password_must_change: false,
        };

        assert!(authorize_project(&user, project1).is_ok());
        assert!(authorize_project(&user, project2).is_err());
    }

    #[test]
    fn test_role_from_permissions() {
        assert_eq!(ApiRole::from_permissions(&["*"].iter().map(|s| s.to_string()).collect()), ApiRole::PlatformAdmin);
        assert_eq!(ApiRole::from_permissions(&["org_admin"].iter().map(|s| s.to_string()).collect()), ApiRole::OrgAdmin);
        assert_eq!(ApiRole::from_permissions(&["developer"].iter().map(|s| s.to_string()).collect()), ApiRole::Developer);
        assert_eq!(ApiRole::from_permissions(&["read"].iter().map(|s| s.to_string()).collect()), ApiRole::Viewer);
    }
}
