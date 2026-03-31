//! Pipeline model.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

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
    /// Whether the pipeline is enabled.
    pub enabled: bool,
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
            enabled: true,
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
}
