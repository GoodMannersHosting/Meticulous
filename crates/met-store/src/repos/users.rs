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
        password_must_change: bool,
    ) -> Result<User> {
        let id = UserId::new();
        let now = Utc::now();

        let user = sqlx::query_as::<_, User>(
            r#"
            INSERT INTO users (id, org_id, username, email, display_name, password_hash, is_active, is_admin, password_must_change, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, true, $7, $8, $9, $9)
            RETURNING id, org_id, username, email, display_name, password_hash, is_active, is_admin, password_must_change, external_id, created_at, updated_at, last_login_at, deleted_at
            "#,
        )
        .bind(id.as_uuid())
        .bind(org_id.as_uuid())
        .bind(username)
        .bind(email)
        .bind(display_name)
        .bind(password_hash)
        .bind(is_admin)
        .bind(password_must_change)
        .bind(now)
        .fetch_one(self.pool)
        .await?;

        Ok(user)
    }

    /// Get a user by ID.
    pub async fn get(&self, id: UserId) -> Result<User> {
        sqlx::query_as::<_, User>(
            r#"
            SELECT id, org_id, username, email, display_name, password_hash, is_active, is_admin, password_must_change, external_id, created_at, updated_at, last_login_at, deleted_at
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
    pub async fn get_by_username(
        &self,
        org_id: OrganizationId,
        username: &str,
    ) -> Result<Option<User>> {
        let user = sqlx::query_as::<_, User>(
            r#"
            SELECT id, org_id, username, email, display_name, password_hash, is_active, is_admin, password_must_change, external_id, created_at, updated_at, last_login_at, deleted_at
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
            SELECT id, org_id, username, email, display_name, password_hash, is_active, is_admin, password_must_change, external_id, created_at, updated_at, last_login_at, deleted_at
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

    /// Get a user by email, including soft-deleted users.
    pub async fn get_by_email_including_deleted(
        &self,
        org_id: OrganizationId,
        email: &str,
    ) -> Result<Option<User>> {
        let user = sqlx::query_as::<_, User>(
            r#"
            SELECT id, org_id, username, email, display_name, password_hash, is_active, is_admin, password_must_change, external_id, created_at, updated_at, last_login_at, deleted_at
            FROM users
            WHERE org_id = $1 AND email = $2
            "#,
        )
        .bind(org_id.as_uuid())
        .bind(email)
        .fetch_optional(self.pool)
        .await?;

        Ok(user)
    }

    /// Restore a soft-deleted user by clearing their deleted_at timestamp.
    ///
    /// IMPORTANT: This explicitly sets is_admin = false as a security measure.
    /// Deleted users should lose all privileges and must be re-granted admin status
    /// manually after restoration. This is required for GDPR compliance and security.
    pub async fn restore(&self, id: UserId) -> Result<User> {
        let user = sqlx::query_as::<_, User>(
            r#"
            UPDATE users
            SET deleted_at = NULL, 
                is_active = true, 
                is_admin = false,
                updated_at = NOW()
            WHERE id = $1
            RETURNING id, org_id, username, email, display_name, password_hash, is_active, is_admin, password_must_change, external_id, created_at, updated_at, last_login_at, deleted_at
            "#,
        )
        .bind(id.as_uuid())
        .fetch_one(self.pool)
        .await?;

        Ok(user)
    }

    /// Record a successful interactive login (`last_login_at` and `updated_at` via trigger).
    pub async fn record_last_login(&self, id: UserId) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE users
            SET last_login_at = NOW()
            WHERE id = $1 AND deleted_at IS NULL
            "#,
        )
        .bind(id.as_uuid())
        .execute(self.pool)
        .await?;

        Ok(())
    }

    /// List all active users in an organization.
    pub async fn list(&self, org_id: OrganizationId, limit: i64, offset: i64) -> Result<Vec<User>> {
        let users = sqlx::query_as::<_, User>(
            r#"
            SELECT id, org_id, username, email, display_name, password_hash, is_active, is_admin, password_must_change, external_id, created_at, updated_at, last_login_at, deleted_at
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

    /// Update a user's password hash and clear the forced-change flag.
    pub async fn update_password(&self, id: UserId, password_hash: &str) -> Result<()> {
        let result = sqlx::query(
            r#"
            UPDATE users
            SET password_hash = $2, password_must_change = false, updated_at = NOW()
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

    /// True when the bootstrap admin (`username`) still has a pending forced password change.
    /// Used to decide whether the login UI may show default credential hints.
    pub async fn bootstrap_admin_pending_password_change(
        &self,
        bootstrap_username: &str,
    ) -> Result<bool> {
        let (pending,): (bool,) = sqlx::query_as(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM users
                WHERE deleted_at IS NULL
                  AND is_admin = true
                  AND password_must_change = true
                  AND username = $1
            )
            "#,
        )
        .bind(bootstrap_username)
        .fetch_one(self.pool)
        .await?;

        Ok(pending)
    }
}
