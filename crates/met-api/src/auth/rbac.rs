//! Role-Based Access Control (RBAC) enforcement.
//!
//! Provides permission checking utilities and macros for protecting endpoints.

use crate::error::ApiError;
use crate::extractors::CurrentUser;

/// Standard permissions used in the API.
///
/// Permissions follow the pattern `resource:action` where:
/// - resource: pipelines, runs, agents, secrets, etc.
/// - action: read, write, delete, admin
pub struct Permission;

impl Permission {
    // Organization permissions
    pub const ORG_READ: &'static str = "org:read";
    pub const ORG_WRITE: &'static str = "org:write";
    pub const ORG_ADMIN: &'static str = "org:admin";

    // Project permissions
    pub const PROJECT_READ: &'static str = "project:read";
    pub const PROJECT_WRITE: &'static str = "project:write";
    pub const PROJECT_DELETE: &'static str = "project:delete";

    // Pipeline permissions
    pub const PIPELINE_READ: &'static str = "pipeline:read";
    pub const PIPELINE_WRITE: &'static str = "pipeline:write";
    pub const PIPELINE_DELETE: &'static str = "pipeline:delete";
    pub const PIPELINE_TRIGGER: &'static str = "pipeline:trigger";

    // Run permissions
    pub const RUN_READ: &'static str = "run:read";
    pub const RUN_CANCEL: &'static str = "run:cancel";
    pub const RUN_RETRY: &'static str = "run:retry";

    // Agent permissions
    pub const AGENT_READ: &'static str = "agent:read";
    pub const AGENT_WRITE: &'static str = "agent:write";
    pub const AGENT_DELETE: &'static str = "agent:delete";

    // Secret permissions
    pub const SECRET_READ: &'static str = "secret:read";
    pub const SECRET_WRITE: &'static str = "secret:write";
    pub const SECRET_DELETE: &'static str = "secret:delete";

    // Variable permissions
    pub const VARIABLE_READ: &'static str = "variable:read";
    pub const VARIABLE_WRITE: &'static str = "variable:write";
    pub const VARIABLE_DELETE: &'static str = "variable:delete";

    // Admin permissions
    pub const ADMIN: &'static str = "*";
}

/// Check if a user has a required permission.
///
/// Returns `Ok(())` if the user has the permission, or an `ApiError::Forbidden` if not.
pub fn require_permission(user: &CurrentUser, permission: &str) -> Result<(), ApiError> {
    if user.has_permission(permission) {
        Ok(())
    } else {
        Err(ApiError::forbidden(format!(
            "missing required permission: {permission}"
        )))
    }
}

/// Check if a user has any of the required permissions.
pub fn require_any_permission(user: &CurrentUser, permissions: &[&str]) -> Result<(), ApiError> {
    if user.has_any_permission(permissions) {
        Ok(())
    } else {
        Err(ApiError::forbidden(format!(
            "missing required permission: one of {}",
            permissions.join(", ")
        )))
    }
}

/// Check if a user has all of the required permissions.
pub fn require_all_permissions(user: &CurrentUser, permissions: &[&str]) -> Result<(), ApiError> {
    if user.has_all_permissions(permissions) {
        Ok(())
    } else {
        let missing: Vec<_> = permissions
            .iter()
            .filter(|p| !user.has_permission(p))
            .collect();
        Err(ApiError::forbidden(format!(
            "missing required permissions: {}",
            missing
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use met_core::{OrganizationId, UserId};

    fn test_user(permissions: &[&str]) -> CurrentUser {
        CurrentUser {
            user_id: UserId::new(),
            org_id: OrganizationId::new(),
            email: "test@example.com".to_string(),
            name: None,
            permissions: permissions.iter().map(|s| s.to_string()).collect(),
            is_api_token: false,
        }
    }

    #[test]
    fn test_require_permission_success() {
        let user = test_user(&[Permission::PIPELINE_READ]);
        assert!(require_permission(&user, Permission::PIPELINE_READ).is_ok());
    }

    #[test]
    fn test_require_permission_failure() {
        let user = test_user(&[Permission::PIPELINE_READ]);
        assert!(require_permission(&user, Permission::PIPELINE_WRITE).is_err());
    }

    #[test]
    fn test_admin_has_all_permissions() {
        let admin = test_user(&[Permission::ADMIN]);
        assert!(require_permission(&admin, Permission::PIPELINE_READ).is_ok());
        assert!(require_permission(&admin, Permission::SECRET_DELETE).is_ok());
        assert!(require_permission(&admin, "any:permission").is_ok());
    }

    #[test]
    fn test_require_any_permission() {
        let user = test_user(&[Permission::PIPELINE_READ]);
        assert!(
            require_any_permission(&user, &[Permission::PIPELINE_READ, Permission::PIPELINE_WRITE])
                .is_ok()
        );
        assert!(
            require_any_permission(&user, &[Permission::SECRET_READ, Permission::SECRET_WRITE])
                .is_err()
        );
    }

    #[test]
    fn test_require_all_permissions() {
        let user = test_user(&[Permission::PIPELINE_READ, Permission::PIPELINE_WRITE]);
        assert!(
            require_all_permissions(&user, &[Permission::PIPELINE_READ, Permission::PIPELINE_WRITE])
                .is_ok()
        );
        assert!(require_all_permissions(
            &user,
            &[Permission::PIPELINE_READ, Permission::PIPELINE_DELETE]
        )
        .is_err());
    }
}
