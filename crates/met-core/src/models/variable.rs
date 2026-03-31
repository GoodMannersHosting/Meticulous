//! Variable models.
//!
//! Variables are non-secret configuration values that can be scoped
//! globally or to specific projects.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{OrganizationId, ProjectId, VariableId};

/// A configuration variable.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct Variable {
    /// Unique identifier.
    pub id: VariableId,
    /// Owning organization.
    pub org_id: OrganizationId,
    /// Project scope (None for global variables).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<ProjectId>,
    /// Scope level.
    pub scope: VariableScope,
    /// Variable name.
    pub name: String,
    /// Variable value.
    pub value: String,
    /// Whether to mask the value in logs.
    #[serde(default)]
    pub is_sensitive: bool,
    /// Optional description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// When the variable was created.
    pub created_at: DateTime<Utc>,
    /// When the variable was last updated.
    pub updated_at: DateTime<Utc>,
}

impl Variable {
    /// Create a new global variable.
    #[must_use]
    pub fn global(
        org_id: OrganizationId,
        name: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: VariableId::new(),
            org_id,
            project_id: None,
            scope: VariableScope::Global,
            name: name.into(),
            value: value.into(),
            is_sensitive: false,
            description: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Create a new project-scoped variable.
    #[must_use]
    pub fn project_scoped(
        org_id: OrganizationId,
        project_id: ProjectId,
        name: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: VariableId::new(),
            org_id,
            project_id: Some(project_id),
            scope: VariableScope::Project,
            name: name.into(),
            value: value.into(),
            is_sensitive: false,
            description: None,
            created_at: now,
            updated_at: now,
        }
    }
}

/// Scope of a variable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type))]
#[cfg_attr(
    feature = "sqlx",
    sqlx(type_name = "variable_scope", rename_all = "snake_case")
)]
#[serde(rename_all = "snake_case")]
pub enum VariableScope {
    /// Available to all projects in the organization.
    #[default]
    Global,
    /// Available only to a specific project.
    Project,
}

/// Input for creating a variable.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateVariable {
    /// Variable name.
    pub name: String,
    /// Variable value.
    pub value: String,
    /// Scope level.
    pub scope: VariableScope,
    /// Project ID (required if scope is Project).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<ProjectId>,
    /// Whether to mask in logs.
    #[serde(default)]
    pub is_sensitive: bool,
    /// Optional description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Input for updating a variable.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateVariable {
    /// New value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// Whether to mask in logs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_sensitive: Option<bool>,
    /// New description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}
