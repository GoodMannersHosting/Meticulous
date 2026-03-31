//! User repository.

use chrono::Utc;
use met_core::ids::{OrganizationId, UserId};
use met_core::models::User;
use sqlx::PgPool;

use crate::error::{Result, StoreError};

/// Repository for user operations.
pub struct UserRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> UserRepo<'a> {
    /// Create a new user repository.
    #[must_use]
    pub const fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Create a new user with password hash.
    pub async fn create(
        &self,
        org_id: OrganizationId,
        username: &str,
        email: &str,
        display_name: Option<&str>,
        password_hash: Option<&str>,
        is_admin: bool,
    ) -> Result<User> {
        let id = UserId::new();
        let now = Utc::now();

        let user = sqlx::query_as::<_, User>(
            r#"
            INSERT INTO users (id, org_id, username, email, display_name, password_hash, is_active, is_admin, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, true, $7, $8, $8)
            RETURNING id, org_id, username, email, display_name, password_hash, is_active, is_admin, external_id, created_at, updated_at, deleted_at
            "#,
        )
        .bind(id.as_uuid())
        .bind(org_id.as_uuid())
        .bind(username)
        .bind(email)
        .bind(display_name)
        .bind(password_hash)
        .bind(is_admin)
        .bind(now)
        .fetch_one(self.pool)
        .await?;

        Ok(user)
    }

    /// Get a user by ID.
    pub async fn get(&self, id: UserId) -> Result<User> {
        sqlx::query_as::<_, User>(
            r#"
            SELECT id, org_id, username, email, display_name, password_hash, is_active, is_admin, external_id, created_at, updated_at, deleted_at
            FROM users
            WHERE id = $1 AND deleted_at IS NULL
            "#,
        )
        .bind(id.as_uuid())
        .fetch_optional(self.pool)
        .await?
        .ok_or_else(|| StoreError::not_found("user", id))
    }

    /// Get a user by username within an organization.
    pub async fn get_by_username(&self, org_id: OrganizationId, username: &str) -> Result<Option<User>> {
        let user = sqlx::query_as::<_, User>(
            r#"
            SELECT id, org_id, username, email, display_name, password_hash, is_active, is_admin, external_id, created_at, updated_at, deleted_at
            FROM users
            WHERE org_id = $1 AND username = $2 AND deleted_at IS NULL
            "#,
        )
        .bind(org_id.as_uuid())
        .bind(username)
        .fetch_optional(self.pool)
        .await?;

        Ok(user)
    }

    /// Get a user by email within an organization.
    pub async fn get_by_email(&self, org_id: OrganizationId, email: &str) -> Result<Option<User>> {
        let user = sqlx::query_as::<_, User>(
            r#"
            SELECT id, org_id, username, email, display_name, password_hash, is_active, is_admin, external_id, created_at, updated_at, deleted_at
            FROM users
            WHERE org_id = $1 AND email = $2 AND deleted_at IS NULL
            "#,
        )
        .bind(org_id.as_uuid())
        .bind(email)
        .fetch_optional(self.pool)
        .await?;

        Ok(user)
    }

    /// List all active users in an organization.
    pub async fn list(&self, org_id: OrganizationId, limit: i64, offset: i64) -> Result<Vec<User>> {
        let users = sqlx::query_as::<_, User>(
            r#"
            SELECT id, org_id, username, email, display_name, password_hash, is_active, is_admin, external_id, created_at, updated_at, deleted_at
            FROM users
            WHERE org_id = $1 AND deleted_at IS NULL
            ORDER BY created_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(org_id.as_uuid())
        .bind(limit)
        .bind(offset)
        .fetch_all(self.pool)
        .await?;

        Ok(users)
    }

    /// Update a user's password hash.
    pub async fn update_password(&self, id: UserId, password_hash: &str) -> Result<()> {
        let result = sqlx::query(
            r#"
            UPDATE users
            SET password_hash = $2, updated_at = NOW()
            WHERE id = $1 AND deleted_at IS NULL
            "#,
        )
        .bind(id.as_uuid())
        .bind(password_hash)
        .execute(self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(StoreError::not_found("user", id));
        }

        Ok(())
    }

    /// Count all users (across all organizations).
    pub async fn count_all(&self) -> Result<i64> {
        let (count,): (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*)
            FROM users
            WHERE deleted_at IS NULL
            "#,
        )
        .fetch_one(self.pool)
        .await?;

        Ok(count)
    }

    /// Count users in an organization.
    pub async fn count(&self, org_id: OrganizationId) -> Result<i64> {
        let (count,): (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*)
            FROM users
            WHERE org_id = $1 AND deleted_at IS NULL
            "#,
        )
        .bind(org_id.as_uuid())
        .fetch_one(self.pool)
        .await?;

        Ok(count)
    }

    /// Check if any users exist in the system (for initial setup).
    pub async fn any_users_exist(&self) -> Result<bool> {
        let (exists,): (bool,) = sqlx::query_as(
            r#"
            SELECT EXISTS(SELECT 1 FROM users WHERE deleted_at IS NULL)
            "#,
        )
        .fetch_one(self.pool)
        .await?;

        Ok(exists)
    }

    /// Check if any organizations exist in the system (for initial setup).
    pub async fn any_orgs_exist(&self) -> Result<bool> {
        let (exists,): (bool,) = sqlx::query_as(
            r#"
            SELECT EXISTS(SELECT 1 FROM organizations WHERE deleted_at IS NULL)
            "#,
        )
        .fetch_one(self.pool)
        .await?;

        Ok(exists)
    }
}
