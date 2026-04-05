//! Join token repository.

use chrono::{DateTime, Utc};
use met_core::ids::{JoinTokenId, OrganizationId, UserId};
use met_core::models::{JoinToken, JoinTokenDescriptionHistory, JoinTokenScope};
use sqlx::PgPool;

use crate::error::{Result, StoreError};

const JOIN_TOKEN_ROW: &str = r#"
    id, token_hash, scope, scope_id, description, org_id, max_uses, current_uses,
    labels, pool_tags, expires_at, revoked, created_by, created_at, updated_at,
    consumed_by_agent_id, consumed_at
"#;

/// Repository for join token operations.
pub struct JoinTokenRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> JoinTokenRepo<'a> {
    /// Create a new join token repository.
    #[must_use]
    pub const fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Create a new join token.
    pub async fn create(&self, token: &JoinToken) -> Result<JoinToken> {
        let created = sqlx::query_as::<_, JoinToken>(&format!(
            r#"
            INSERT INTO join_tokens (
                id, token_hash, scope, scope_id, description, org_id, max_uses, current_uses,
                labels, pool_tags, expires_at, revoked, created_by, created_at, updated_at,
                consumed_by_agent_id, consumed_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)
            RETURNING {JOIN_TOKEN_ROW}
            "#,
        ))
        .bind(token.id.as_uuid())
        .bind(&token.token_hash)
        .bind(&token.scope)
        .bind(token.scope_id)
        .bind(&token.description)
        .bind(token.org_id.map(|o| o.as_uuid()))
        .bind(token.max_uses)
        .bind(token.current_uses)
        .bind(&token.labels)
        .bind(&token.pool_tags)
        .bind(token.expires_at)
        .bind(token.revoked)
        .bind(token.created_by.as_uuid())
        .bind(token.created_at)
        .bind(token.updated_at)
        .bind(token.consumed_by_agent_id.map(|a| a.as_uuid()))
        .bind(token.consumed_at)
        .fetch_one(self.pool)
        .await?;

        Ok(created)
    }

    /// Record a description in the audit trail (e.g. after create or update).
    pub async fn insert_description_history(
        &self,
        join_token_id: JoinTokenId,
        description: &str,
        changed_by: UserId,
        changed_at: DateTime<Utc>,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO join_token_description_history (join_token_id, description, changed_at, changed_by)
            VALUES ($1, $2, $3, $4)
            "#,
        )
        .bind(join_token_id.as_uuid())
        .bind(description)
        .bind(changed_at)
        .bind(changed_by.as_uuid())
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// Ordered history (oldest first) for display as a timeline.
    pub async fn list_description_history(
        &self,
        join_token_id: JoinTokenId,
    ) -> Result<Vec<JoinTokenDescriptionHistory>> {
        let rows = sqlx::query_as::<_, JoinTokenDescriptionHistory>(
            r#"
            SELECT id, join_token_id, description, changed_at, changed_by
            FROM join_token_description_history
            WHERE join_token_id = $1
            ORDER BY changed_at ASC
            "#,
        )
        .bind(join_token_id.as_uuid())
        .fetch_all(self.pool)
        .await?;
        Ok(rows)
    }

    /// Update the current description and append a history row (transactional).
    pub async fn update_description(
        &self,
        id: JoinTokenId,
        new_description: &str,
        changed_by: UserId,
    ) -> Result<JoinToken> {
        let mut tx = self.pool.begin().await?;
        let result = sqlx::query(
            r#"
            UPDATE join_tokens
            SET description = $1, updated_at = NOW()
            WHERE id = $2
            "#,
        )
        .bind(new_description)
        .bind(id.as_uuid())
        .execute(&mut *tx)
        .await?;
        if result.rows_affected() != 1 {
            tx.rollback().await.ok();
            return Err(StoreError::not_found("join_token", id));
        }
        sqlx::query(
            r#"
            INSERT INTO join_token_description_history (join_token_id, description, changed_at, changed_by)
            VALUES ($1, $2, NOW(), $3)
            "#,
        )
        .bind(id.as_uuid())
        .bind(new_description)
        .bind(changed_by.as_uuid())
        .execute(&mut *tx)
        .await?;
        let token = sqlx::query_as::<_, JoinToken>(&format!(
            r#"SELECT {JOIN_TOKEN_ROW} FROM join_tokens WHERE id = $1"#,
        ))
        .bind(id.as_uuid())
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(token)
    }

    /// Get a join token by ID.
    pub async fn get(&self, id: JoinTokenId) -> Result<JoinToken> {
        sqlx::query_as::<_, JoinToken>(&format!(
            r#"SELECT {JOIN_TOKEN_ROW} FROM join_tokens WHERE id = $1"#,
        ))
        .bind(id.as_uuid())
        .fetch_optional(self.pool)
        .await?
        .ok_or_else(|| StoreError::not_found("join_token", id))
    }

    /// Find a join token by its hash.
    pub async fn find_by_hash(&self, token_hash: &str) -> Result<Option<JoinToken>> {
        let token = sqlx::query_as::<_, JoinToken>(&format!(
            r#"SELECT {JOIN_TOKEN_ROW} FROM join_tokens WHERE token_hash = $1 AND NOT revoked"#,
        ))
        .bind(token_hash)
        .fetch_optional(self.pool)
        .await?;

        Ok(token)
    }

    /// List join tokens by scope.
    pub async fn list_by_scope(
        &self,
        scope: JoinTokenScope,
        scope_id: Option<uuid::Uuid>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<JoinToken>> {
        let tokens = sqlx::query_as::<_, JoinToken>(&format!(
            r#"
            SELECT {JOIN_TOKEN_ROW}
            FROM join_tokens
            WHERE scope = $1 AND (scope_id = $2 OR ($2 IS NULL AND scope_id IS NULL))
            ORDER BY created_at DESC
            LIMIT $3 OFFSET $4
            "#,
        ))
        .bind(scope)
        .bind(scope_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(self.pool)
        .await?;

        Ok(tokens)
    }

    /// List all join tokens created by a user.
    pub async fn list_by_creator(
        &self,
        created_by: UserId,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<JoinToken>> {
        let tokens = sqlx::query_as::<_, JoinToken>(&format!(
            r#"
            SELECT {JOIN_TOKEN_ROW}
            FROM join_tokens
            WHERE created_by = $1
            ORDER BY created_at DESC
            LIMIT $2 OFFSET $3
            "#,
        ))
        .bind(created_by.as_uuid())
        .bind(limit)
        .bind(offset)
        .fetch_all(self.pool)
        .await?;

        Ok(tokens)
    }

    /// Increment the usage count of a join token.
    pub async fn increment_usage(&self, id: JoinTokenId) -> Result<()> {
        let result = sqlx::query(
            r#"
            UPDATE join_tokens
            SET current_uses = current_uses + 1
            WHERE id = $1
            "#,
        )
        .bind(id.as_uuid())
        .execute(self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(StoreError::not_found("join_token", id));
        }

        Ok(())
    }

    /// Revoke a join token.
    pub async fn revoke(&self, id: JoinTokenId) -> Result<()> {
        let result = sqlx::query(
            r#"
            UPDATE join_tokens
            SET revoked = true
            WHERE id = $1
            "#,
        )
        .bind(id.as_uuid())
        .execute(self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(StoreError::not_found("join_token", id));
        }

        Ok(())
    }

    /// Delete expired tokens (cleanup job).
    pub async fn delete_expired(&self) -> Result<u64> {
        let result = sqlx::query(
            r#"
            DELETE FROM join_tokens
            WHERE expires_at IS NOT NULL AND expires_at < NOW()
            "#,
        )
        .execute(self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    /// Load a join token row by hash, including revoked / expired / exhausted tokens.
    pub async fn get_by_token_hash(&self, token_hash: &str) -> Result<Option<JoinToken>> {
        let row = sqlx::query_as::<_, JoinToken>(&format!(
            r#"
            SELECT {JOIN_TOKEN_ROW}
            FROM join_tokens
            WHERE token_hash = $1
            "#,
        ))
        .bind(token_hash)
        .fetch_optional(self.pool)
        .await?;
        Ok(row)
    }

    /// Check if a token hash exists and is valid.
    pub async fn validate_token(&self, token_hash: &str) -> Result<Option<JoinToken>> {
        let token = sqlx::query_as::<_, JoinToken>(&format!(
            r#"
            SELECT {JOIN_TOKEN_ROW}
            FROM join_tokens
            WHERE token_hash = $1
                AND NOT revoked
                AND (expires_at IS NULL OR expires_at > NOW())
                AND current_uses < max_uses
            "#,
        ))
        .bind(token_hash)
        .fetch_optional(self.pool)
        .await?;

        Ok(token)
    }

    /// List all active (non-revoked, non-expired, not exhausted) join tokens with optional pagination.
    pub async fn list_active(&self, limit: i64, offset: i64) -> Result<Vec<JoinToken>> {
        let tokens = sqlx::query_as::<_, JoinToken>(&format!(
            r#"
            SELECT {JOIN_TOKEN_ROW}
            FROM join_tokens
            WHERE NOT revoked
                AND (expires_at IS NULL OR expires_at > NOW())
                AND current_uses < max_uses
            ORDER BY created_at DESC
            LIMIT $1 OFFSET $2
            "#,
        ))
        .bind(limit)
        .bind(offset)
        .fetch_all(self.pool)
        .await?;

        Ok(tokens)
    }

    /// List tokens for an organization (by tenant scope).
    pub async fn list_by_org(
        &self,
        org_id: OrganizationId,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<JoinToken>> {
        self.list_by_org_filtered(org_id, None, limit, offset).await
    }

    /// Count tenant-scoped tokens for an org with optional search on description or token hash.
    pub async fn count_by_org_filtered(
        &self,
        org_id: OrganizationId,
        search: Option<&str>,
    ) -> Result<i64> {
        let pattern: Option<String> = search
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| format!("%{s}%"));

        let row: (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*)::bigint
            FROM join_tokens
            WHERE scope = 'tenant' AND scope_id = $1
              AND ($2::text IS NULL OR description ILIKE $2 OR token_hash ILIKE $2)
            "#,
        )
        .bind(org_id.as_uuid())
        .bind(pattern)
        .fetch_one(self.pool)
        .await?;

        Ok(row.0)
    }

    /// List tenant-scoped tokens for an org with optional search and pagination.
    pub async fn list_by_org_filtered(
        &self,
        org_id: OrganizationId,
        search: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<JoinToken>> {
        let pattern: Option<String> = search
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| format!("%{s}%"));

        let tokens = sqlx::query_as::<_, JoinToken>(&format!(
            r#"
            SELECT {JOIN_TOKEN_ROW}
            FROM join_tokens
            WHERE scope = 'tenant' AND scope_id = $1
              AND ($2::text IS NULL OR description ILIKE $2 OR token_hash ILIKE $2)
            ORDER BY created_at DESC
            LIMIT $3 OFFSET $4
            "#,
        ))
        .bind(org_id.as_uuid())
        .bind(pattern)
        .bind(limit)
        .bind(offset)
        .fetch_all(self.pool)
        .await?;

        Ok(tokens)
    }

    /// Permanently delete a join token row.
    ///
    /// Clears `agents.join_token_id` first so delete succeeds even when the database predates
    /// migration `012_agents_join_token_on_delete_set_null` (default FK blocks delete).
    pub async fn delete_by_id(&self, id: JoinTokenId) -> Result<()> {
        let mut tx = self.pool.begin().await?;

        sqlx::query(
            r#"
            UPDATE agents
            SET join_token_id = NULL
            WHERE join_token_id = $1
            "#,
        )
        .bind(id.as_uuid())
        .execute(&mut *tx)
        .await?;

        let result = sqlx::query(r#"DELETE FROM join_tokens WHERE id = $1"#)
            .bind(id.as_uuid())
            .execute(&mut *tx)
            .await?;

        if result.rows_affected() == 0 {
            tx.rollback().await?;
            return Err(StoreError::not_found("join_token", id));
        }

        tx.commit().await?;
        Ok(())
    }
}
