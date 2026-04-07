//! User and group models.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{GroupId, OrganizationId, UserId};

/// A user account.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct User {
    /// Unique identifier.
    pub id: UserId,
    /// Owning organization.
    pub org_id: OrganizationId,
    /// Username (unique within org).
    pub username: String,
    /// Email address.
    pub email: String,
    /// Display name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// Password hash (argon2id).
    #[serde(skip_serializing)]
    pub password_hash: Option<String>,
    /// Whether the user is active.
    pub is_active: bool,
    /// Whether the user is an org admin.
    #[serde(default)]
    pub is_admin: bool,
    /// When true, password login is allowed but API access is limited until the password is changed.
    #[serde(default)]
    pub password_must_change: bool,
    /// External identity provider ID (for SSO).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_id: Option<String>,
    /// When the user was created.
    pub created_at: DateTime<Utc>,
    /// When the user was last updated.
    pub updated_at: DateTime<Utc>,
    /// Last successful interactive login (password or OAuth).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_login_at: Option<DateTime<Utc>>,
    /// Soft-delete timestamp.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<DateTime<Utc>>,
}

impl User {
    /// Create a new user.
    #[must_use]
    pub fn new(
        org_id: OrganizationId,
        username: impl Into<String>,
        email: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: UserId::new(),
            org_id,
            username: username.into(),
            email: email.into(),
            display_name: None,
            password_hash: None,
            is_active: true,
            is_admin: false,
            password_must_change: false,
            external_id: None,
            created_at: now,
            updated_at: now,
            last_login_at: None,
            deleted_at: None,
        }
    }

    /// Check if the user is active (not deleted).
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.is_active && self.deleted_at.is_none()
    }
}

/// A group of users for RBAC.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct Group {
    /// Unique identifier.
    pub id: GroupId,
    /// Owning organization.
    pub org_id: OrganizationId,
    /// Group name (unique within org).
    pub name: String,
    /// Optional description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// When the group was created.
    pub created_at: DateTime<Utc>,
    /// When the group was last updated.
    pub updated_at: DateTime<Utc>,
}

impl Group {
    /// Create a new group.
    #[must_use]
    pub fn new(org_id: OrganizationId, name: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: GroupId::new(),
            org_id,
            name: name.into(),
            description: None,
            created_at: now,
            updated_at: now,
        }
    }
}

/// Membership linking users to groups.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct GroupMembership {
    /// Group ID.
    pub group_id: GroupId,
    /// User ID.
    pub user_id: UserId,
    /// Role within the group.
    pub role: GroupRole,
    /// When the membership was created.
    pub created_at: DateTime<Utc>,
}

/// Role within a group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type))]
#[cfg_attr(
    feature = "sqlx",
    sqlx(type_name = "group_role", rename_all = "snake_case")
)]
#[serde(rename_all = "snake_case")]
pub enum GroupRole {
    /// Regular member.
    #[default]
    Member,
    /// Can manage group membership.
    Maintainer,
    /// Full control over the group.
    Owner,
}

/// Input for creating a user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateUser {
    /// Username.
    pub username: String,
    /// Email address.
    pub email: String,
    /// Display name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// Password (will be hashed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
}

/// Input for creating a group.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateGroup {
    /// Group name.
    pub name: String,
    /// Optional description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}
