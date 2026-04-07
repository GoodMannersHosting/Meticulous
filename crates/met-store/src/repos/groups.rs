//! Group repository.

use chrono::Utc;
use met_core::ids::{GroupId, OrganizationId, UserId};
use met_core::models::{CreateGroup, Group, GroupMembership, GroupRole};
use sqlx::PgPool;

use crate::error::{Result, StoreError};

/// Repository for group operations.
pub struct GroupRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> GroupRepo<'a> {
    /// Create a new group repository.
    #[must_use]
    pub const fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Create a new group.
    pub async fn create(&self, org_id: OrganizationId, input: &CreateGroup) -> Result<Group> {
        let id = GroupId::new();
        let now = Utc::now();

        let group = sqlx::query_as::<_, Group>(
            r#"
            INSERT INTO groups (id, org_id, name, description, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $5)
            RETURNING id, org_id, name, description, created_at, updated_at
            "#,
        )
        .bind(id.as_uuid())
        .bind(org_id.as_uuid())
        .bind(&input.name)
        .bind(&input.description)
        .bind(now)
        .fetch_one(self.pool)
        .await?;

        Ok(group)
    }

    /// Get a group by ID.
    pub async fn get(&self, id: GroupId) -> Result<Group> {
        sqlx::query_as::<_, Group>(
            r#"
            SELECT id, org_id, name, description, created_at, updated_at
            FROM groups
            WHERE id = $1
            "#,
        )
        .bind(id.as_uuid())
        .fetch_optional(self.pool)
        .await?
        .ok_or_else(|| StoreError::not_found("group", id))
    }

    /// Get a group by name within an organization.
    pub async fn get_by_name(&self, org_id: OrganizationId, name: &str) -> Result<Option<Group>> {
        let group = sqlx::query_as::<_, Group>(
            r#"
            SELECT id, org_id, name, description, created_at, updated_at
            FROM groups
            WHERE org_id = $1 AND name = $2
            "#,
        )
        .bind(org_id.as_uuid())
        .bind(name)
        .fetch_optional(self.pool)
        .await?;

        Ok(group)
    }

    /// List groups in an organization.
    pub async fn list(
        &self,
        org_id: OrganizationId,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Group>> {
        let groups = sqlx::query_as::<_, Group>(
            r#"
            SELECT id, org_id, name, description, created_at, updated_at
            FROM groups
            WHERE org_id = $1
            ORDER BY name ASC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(org_id.as_uuid())
        .bind(limit)
        .bind(offset)
        .fetch_all(self.pool)
        .await?;

        Ok(groups)
    }

    /// Update a group.
    pub async fn update(
        &self,
        id: GroupId,
        name: Option<&str>,
        description: Option<&str>,
    ) -> Result<Group> {
        let existing = self.get(id).await?;

        let name = name.unwrap_or(&existing.name);
        let description = description.or(existing.description.as_deref());

        let group = sqlx::query_as::<_, Group>(
            r#"
            UPDATE groups
            SET name = $2, description = $3, updated_at = NOW()
            WHERE id = $1
            RETURNING id, org_id, name, description, created_at, updated_at
            "#,
        )
        .bind(id.as_uuid())
        .bind(name)
        .bind(description)
        .fetch_one(self.pool)
        .await?;

        Ok(group)
    }

    /// Delete a group.
    pub async fn delete(&self, id: GroupId) -> Result<()> {
        let result = sqlx::query(
            r#"
            DELETE FROM groups WHERE id = $1
            "#,
        )
        .bind(id.as_uuid())
        .execute(self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(StoreError::not_found("group", id));
        }

        Ok(())
    }

    /// Count groups in an organization.
    pub async fn count(&self, org_id: OrganizationId) -> Result<i64> {
        let (count,): (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*) FROM groups WHERE org_id = $1
            "#,
        )
        .bind(org_id.as_uuid())
        .fetch_one(self.pool)
        .await?;

        Ok(count)
    }

    /// Add a user to a group.
    pub async fn add_member(
        &self,
        group_id: GroupId,
        user_id: UserId,
        role: GroupRole,
    ) -> Result<GroupMembership> {
        let now = Utc::now();

        let membership = sqlx::query_as::<_, GroupMembership>(
            r#"
            INSERT INTO group_memberships (group_id, user_id, role, created_at)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (group_id, user_id) DO UPDATE SET role = $3
            RETURNING group_id, user_id, role, created_at
            "#,
        )
        .bind(group_id.as_uuid())
        .bind(user_id.as_uuid())
        .bind(role)
        .bind(now)
        .fetch_one(self.pool)
        .await?;

        Ok(membership)
    }

    /// Remove a user from a group.
    pub async fn remove_member(&self, group_id: GroupId, user_id: UserId) -> Result<()> {
        let result = sqlx::query(
            r#"
            DELETE FROM group_memberships
            WHERE group_id = $1 AND user_id = $2
            "#,
        )
        .bind(group_id.as_uuid())
        .bind(user_id.as_uuid())
        .execute(self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(StoreError::not_found(
                "group_membership",
                format!("{group_id}/{user_id}"),
            ));
        }

        Ok(())
    }

    /// Update a member's role.
    pub async fn update_member_role(
        &self,
        group_id: GroupId,
        user_id: UserId,
        role: GroupRole,
    ) -> Result<GroupMembership> {
        let membership = sqlx::query_as::<_, GroupMembership>(
            r#"
            UPDATE group_memberships
            SET role = $3
            WHERE group_id = $1 AND user_id = $2
            RETURNING group_id, user_id, role, created_at
            "#,
        )
        .bind(group_id.as_uuid())
        .bind(user_id.as_uuid())
        .bind(role)
        .fetch_optional(self.pool)
        .await?
        .ok_or_else(|| {
            StoreError::not_found("group_membership", format!("{group_id}/{user_id}"))
        })?;

        Ok(membership)
    }

    /// List members of a group.
    pub async fn list_members(&self, group_id: GroupId) -> Result<Vec<GroupMembership>> {
        let memberships = sqlx::query_as::<_, GroupMembership>(
            r#"
            SELECT group_id, user_id, role, created_at
            FROM group_memberships
            WHERE group_id = $1
            ORDER BY created_at ASC
            "#,
        )
        .bind(group_id.as_uuid())
        .fetch_all(self.pool)
        .await?;

        Ok(memberships)
    }

    /// List groups a user belongs to.
    pub async fn list_user_groups(&self, user_id: UserId) -> Result<Vec<GroupMembership>> {
        let memberships = sqlx::query_as::<_, GroupMembership>(
            r#"
            SELECT group_id, user_id, role, created_at
            FROM group_memberships
            WHERE user_id = $1
            ORDER BY created_at ASC
            "#,
        )
        .bind(user_id.as_uuid())
        .fetch_all(self.pool)
        .await?;

        Ok(memberships)
    }

    /// Count members in a group.
    pub async fn count_members(&self, group_id: GroupId) -> Result<i64> {
        let (count,): (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*) FROM group_memberships WHERE group_id = $1
            "#,
        )
        .bind(group_id.as_uuid())
        .fetch_one(self.pool)
        .await?;

        Ok(count)
    }

    /// Check if a user is a member of a group.
    pub async fn is_member(&self, group_id: GroupId, user_id: UserId) -> Result<bool> {
        let (exists,): (bool,) = sqlx::query_as(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM group_memberships
                WHERE group_id = $1 AND user_id = $2
            )
            "#,
        )
        .bind(group_id.as_uuid())
        .bind(user_id.as_uuid())
        .fetch_one(self.pool)
        .await?;

        Ok(exists)
    }
}
