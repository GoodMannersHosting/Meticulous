//! Cache entry repository.

use chrono::{DateTime, Utc};
use met_core::ids::ProjectId;
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{Result, StoreError};

/// Cache entry model.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CacheEntry {
    pub id: Uuid,
    pub project_id: Uuid,
    pub cache_key: String,
    pub storage_path: String,
    pub size_bytes: i64,
    pub compression: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_hit_at: DateTime<Utc>,
    pub hit_count: i32,
    pub expires_at: Option<DateTime<Utc>>,
    pub metadata: Option<serde_json::Value>,
}

/// Repository for cache entry operations.
pub struct CacheEntryRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> CacheEntryRepo<'a> {
    /// Create a new cache entry repository.
    #[must_use]
    pub const fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Create or update a cache entry.
    pub async fn upsert(
        &self,
        project_id: ProjectId,
        cache_key: &str,
        storage_path: &str,
        size_bytes: i64,
        compression: Option<&str>,
        metadata: Option<serde_json::Value>,
    ) -> Result<CacheEntry> {
        let now = Utc::now();

        let entry = sqlx::query_as::<_, CacheEntry>(
            r#"
            INSERT INTO cache_entries (project_id, cache_key, storage_path, size_bytes, compression, metadata, created_at, last_hit_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $7)
            ON CONFLICT (project_id, cache_key) DO UPDATE SET
                storage_path = EXCLUDED.storage_path,
                size_bytes = EXCLUDED.size_bytes,
                compression = EXCLUDED.compression,
                metadata = EXCLUDED.metadata,
                created_at = $7,
                last_hit_at = $7,
                hit_count = 0
            RETURNING id, project_id, cache_key, storage_path, size_bytes, compression, 
                      created_at, last_hit_at, hit_count, expires_at, metadata
            "#,
        )
        .bind(project_id.as_uuid())
        .bind(cache_key)
        .bind(storage_path)
        .bind(size_bytes)
        .bind(compression)
        .bind(metadata)
        .bind(now)
        .fetch_one(self.pool)
        .await?;

        Ok(entry)
    }

    /// Lookup a cache entry by key.
    pub async fn lookup(&self, project_id: ProjectId, cache_key: &str) -> Result<Option<CacheEntry>> {
        let entry = sqlx::query_as::<_, CacheEntry>(
            r#"
            SELECT id, project_id, cache_key, storage_path, size_bytes, compression, 
                   created_at, last_hit_at, hit_count, expires_at, metadata
            FROM cache_entries
            WHERE project_id = $1 AND cache_key = $2 
                  AND (expires_at IS NULL OR expires_at > NOW())
            "#,
        )
        .bind(project_id.as_uuid())
        .bind(cache_key)
        .fetch_optional(self.pool)
        .await?;

        Ok(entry)
    }

    /// Record a cache hit (updates last_hit_at and increments hit_count).
    pub async fn record_hit(&self, project_id: ProjectId, cache_key: &str) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE cache_entries
            SET last_hit_at = NOW(), hit_count = hit_count + 1
            WHERE project_id = $1 AND cache_key = $2
            "#,
        )
        .bind(project_id.as_uuid())
        .bind(cache_key)
        .execute(self.pool)
        .await?;

        Ok(())
    }

    /// Lookup with prefix matching (for restore keys).
    pub async fn lookup_by_prefix(
        &self,
        project_id: ProjectId,
        key_prefix: &str,
    ) -> Result<Option<CacheEntry>> {
        let entry = sqlx::query_as::<_, CacheEntry>(
            r#"
            SELECT id, project_id, cache_key, storage_path, size_bytes, compression, 
                   created_at, last_hit_at, hit_count, expires_at, metadata
            FROM cache_entries
            WHERE project_id = $1 AND cache_key LIKE $2
                  AND (expires_at IS NULL OR expires_at > NOW())
            ORDER BY last_hit_at DESC
            LIMIT 1
            "#,
        )
        .bind(project_id.as_uuid())
        .bind(format!("{}%", key_prefix))
        .fetch_optional(self.pool)
        .await?;

        Ok(entry)
    }

    /// Delete a cache entry.
    pub async fn delete(&self, project_id: ProjectId, cache_key: &str) -> Result<bool> {
        let result = sqlx::query(
            r#"
            DELETE FROM cache_entries
            WHERE project_id = $1 AND cache_key = $2
            "#,
        )
        .bind(project_id.as_uuid())
        .bind(cache_key)
        .execute(self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// List cache entries for a project, ordered by last hit (LRU).
    pub async fn list_by_project_lru(
        &self,
        project_id: ProjectId,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<CacheEntry>> {
        let entries = sqlx::query_as::<_, CacheEntry>(
            r#"
            SELECT id, project_id, cache_key, storage_path, size_bytes, compression, 
                   created_at, last_hit_at, hit_count, expires_at, metadata
            FROM cache_entries
            WHERE project_id = $1
            ORDER BY last_hit_at ASC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(project_id.as_uuid())
        .bind(limit)
        .bind(offset)
        .fetch_all(self.pool)
        .await?;

        Ok(entries)
    }

    /// Get total cache size for a project.
    pub async fn total_size(&self, project_id: ProjectId) -> Result<i64> {
        let (total,): (Option<i64>,) = sqlx::query_as(
            r#"
            SELECT SUM(size_bytes)
            FROM cache_entries
            WHERE project_id = $1
            "#,
        )
        .bind(project_id.as_uuid())
        .fetch_one(self.pool)
        .await?;

        Ok(total.unwrap_or(0))
    }

    /// Evict oldest entries until total size is under quota.
    pub async fn evict_to_quota(&self, project_id: ProjectId, quota_bytes: i64) -> Result<i32> {
        let mut evicted = 0;
        
        loop {
            let total = self.total_size(project_id).await?;
            if total <= quota_bytes {
                break;
            }

            let oldest = self.list_by_project_lru(project_id, 1, 0).await?;
            if let Some(entry) = oldest.first() {
                self.delete(project_id, &entry.cache_key).await?;
                evicted += 1;
            } else {
                break;
            }
        }

        Ok(evicted)
    }

    /// Delete expired entries.
    pub async fn delete_expired(&self) -> Result<i64> {
        let result = sqlx::query(
            r#"
            DELETE FROM cache_entries
            WHERE expires_at IS NOT NULL AND expires_at < NOW()
            "#,
        )
        .execute(self.pool)
        .await?;

        Ok(result.rows_affected() as i64)
    }

    /// Delete entries older than a given duration.
    pub async fn delete_older_than(&self, project_id: ProjectId, older_than: DateTime<Utc>) -> Result<i64> {
        let result = sqlx::query(
            r#"
            DELETE FROM cache_entries
            WHERE project_id = $1 AND last_hit_at < $2
            "#,
        )
        .bind(project_id.as_uuid())
        .bind(older_than)
        .execute(self.pool)
        .await?;

        Ok(result.rows_affected() as i64)
    }
}
