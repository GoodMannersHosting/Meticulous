//! RBAC models for permission roles, API tokens, and auth providers.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::user::GroupRole;
use crate::ids::{
    ApiTokenId, AuthProviderId, GroupId, OidcGroupMappingId, OrganizationId, PipelineId, ProjectId,
    UserId,
};

/// Permission roles available in the system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type))]
#[cfg_attr(
    feature = "sqlx",
    sqlx(type_name = "permission_role", rename_all = "snake_case")
)]
#[serde(rename_all = "snake_case")]
pub enum PermissionRole {
    /// Unrestricted access (break-glass). Use sparingly; audited.
    SuperAdmin,
    /// Org-wide metadata and permission management. Cannot read pipeline
    /// definitions, run logs, secret values, or artifact content.
    Admin,
    /// Read-only access to all resources and audit logs.
    Auditor,
    /// User management, token revocation, and audit logs.
    SecurityLead,
    /// Org-wide security search (blast radius); platform admins use `*`.
    SecurityAuditor,
    /// Standard read/write for assigned projects.
    User,
}

impl PermissionRole {
    /// Get the permissions granted by this role.
    #[must_use]
    pub fn permissions(&self) -> Vec<&'static str> {
        match self {
            Self::SuperAdmin => vec!["*"],
            Self::Admin => vec![
                "admin:metadata",
                "admin:permissions",
                "user:read",
                "user:write",
                "audit:read",
            ],
            Self::Auditor => vec!["read:*", "audit:read"],
            Self::SecurityLead => vec!["user:read", "user:write", "token:revoke", "audit:read"],
            Self::SecurityAuditor => vec!["security:blast-radius:org", "read:*"],
            Self::User => vec!["pipeline:read", "run:read", "run:write", "agent:read"],
        }
    }

    /// Whether this role grants unrestricted access (break-glass).
    #[must_use]
    pub fn is_super_admin(self) -> bool {
        matches!(self, Self::SuperAdmin)
    }

    /// Whether this role is a platform admin (metadata-only, no content access).
    #[must_use]
    pub fn is_platform_admin(self) -> bool {
        matches!(self, Self::Admin)
    }
}

/// A user role assignment.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct UserRole {
    /// User ID.
    pub user_id: UserId,
    /// Assigned role.
    pub role: PermissionRole,
    /// Who granted this role.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub granted_by: Option<UserId>,
    /// When the role was granted.
    pub granted_at: DateTime<Utc>,
}

/// An API token for programmatic access.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct ApiToken {
    /// Unique identifier.
    pub id: ApiTokenId,
    /// Owning user.
    pub user_id: UserId,
    /// Token name for display.
    pub name: String,
    /// Optional description of the token's purpose.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Hash of the token secret.
    #[serde(skip_serializing)]
    pub token_hash: String,
    /// First 8 characters for display.
    pub prefix: String,
    /// Scopes/permissions granted.
    pub scopes: Vec<String>,
    /// Projects this token can access (None = all projects).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_ids: Option<Vec<ProjectId>>,
    /// Pipelines this token can access (None = all pipelines within allowed projects).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pipeline_ids: Option<Vec<PipelineId>>,
    /// When the token expires.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
    /// Last time the token was used.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<DateTime<Utc>>,
    /// When the token was revoked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revoked_at: Option<DateTime<Utc>>,
    /// When the token was deactivated (reactivatable).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deactivated_at: Option<DateTime<Utc>>,
    /// When the token was created.
    pub created_at: DateTime<Utc>,
}

impl ApiToken {
    /// Check if the token is valid (not expired or revoked).
    #[must_use]
    pub fn is_valid(&self) -> bool {
        if self.revoked_at.is_some() {
            return false;
        }
        if self.deactivated_at.is_some() {
            return false;
        }
        if let Some(expires_at) = self.expires_at {
            if expires_at <= Utc::now() {
                return false;
            }
        }
        true
    }

    /// Check if the token can access a specific project.
    #[must_use]
    pub fn can_access_project(&self, project_id: ProjectId) -> bool {
        self.project_ids
            .as_ref()
            .map_or(true, |ids| ids.contains(&project_id))
    }

    /// Check pipeline access (call after project allowlist is satisfied).
    #[must_use]
    pub fn can_access_pipeline(&self, pipeline_id: PipelineId) -> bool {
        self.pipeline_ids
            .as_ref()
            .map_or(true, |ids| ids.contains(&pipeline_id))
    }
}

/// Input for creating an API token.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateApiToken {
    /// Token name.
    pub name: String,
    /// Optional description of the token's purpose.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Scopes/permissions.
    #[serde(default)]
    pub scopes: Vec<String>,
    /// Project IDs (None = all projects).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_ids: Option<Vec<ProjectId>>,
    /// Pipeline IDs (None = all pipelines in allowed projects).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pipeline_ids: Option<Vec<PipelineId>>,
    /// Expiration in seconds from now.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_in: Option<i64>,
}

/// Response when creating an API token (includes the plain token).
#[derive(Debug, Clone, Serialize)]
pub struct CreateApiTokenResponse {
    /// The created token metadata.
    pub token: ApiToken,
    /// The plain token value (only shown once).
    pub plain_token: String,
}

/// An authentication provider (OIDC or GitHub).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct AuthProvider {
    /// Unique identifier.
    pub id: AuthProviderId,
    /// Owning organization.
    pub org_id: OrganizationId,
    /// Provider type (oidc, github).
    pub provider_type: String,
    /// Display name.
    pub name: String,
    /// OAuth client ID.
    pub client_id: String,
    /// Reference to the client secret in secrets store.
    #[serde(skip_serializing)]
    pub client_secret_ref: String,
    /// OIDC issuer URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issuer_url: Option<String>,
    /// Whether this provider is enabled.
    pub enabled: bool,
    /// Additional configuration (JSON).
    pub config: serde_json::Value,
    /// When the provider was created.
    pub created_at: DateTime<Utc>,
    /// When the provider was last updated.
    pub updated_at: DateTime<Utc>,
}

/// Input for creating an auth provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAuthProvider {
    /// Provider type (oidc, github).
    pub provider_type: String,
    /// Display name.
    pub name: String,
    /// OAuth client ID.
    pub client_id: String,
    /// OAuth client secret.
    pub client_secret: String,
    /// OIDC issuer URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issuer_url: Option<String>,
    /// Additional configuration.
    #[serde(default)]
    pub config: serde_json::Value,
}

/// Input for updating an auth provider.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateAuthProvider {
    /// New display name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// New OAuth client ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    /// New OAuth client secret.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
    /// New OIDC issuer URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issuer_url: Option<String>,
    /// New configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<serde_json::Value>,
}

/// OIDC group to Meticulous group mapping.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct OidcGroupMapping {
    /// Unique identifier.
    pub id: OidcGroupMappingId,
    /// Auth provider ID.
    pub provider_id: AuthProviderId,
    /// OIDC group claim value.
    pub oidc_group_claim: String,
    /// Meticulous group ID.
    pub meticulous_group_id: GroupId,
    /// Role to assign when mapping.
    pub role: GroupRole,
    /// When the mapping was created.
    pub created_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_super_admin_permissions_contain_wildcard() {
        assert!(PermissionRole::SuperAdmin.permissions().contains(&"*"));
    }

    #[test]
    fn test_super_admin_is_super_admin() {
        assert!(PermissionRole::SuperAdmin.is_super_admin());
    }

    #[test]
    fn test_admin_is_not_super_admin() {
        assert!(!PermissionRole::Admin.is_super_admin());
    }

    #[test]
    fn test_admin_is_platform_admin() {
        assert!(PermissionRole::Admin.is_platform_admin());
    }

    #[test]
    fn test_super_admin_is_not_platform_admin() {
        assert!(!PermissionRole::SuperAdmin.is_platform_admin());
    }

    #[test]
    fn test_admin_permissions_do_not_contain_wildcard() {
        let perms = PermissionRole::Admin.permissions();
        assert!(!perms.contains(&"*"));
        assert!(perms.contains(&"admin:metadata"));
    }
}

/// Input for creating an OIDC group mapping.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateOidcGroupMapping {
    /// OIDC group claim value.
    pub oidc_group_claim: String,
    /// Meticulous group ID.
    pub meticulous_group_id: GroupId,
    /// Role to assign.
    #[serde(default)]
    pub role: GroupRole,
}

/// Platform settings entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct PlatformSetting {
    /// Setting key.
    pub key: String,
    /// Setting value (JSON).
    pub value: serde_json::Value,
    /// When the setting was last updated.
    pub updated_at: DateTime<Utc>,
    /// Who last updated the setting.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by: Option<UserId>,
}
