//! Join token repository.

use met_core::ids::{JoinTokenId, OrganizationId, UserId};
use met_core::models::{JoinToken, JoinTokenScope};
use sqlx::PgPool;

use crate::error::{Result, StoreError};

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
        let created = sqlx::query_as::<_, JoinToken>(
            r#"
            INSERT INTO join_tokens (id, token_hash, scope, scope_id, max_uses, current_uses, labels, pool_tags, expires_at, revoked, created_by, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            RETURNING id, token_hash, scope, scope_id, max_uses, current_uses, labels, pool_tags, expires_at, revoked, created_by, created_at, updated_at
            "#,
        )
        .bind(token.id.as_uuid())
        .bind(&token.token_hash)
        .bind(&token.scope)
        .bind(token.scope_id)
        .bind(token.max_uses)
        .bind(token.current_uses)
        .bind(&token.labels)
        .bind(&token.pool_tags)
        .bind(token.expires_at)
        .bind(token.revoked)
        .bind(token.created_by.as_uuid())
        .bind(token.created_at)
        .bind(token.updated_at)
        .fetch_one(self.pool)
        .await?;

        Ok(created)
    }

    /// Get a join token by ID.
    pub async fn get(&self, id: JoinTokenId) -> Result<JoinToken> {
        sqlx::query_as::<_, JoinToken>(
            r#"
            SELECT id, token_hash, scope, scope_id, max_uses, current_uses, labels, pool_tags, expires_at, revoked, created_by, created_at, updated_at
            FROM join_tokens
            WHERE id = $1
            "#,
        )
        .bind(id.as_uuid())
        .fetch_optional(self.pool)
        .await?
        .ok_or_else(|| StoreError::not_found("join_token", id))
    }

    /// Find a join token by its hash.
    pub async fn find_by_hash(&self, token_hash: &str) -> Result<Option<JoinToken>> {
        let token = sqlx::query_as::<_, JoinToken>(
            r#"
            SELECT id, token_hash, scope, scope_id, max_uses, current_uses, labels, pool_tags, expires_at, revoked, created_by, created_at, updated_at
            FROM join_tokens
            WHERE token_hash = $1 AND NOT revoked
            "#,
        )
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
        let tokens = sqlx::query_as::<_, JoinToken>(
            r#"
            SELECT id, token_hash, scope, scope_id, max_uses, current_uses, labels, pool_tags, expires_at, revoked, created_by, created_at, updated_at
            FROM join_tokens
            WHERE scope = $1 AND (scope_id = $2 OR ($2 IS NULL AND scope_id IS NULL))
            ORDER BY created_at DESC
            LIMIT $3 OFFSET $4
            "#,
        )
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
        let tokens = sqlx::query_as::<_, JoinToken>(
            r#"
            SELECT id, token_hash, scope, scope_id, max_uses, current_uses, labels, pool_tags, expires_at, revoked, created_by, created_at, updated_at
            FROM join_tokens
            WHERE created_by = $1
            ORDER BY created_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
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

    /// Check if a token hash exists and is valid.
    pub async fn validate_token(&self, token_hash: &str) -> Result<Option<JoinToken>> {
        let token = sqlx::query_as::<_, JoinToken>(
            r#"
            SELECT id, token_hash, scope, scope_id, max_uses, current_uses, labels, pool_tags, expires_at, revoked, created_by, created_at, updated_at
            FROM join_tokens
            WHERE token_hash = $1
                AND NOT revoked
                AND (expires_at IS NULL OR expires_at > NOW())
                AND (max_uses IS NULL OR current_uses < max_uses)
            "#,
        )
        .bind(token_hash)
        .fetch_optional(self.pool)
        .await?;

        Ok(token)
    }

    /// Validate a join token hash, check expiry/revocation/max_uses, and atomically
    /// increment the use count. Returns the token if valid, or an error if the token
    /// is expired, revoked, exhausted, or not found.
    pub async fn validate_and_consume(&self, token_hash: &str) -> Result<JoinToken> {
        let token = sqlx::query_as::<_, JoinToken>(
            r#"
            UPDATE join_tokens
            SET current_uses = current_uses + 1,
                updated_at = NOW()
            WHERE token_hash = $1
                AND NOT revoked
                AND (expires_at IS NULL OR expires_at > NOW())
                AND (max_uses IS NULL OR current_uses < max_uses)
            RETURNING id, token_hash, scope, scope_id, max_uses, current_uses, labels, pool_tags, expires_at, revoked, created_by, created_at, updated_at
            "#,
        )
        .bind(token_hash)
        .fetch_optional(self.pool)
        .await?;

        match token {
            Some(t) => Ok(t),
            None => {
                // Distinguish between "not found" and "invalid" by checking existence
                let exists = sqlx::query_scalar::<_, bool>(
                    "SELECT EXISTS(SELECT 1 FROM join_tokens WHERE token_hash = $1)",
                )
                .bind(token_hash)
                .fetch_one(self.pool)
                .await?;

                if exists {
                    Err(StoreError::Constraint(
                        "join token is expired, revoked, or has reached max uses".into(),
                    ))
                } else {
                    Err(StoreError::not_found("join_token", token_hash))
                }
            }
        }
    }

    /// List all active (non-revoked, non-expired) join tokens with optional pagination.
    pub async fn list_active(&self, limit: i64, offset: i64) -> Result<Vec<JoinToken>> {
        let tokens = sqlx::query_as::<_, JoinToken>(
            r#"
            SELECT id, token_hash, scope, scope_id, max_uses, current_uses, labels, pool_tags, expires_at, revoked, created_by, created_at, updated_at
            FROM join_tokens
            WHERE NOT revoked
                AND (expires_at IS NULL OR expires_at > NOW())
                AND (max_uses IS NULL OR current_uses < max_uses)
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

    /// List tokens for an organization (by tenant scope).
    pub async fn list_by_org(
        &self,
        org_id: OrganizationId,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<JoinToken>> {
        let tokens = sqlx::query_as::<_, JoinToken>(
            r#"
            SELECT id, token_hash, scope, scope_id, max_uses, current_uses, labels, pool_tags, expires_at, revoked, created_by, created_at, updated_at
            FROM join_tokens
            WHERE scope = 'tenant' AND scope_id = $1
            ORDER BY created_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(org_id.as_uuid())
        .bind(limit)
        .bind(offset)
        .fetch_all(self.pool)
        .await?;

        Ok(tokens)
    }
}
