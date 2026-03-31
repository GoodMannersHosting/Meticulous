//! Secret reference models.
//!
//! Meticulous never stores secret values directly - only references to
//! external secret providers (Vault, AWS Secrets Manager, etc.).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{OrganizationId, ProjectId, SecretId};

/// A reference to a secret stored in an external provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct SecretRef {
    /// Unique identifier.
    pub id: SecretId,
    /// Owning organization.
    pub org_id: OrganizationId,
    /// Project scope (None for global secrets).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<ProjectId>,
    /// Scope level.
    pub scope: SecretScope,
    /// Secret name (used in pipeline definitions).
    pub name: String,
    /// Provider type (vault, aws_sm, k8s, etc.).
    pub provider: String,
    /// Provider-specific reference (path, ARN, etc.).
    pub provider_ref: String,
    /// Optional description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// When the reference was created.
    pub created_at: DateTime<Utc>,
    /// When the reference was last updated.
    pub updated_at: DateTime<Utc>,
}

impl SecretRef {
    /// Create a new global secret reference.
    #[must_use]
    pub fn global(
        org_id: OrganizationId,
        name: impl Into<String>,
        provider: impl Into<String>,
        provider_ref: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: SecretId::new(),
            org_id,
            project_id: None,
            scope: SecretScope::Global,
            name: name.into(),
            provider: provider.into(),
            provider_ref: provider_ref.into(),
            description: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Create a new project-scoped secret reference.
    #[must_use]
    pub fn project_scoped(
        org_id: OrganizationId,
        project_id: ProjectId,
        name: impl Into<String>,
        provider: impl Into<String>,
        provider_ref: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: SecretId::new(),
            org_id,
            project_id: Some(project_id),
            scope: SecretScope::Project,
            name: name.into(),
            provider: provider.into(),
            provider_ref: provider_ref.into(),
            description: None,
            created_at: now,
            updated_at: now,
        }
    }
}

/// Scope of a secret.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type))]
#[cfg_attr(
    feature = "sqlx",
    sqlx(type_name = "secret_scope", rename_all = "snake_case")
)]
#[serde(rename_all = "snake_case")]
pub enum SecretScope {
    /// Available to all projects in the organization.
    #[default]
    Global,
    /// Available only to a specific project.
    Project,
}

/// Well-known secret providers.
pub mod providers {
    /// HashiCorp Vault / OpenBao.
    pub const VAULT: &str = "vault";
    /// AWS Secrets Manager.
    pub const AWS_SM: &str = "aws_sm";
    /// Kubernetes secrets.
    pub const K8S: &str = "k8s";
    /// Azure Key Vault.
    pub const AZURE_KV: &str = "azure_kv";
    /// GCP Secret Manager.
    pub const GCP_SM: &str = "gcp_sm";
    /// Built-in encrypted storage (discouraged).
    pub const BUILTIN: &str = "builtin";
}

/// Input for creating a secret reference.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSecretRef {
    /// Secret name.
    pub name: String,
    /// Scope level.
    pub scope: SecretScope,
    /// Project ID (required if scope is Project).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<ProjectId>,
    /// Provider type.
    pub provider: String,
    /// Provider-specific reference.
    pub provider_ref: String,
    /// Optional description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}
