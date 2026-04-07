//! User role repository.

use chrono::Utc;
use met_core::ids::UserId;
use met_core::models::{PermissionRole, UserRole};
use sqlx::PgPool;

use crate::error::{Result, StoreError};

/// Repository for user role operations.
pub struct RoleRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> RoleRepo<'a> {
    /// Create a new role repository.
    #[must_use]
    pub const fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Assign a role to a user.
    pub async fn assign(
        &self,
        user_id: UserId,
        role: PermissionRole,
        granted_by: Option<UserId>,
    ) -> Result<UserRole> {
        let now = Utc::now();

        let user_role = sqlx::query_as::<_, UserRole>(
            r#"
            INSERT INTO user_roles (user_id, role, granted_by, granted_at)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (user_id, role) DO UPDATE SET granted_by = $3, granted_at = $4
            RETURNING user_id, role, granted_by, granted_at
            "#,
        )
        .bind(user_id.as_uuid())
        .bind(role)
        .bind(granted_by.map(|id| id.as_uuid()))
        .bind(now)
        .fetch_one(self.pool)
        .await?;

        Ok(user_role)
    }

    /// Remove a role from a user.
    pub async fn revoke(&self, user_id: UserId, role: PermissionRole) -> Result<()> {
        let result = sqlx::query(
            r#"
            DELETE FROM user_roles WHERE user_id = $1 AND role = $2
            "#,
        )
        .bind(user_id.as_uuid())
        .bind(role)
        .execute(self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(StoreError::not_found(
                "user_role",
                format!("{user_id}/{role:?}"),
            ));
        }

        Ok(())
    }

    /// Get all roles for a user.
    pub async fn get_user_roles(&self, user_id: UserId) -> Result<Vec<UserRole>> {
        let roles = sqlx::query_as::<_, UserRole>(
            r#"
            SELECT user_id, role, granted_by, granted_at
            FROM user_roles
            WHERE user_id = $1
            ORDER BY granted_at ASC
            "#,
        )
        .bind(user_id.as_uuid())
        .fetch_all(self.pool)
        .await?;

        Ok(roles)
    }

    /// Get all users with a specific role.
    pub async fn get_users_with_role(
        &self,
        role: PermissionRole,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<UserRole>> {
        let roles = sqlx::query_as::<_, UserRole>(
            r#"
            SELECT user_id, role, granted_by, granted_at
            FROM user_roles
            WHERE role = $1
            ORDER BY granted_at ASC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(role)
        .bind(limit)
        .bind(offset)
        .fetch_all(self.pool)
        .await?;

        Ok(roles)
    }

    /// Check if a user has a specific role.
    pub async fn has_role(&self, user_id: UserId, role: PermissionRole) -> Result<bool> {
        let (exists,): (bool,) = sqlx::query_as(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM user_roles
                WHERE user_id = $1 AND role = $2
            )
            "#,
        )
        .bind(user_id.as_uuid())
        .bind(role)
        .fetch_one(self.pool)
        .await?;

        Ok(exists)
    }

    /// Check if a user has admin role.
    pub async fn is_admin(&self, user_id: UserId) -> Result<bool> {
        self.has_role(user_id, PermissionRole::Admin).await
    }

    /// Count users with a specific role.
    pub async fn count_with_role(&self, role: PermissionRole) -> Result<i64> {
        let (count,): (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*) FROM user_roles WHERE role = $1
            "#,
        )
        .bind(role)
        .fetch_one(self.pool)
        .await?;

        Ok(count)
    }

    /// Remove all roles from a user.
    pub async fn revoke_all(&self, user_id: UserId) -> Result<u64> {
        let result = sqlx::query(
            r#"
            DELETE FROM user_roles WHERE user_id = $1
            "#,
        )
        .bind(user_id.as_uuid())
        .execute(self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    /// Get aggregated permissions for a user based on their roles.
    pub async fn get_permissions(&self, user_id: UserId) -> Result<Vec<String>> {
        let roles = self.get_user_roles(user_id).await?;

        let mut permissions = Vec::new();
        for user_role in roles {
            permissions.extend(
                user_role
                    .role
                    .permissions()
                    .iter()
                    .map(|s| (*s).to_string()),
            );
        }

        // Deduplicate
        permissions.sort();
        permissions.dedup();

        Ok(permissions)
    }
}
