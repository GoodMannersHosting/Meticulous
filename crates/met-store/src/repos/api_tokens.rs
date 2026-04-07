//! API token repository.

use chrono::{Duration, Utc};
use met_core::ids::{ApiTokenId, ProjectId, UserId};
use met_core::models::{ApiToken, CreateApiToken};
use sqlx::PgPool;

use crate::error::{Result, StoreError};

/// Repository for API token operations.
pub struct ApiTokenRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> ApiTokenRepo<'a> {
    /// Create a new API token repository.
    #[must_use]
    pub const fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Create a new API token.
    /// Returns the token with the plain token value that should be shown to the user once.
    pub async fn create(
        &self,
        user_id: UserId,
        input: &CreateApiToken,
        token_hash: &str,
        prefix: &str,
    ) -> Result<ApiToken> {
        let id = ApiTokenId::new();
        let now = Utc::now();
        let expires_at = input.expires_in.map(|secs| now + Duration::seconds(secs));

        let token = sqlx::query_as::<_, ApiToken>(
            r#"
            INSERT INTO api_tokens (id, user_id, name, description, token_hash, prefix, scopes, project_ids, expires_at, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            RETURNING id, user_id, name, description, token_hash, prefix, scopes, project_ids, expires_at, last_used_at, revoked_at, created_at
            "#,
        )
        .bind(id.as_uuid())
        .bind(user_id.as_uuid())
        .bind(&input.name)
        .bind(&input.description)
        .bind(token_hash)
        .bind(prefix)
        .bind(&input.scopes)
        .bind(input.project_ids.as_ref().map(|ids| ids.iter().map(|id| id.as_uuid()).collect::<Vec<_>>()))
        .bind(expires_at)
        .bind(now)
        .fetch_one(self.pool)
        .await?;

        Ok(token)
    }

    /// Get a token by ID.
    pub async fn get(&self, id: ApiTokenId) -> Result<ApiToken> {
        sqlx::query_as::<_, ApiToken>(
            r#"
            SELECT id, user_id, name, description, token_hash, prefix, scopes, project_ids, expires_at, last_used_at, revoked_at, created_at
            FROM api_tokens
            WHERE id = $1
            "#,
        )
        .bind(id.as_uuid())
        .fetch_optional(self.pool)
        .await?
        .ok_or_else(|| StoreError::not_found("api_token", id))
    }

    /// Get a token by its hash.
    pub async fn get_by_hash(&self, token_hash: &str) -> Result<Option<ApiToken>> {
        let token = sqlx::query_as::<_, ApiToken>(
            r#"
            SELECT id, user_id, name, description, token_hash, prefix, scopes, project_ids, expires_at, last_used_at, revoked_at, created_at
            FROM api_tokens
            WHERE token_hash = $1 AND revoked_at IS NULL
            "#,
        )
        .bind(token_hash)
        .fetch_optional(self.pool)
        .await?;

        Ok(token)
    }

    /// Get a token by prefix (for display purposes).
    pub async fn get_by_prefix(&self, prefix: &str) -> Result<Option<ApiToken>> {
        let token = sqlx::query_as::<_, ApiToken>(
            r#"
            SELECT id, user_id, name, description, token_hash, prefix, scopes, project_ids, expires_at, last_used_at, revoked_at, created_at
            FROM api_tokens
            WHERE prefix = $1 AND revoked_at IS NULL
            "#,
        )
        .bind(prefix)
        .fetch_optional(self.pool)
        .await?;

        Ok(token)
    }

    /// List tokens for a user.
    pub async fn list_by_user(
        &self,
        user_id: UserId,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ApiToken>> {
        let tokens = sqlx::query_as::<_, ApiToken>(
            r#"
            SELECT id, user_id, name, description, token_hash, prefix, scopes, project_ids, expires_at, last_used_at, revoked_at, created_at
            FROM api_tokens
            WHERE user_id = $1 AND revoked_at IS NULL
            ORDER BY created_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(user_id.as_uuid())
        .bind(limit)
        .bind(offset)
        .fetch_all(self.pool)
        .await?;

        Ok(tokens)
    }

    /// List all active tokens (for admin).
    pub async fn list_all(&self, limit: i64, offset: i64) -> Result<Vec<ApiToken>> {
        let tokens = sqlx::query_as::<_, ApiToken>(
            r#"
            SELECT id, user_id, name, description, token_hash, prefix, scopes, project_ids, expires_at, last_used_at, revoked_at, created_at
            FROM api_tokens
            WHERE revoked_at IS NULL
            ORDER BY created_at DESC
            LIMIT $1 OFFSET $2
            "#,
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(self.pool)
        .await?;

        Ok(tokens)
    }

    /// Update last used timestamp.
    pub async fn touch(&self, id: ApiTokenId) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE api_tokens SET last_used_at = NOW() WHERE id = $1
            "#,
        )
        .bind(id.as_uuid())
        .execute(self.pool)
        .await?;

        Ok(())
    }

    /// Revoke a token.
    pub async fn revoke(&self, id: ApiTokenId) -> Result<()> {
        let result = sqlx::query(
            r#"
            UPDATE api_tokens SET revoked_at = NOW() WHERE id = $1 AND revoked_at IS NULL
            "#,
        )
        .bind(id.as_uuid())
        .execute(self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(StoreError::not_found("api_token", id));
        }

        Ok(())
    }

    /// Revoke all tokens for a user.
    pub async fn revoke_all_for_user(&self, user_id: UserId) -> Result<u64> {
        let result = sqlx::query(
            r#"
            UPDATE api_tokens SET revoked_at = NOW() WHERE user_id = $1 AND revoked_at IS NULL
            "#,
        )
        .bind(user_id.as_uuid())
        .execute(self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    /// Delete a token permanently.
    pub async fn delete(&self, id: ApiTokenId) -> Result<()> {
        let result = sqlx::query(
            r#"
            DELETE FROM api_tokens WHERE id = $1
            "#,
        )
        .bind(id.as_uuid())
        .execute(self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(StoreError::not_found("api_token", id));
        }

        Ok(())
    }

    /// Count tokens for a user.
    pub async fn count_by_user(&self, user_id: UserId) -> Result<i64> {
        let (count,): (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*) FROM api_tokens WHERE user_id = $1 AND revoked_at IS NULL
            "#,
        )
        .bind(user_id.as_uuid())
        .fetch_one(self.pool)
        .await?;

        Ok(count)
    }

    /// List tokens that can access a specific project.
    pub async fn list_by_project(
        &self,
        project_id: ProjectId,
        limit: i64,
    ) -> Result<Vec<ApiToken>> {
        let tokens = sqlx::query_as::<_, ApiToken>(
            r#"
            SELECT id, user_id, name, description, token_hash, prefix, scopes, project_ids, expires_at, last_used_at, revoked_at, created_at
            FROM api_tokens
            WHERE revoked_at IS NULL
              AND (project_ids IS NULL OR $1 = ANY(project_ids))
            ORDER BY created_at DESC
            LIMIT $2
            "#,
        )
        .bind(project_id.as_uuid())
        .bind(limit)
        .fetch_all(self.pool)
        .await?;

        Ok(tokens)
    }

    /// Remove a project from all tokens (when project is deleted).
    pub async fn remove_project_from_all(&self, project_id: ProjectId) -> Result<u64> {
        let result = sqlx::query(
            r#"
            UPDATE api_tokens
            SET project_ids = array_remove(project_ids, $1)
            WHERE project_ids IS NOT NULL AND $1 = ANY(project_ids)
            "#,
        )
        .bind(project_id.as_uuid())
        .execute(self.pool)
        .await?;

        Ok(result.rows_affected())
    }
}
