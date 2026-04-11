//! Project model.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{GroupId, OrganizationId, ProjectId, UserId};

/// Three-tier visibility for projects and pipelines (ADR-021).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type))]
#[cfg_attr(
    feature = "sqlx",
    sqlx(type_name = "resource_visibility", rename_all = "snake_case")
)]
#[serde(rename_all = "snake_case")]
pub enum ResourceVisibility {
    /// Visible to anyone (including unauthenticated when globally enabled).
    Public,
    /// Visible to any authenticated user in the org.
    Authenticated,
    /// Visible only to explicit members.
    Private,
}

impl Default for ResourceVisibility {
    fn default() -> Self {
        Self::Authenticated
    }
}

/// A project contains pipelines, secrets, and variables.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct Project {
    /// Unique identifier.
    pub id: ProjectId,
    /// Owning organization.
    pub org_id: OrganizationId,
    /// Display name.
    pub name: String,
    /// URL-safe identifier (unique within org).
    pub slug: String,
    /// Optional description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Type of owner (user or group).
    pub owner_type: OwnerType,
    /// ID of the owner (user or group).
    pub owner_id: String,
    /// Who may discover and view this project.
    pub visibility: ResourceVisibility,
    /// When the project was created.
    pub created_at: DateTime<Utc>,
    /// When the project was last updated.
    pub updated_at: DateTime<Utc>,
    /// Soft-delete timestamp.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<DateTime<Utc>>,
    /// When the project was archived.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<DateTime<Utc>>,
    /// When the project is scheduled for permanent deletion.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheduled_deletion_at: Option<DateTime<Utc>>,
}

impl Project {
    /// Create a new project owned by a user.
    #[must_use]
    pub fn new_user_owned(
        org_id: OrganizationId,
        name: impl Into<String>,
        slug: impl Into<String>,
        owner_id: UserId,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: ProjectId::new(),
            org_id,
            name: name.into(),
            slug: slug.into(),
            description: None,
            owner_type: OwnerType::User,
            owner_id: owner_id.to_string(),
            visibility: ResourceVisibility::default(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            archived_at: None,
            scheduled_deletion_at: None,
        }
    }

    /// Create a new project owned by a group.
    #[must_use]
    pub fn new_group_owned(
        org_id: OrganizationId,
        name: impl Into<String>,
        slug: impl Into<String>,
        owner_id: GroupId,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: ProjectId::new(),
            org_id,
            name: name.into(),
            slug: slug.into(),
            description: None,
            owner_type: OwnerType::Group,
            owner_id: owner_id.to_string(),
            visibility: ResourceVisibility::default(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            archived_at: None,
            scheduled_deletion_at: None,
        }
    }

    /// Check if the project is active (not deleted or archived).
    #[must_use]
    pub const fn is_active(&self) -> bool {
        self.deleted_at.is_none() && self.archived_at.is_none()
    }

    /// Check if the project is archived.
    #[must_use]
    pub const fn is_archived(&self) -> bool {
        self.archived_at.is_some()
    }

    /// Check if the project is pending deletion.
    #[must_use]
    pub const fn is_pending_deletion(&self) -> bool {
        self.scheduled_deletion_at.is_some()
    }

    /// Get the project lifecycle state.
    #[must_use]
    pub fn lifecycle_state(&self) -> ProjectLifecycleState {
        if self.deleted_at.is_some() {
            ProjectLifecycleState::Deleted
        } else if self.scheduled_deletion_at.is_some() {
            ProjectLifecycleState::PendingDeletion
        } else if self.archived_at.is_some() {
            ProjectLifecycleState::Archived
        } else {
            ProjectLifecycleState::Active
        }
    }
}

/// Project lifecycle states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectLifecycleState {
    /// Project is active and usable.
    Active,
    /// Project is archived (read-only).
    Archived,
    /// Project is scheduled for permanent deletion.
    PendingDeletion,
    /// Project has been permanently deleted.
    Deleted,
}

/// Type of entity that owns a project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type))]
#[cfg_attr(
    feature = "sqlx",
    sqlx(type_name = "owner_type", rename_all = "snake_case")
)]
#[serde(rename_all = "snake_case")]
pub enum OwnerType {
    /// Owned by an individual user.
    User,
    /// Owned by a group.
    Group,
}

/// Input for creating a new project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateProject {
    /// Display name.
    pub name: String,
    /// URL-safe identifier.
    pub slug: String,
    /// Optional description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Type of owner.
    pub owner_type: OwnerType,
    /// Owner ID (user or group).
    pub owner_id: String,
    /// Visibility tier (defaults to `authenticated`).
    #[serde(default)]
    pub visibility: ResourceVisibility,
}

/// Input for updating a project.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateProject {
    /// New display name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// New URL slug (unique per organization).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slug: Option<String>,
    /// New description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// New visibility tier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visibility: Option<ResourceVisibility>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_visibility_default_is_authenticated() {
        assert_eq!(ResourceVisibility::default(), ResourceVisibility::Authenticated);
    }

    #[test]
    fn test_resource_visibility_serde_roundtrip() {
        let cases = [
            ("\"public\"", ResourceVisibility::Public),
            ("\"authenticated\"", ResourceVisibility::Authenticated),
            ("\"private\"", ResourceVisibility::Private),
        ];
        for (json, expected) in cases {
            let deserialized: ResourceVisibility = serde_json::from_str(json).unwrap();
            assert_eq!(deserialized, expected);

            let serialized = serde_json::to_string(&expected).unwrap();
            assert_eq!(serialized, json);
        }
    }

    #[test]
    fn test_resource_visibility_rejects_unknown() {
        let result = serde_json::from_str::<ResourceVisibility>("\"internal\"");
        assert!(result.is_err());
    }
}
