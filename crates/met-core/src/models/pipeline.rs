//! Pipeline model.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use super::project::{OwnerType, ResourceVisibility};
use crate::ids::{PipelineId, ProjectId};

/// A pipeline defines a CI/CD workflow with jobs and steps.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct Pipeline {
    /// Unique identifier.
    pub id: PipelineId,
    /// Owning project.
    pub project_id: ProjectId,
    /// Display name.
    pub name: String,
    /// URL-safe identifier (unique within project).
    pub slug: String,
    /// Optional description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The parsed pipeline definition (stored as JSON).
    #[cfg_attr(feature = "sqlx", sqlx(json))]
    pub definition: JsonValue,
    /// Path to the pipeline definition file (if file-based).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub definition_path: Option<String>,
    /// SCM provider when definition is loaded from a remote repo (e.g. `github`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scm_provider: Option<String>,
    /// Repository slug (`owner/name`) or normalized identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scm_repository: Option<String>,
    /// Git ref (branch, tag, or SHA) last synced.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scm_ref: Option<String>,
    /// Path to pipeline YAML within the repo.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scm_path: Option<String>,
    /// `builtin_secrets.path` for GitHub App credentials (project-scoped).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scm_credentials_secret_path: Option<String>,
    /// Commit SHA of the definition blob last fetched.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scm_revision: Option<String>,
    /// Type of entity that owns this pipeline.
    pub owner_type: OwnerType,
    /// ID of the pipeline owner.
    pub owner_id: String,
    /// Who may discover and view this pipeline.
    pub visibility: ResourceVisibility,
    /// Whether the pipeline is enabled.
    pub enabled: bool,
    /// When the pipeline was archived (soft).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<DateTime<Utc>>,
    /// When the pipeline was created.
    pub created_at: DateTime<Utc>,
    /// When the pipeline was last updated.
    pub updated_at: DateTime<Utc>,
}

impl Pipeline {
    /// Create a new pipeline with default values.
    #[must_use]
    pub fn new(
        project_id: ProjectId,
        name: impl Into<String>,
        slug: impl Into<String>,
        definition: JsonValue,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: PipelineId::new(),
            project_id,
            name: name.into(),
            slug: slug.into(),
            description: None,
            definition,
            definition_path: None,
            scm_provider: None,
            scm_repository: None,
            scm_ref: None,
            scm_path: None,
            scm_credentials_secret_path: None,
            scm_revision: None,
            owner_type: OwnerType::User,
            owner_id: String::new(),
            visibility: ResourceVisibility::default(),
            enabled: true,
            archived_at: None,
            created_at: now,
            updated_at: now,
        }
    }
}

/// Input for creating a new pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePipeline {
    /// Display name.
    pub name: String,
    /// URL-safe identifier.
    pub slug: String,
    /// Optional description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Pipeline definition (YAML string or parsed JSON).
    pub definition: JsonValue,
    /// Path to definition file (if file-based).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub definition_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scm_provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scm_repository: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scm_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scm_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scm_credentials_secret_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scm_revision: Option<String>,
    /// Visibility tier (defaults to `authenticated`).
    #[serde(default)]
    pub visibility: ResourceVisibility,
}

/// Input for updating a pipeline.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdatePipeline {
    /// New display name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// New description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// New definition.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub definition: Option<JsonValue>,
    /// Whether the pipeline is enabled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scm_provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scm_repository: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scm_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scm_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scm_credentials_secret_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scm_revision: Option<String>,
    /// New visibility tier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visibility: Option<ResourceVisibility>,
}

/// A member of a pipeline (direct or inherited from project).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct PipelineMember {
    pub id: uuid::Uuid,
    pub pipeline_id: PipelineId,
    pub principal_type: String,
    pub principal_id: uuid::Uuid,
    pub role: String,
    pub inherited: bool,
    pub created_at: DateTime<Utc>,
}
