//! Reusable workflow models.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::ids::{OrganizationId, ProjectId, WorkflowId};

/// A reusable workflow that can be referenced from pipelines.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct ReusableWorkflow {
    /// Unique identifier.
    pub id: WorkflowId,
    /// Owning organization.
    pub org_id: OrganizationId,
    /// Project scope (None for global workflows).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<ProjectId>,
    /// Scope level.
    pub scope: WorkflowScope,
    /// Workflow name.
    pub name: String,
    /// Version string (semver recommended).
    pub version: String,
    /// Workflow definition (parsed YAML as JSON).
    #[cfg_attr(feature = "sqlx", sqlx(json))]
    pub definition: JsonValue,
    /// Optional description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// When the workflow was created.
    pub created_at: DateTime<Utc>,
    /// When the workflow was last updated.
    pub updated_at: DateTime<Utc>,
}

impl ReusableWorkflow {
    /// Create a new global workflow.
    #[must_use]
    pub fn global(
        org_id: OrganizationId,
        name: impl Into<String>,
        version: impl Into<String>,
        definition: JsonValue,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: WorkflowId::new(),
            org_id,
            project_id: None,
            scope: WorkflowScope::Global,
            name: name.into(),
            version: version.into(),
            definition,
            description: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Create a new project-scoped workflow.
    #[must_use]
    pub fn project_scoped(
        org_id: OrganizationId,
        project_id: ProjectId,
        name: impl Into<String>,
        version: impl Into<String>,
        definition: JsonValue,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: WorkflowId::new(),
            org_id,
            project_id: Some(project_id),
            scope: WorkflowScope::Project,
            name: name.into(),
            version: version.into(),
            definition,
            description: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Get a reference string for this workflow (e.g., "global/docker-build@1.0.0").
    #[must_use]
    pub fn reference(&self) -> String {
        let prefix = match self.scope {
            WorkflowScope::Global => "global",
            WorkflowScope::Project => "project",
        };
        format!("{}/{}@{}", prefix, self.name, self.version)
    }
}

/// Scope of a reusable workflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type))]
#[cfg_attr(
    feature = "sqlx",
    sqlx(type_name = "workflow_scope", rename_all = "snake_case")
)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowScope {
    /// Available to all projects in the organization.
    #[default]
    Global,
    /// Available only to a specific project.
    Project,
}

/// Input for creating a reusable workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateWorkflow {
    /// Workflow name.
    pub name: String,
    /// Version string.
    pub version: String,
    /// Scope level.
    pub scope: WorkflowScope,
    /// Project ID (required if scope is Project).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<ProjectId>,
    /// Workflow definition.
    pub definition: JsonValue,
    /// Optional description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}
