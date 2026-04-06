//! Meticulous App integrations (machine-to-machine auth).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{AppInstallationId, AppKeyId, MeticulousAppId, ProjectId, UserId};

/// Known installation permission strings (allowlist on JWT + DB).
pub mod app_permissions {
    /// Create scoped join tokens for the installation project (and org when required).
    pub const JOIN_TOKENS_CREATE: &str = "join_tokens:create";
    /// Revoke join tokens in the installation scope.
    pub const JOIN_TOKENS_REVOKE: &str = "join_tokens:revoke";
    /// Soft-delete agents registered in the installation org (e.g. Kubernetes operator cleanup).
    pub const AGENTS_DELETE: &str = "agents:delete";
}

/// DB row: registered app.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct MeticulousApp {
    pub id: MeticulousAppId,
    pub application_id: String,
    pub name: String,
    pub description: Option<String>,
    pub created_by: UserId,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// DB row: app signing key (public half).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct MeticulousAppKey {
    pub id: AppKeyId,
    pub app_id: MeticulousAppId,
    pub key_id: String,
    pub public_key_pem: String,
    pub created_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}

/// DB row: project installation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct MeticulousAppInstallation {
    pub id: AppInstallationId,
    pub app_id: MeticulousAppId,
    pub project_id: ProjectId,
    pub permissions: Vec<String>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}
