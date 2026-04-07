//! Organization (tenant) model.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::OrganizationId;

/// An organization represents a tenant boundary in Meticulous.
///
/// All projects, users, and resources belong to exactly one organization.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct Organization {
    /// Unique identifier.
    pub id: OrganizationId,
    /// Display name.
    pub name: String,
    /// URL-safe identifier (unique).
    pub slug: String,
    /// When the organization was created.
    pub created_at: DateTime<Utc>,
    /// When the organization was last updated.
    pub updated_at: DateTime<Utc>,
    /// Soft-delete timestamp (None if active).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<DateTime<Utc>>,
    /// When false, global catalog workflows marked untrusted are blocked from execution.
    pub allow_untrusted_workflows: bool,
}

impl Organization {
    /// Create a new organization with default timestamps.
    #[must_use]
    pub fn new(name: impl Into<String>, slug: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: OrganizationId::new(),
            name: name.into(),
            slug: slug.into(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            allow_untrusted_workflows: true,
        }
    }

    /// Check if the organization is active (not deleted).
    #[must_use]
    pub const fn is_active(&self) -> bool {
        self.deleted_at.is_none()
    }
}

/// Input for creating a new organization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateOrganization {
    /// Display name.
    pub name: String,
    /// URL-safe identifier.
    pub slug: String,
}

/// Input for updating an organization.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateOrganization {
    /// New display name (if changing).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Allow execution of org-global workflows in untrusted state (catalog trust model).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_untrusted_workflows: Option<bool>,
}
