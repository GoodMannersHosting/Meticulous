//! PostgreSQL cache for job logs (24h TTL when lazy-loaded; NULL TTL while job is active).

use chrono::{DateTime, Duration, Utc};
use met_core::ids::{JobRunId, ProjectId, RunId, StepRunId};
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{Result, StoreError};

/// One cached log line.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct LogCacheEntry {
    pub job_run_id: Uuid,
    pub sequence: i64,
    pub timestamp: DateTime<Utc>,
    pub stream: String,
    pub content: String,
    pub run_id: Uuid,
    pub step_run_id: Option<Uuid>,
    pub cached_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

/// Archived log metadata (object stored in SeaweedFS / S3).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct LogArchiveRow {
    pub id: Uuid,
    pub job_run_id: Uuid,
    pub run_id: Uuid,
    pub project_id: Uuid,
    pub storage_key: String,
    pub line_count: i64,
    pub size_bytes: i64,
    pub compressed: bool,
    pub archived_at: DateTime<Utc>,
    pub sha256_checksum: Option<String>,
}

/// Repository for `log_cache` and `log_archives`.
/// One line to insert when rehydrating cache from object storage.
#[derive(Debug, Clone)]
pub struct LazyCacheLine {
    pub job_run_id: JobRunId,
    pub run_id: RunId,
    pub step_run_id: Option<StepRunId>,
    pub sequence: i64,
    pub stream: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
}

pub struct LogCacheRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> LogCacheRepo<'a> {
    #[must_use]
    pub const fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Append one line during live streaming (`expires_at` = NULL until archived).
    pub async fn append_streaming(
        &self,
        job_run_id: JobRunId,
        run_id: RunId,
        step_run_id: Option<StepRunId>,
        sequence: i64,
        stream: &str,
        content: &str,
        line_ts: DateTime<Utc>,
    ) -> Result<LogCacheEntry> {
        let row = sqlx::query_as::<_, LogCacheEntry>(
            r#"
            INSERT INTO log_cache (job_run_id, sequence, timestamp, stream, content, run_id, step_run_id, cached_at, expires_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, NOW(), NULL)
            ON CONFLICT (job_run_id, sequence) DO UPDATE SET
                timestamp = EXCLUDED.timestamp,
                stream = EXCLUDED.stream,
                content = EXCLUDED.content,
                run_id = EXCLUDED.run_id,
                step_run_id = EXCLUDED.step_run_id,
                cached_at = NOW()
            RETURNING job_run_id, sequence, timestamp, stream, content, run_id, step_run_id, cached_at, expires_at
            "#,
        )
        .bind(job_run_id.as_uuid())
        .bind(sequence)
        .bind(line_ts)
        .bind(stream)
        .bind(content)
        .bind(run_id.as_uuid())
        .bind(step_run_id.map(|s| s.as_uuid()))
        .fetch_one(self.pool)
        .await?;

        Ok(row)
    }

    /// Fetch lines for a job run (ordered by sequence).
    pub async fn list_for_job_run(
        &self,
        job_run_id: JobRunId,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<LogCacheEntry>> {
        let rows = sqlx::query_as::<_, LogCacheEntry>(
            r#"
            SELECT job_run_id, sequence, timestamp, stream, content, run_id, step_run_id, cached_at, expires_at
            FROM log_cache
            WHERE job_run_id = $1
            ORDER BY sequence ASC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(job_run_id.as_uuid())
        .bind(limit)
        .bind(offset)
        .fetch_all(self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn get_all_for_job_run(&self, job_run_id: JobRunId) -> Result<Vec<LogCacheEntry>> {
        let rows = sqlx::query_as::<_, LogCacheEntry>(
            r#"
            SELECT job_run_id, sequence, timestamp, stream, content, run_id, step_run_id, cached_at, expires_at
            FROM log_cache
            WHERE job_run_id = $1
            ORDER BY sequence ASC
            "#,
        )
        .bind(job_run_id.as_uuid())
        .fetch_all(self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn count_for_job_run(&self, job_run_id: JobRunId) -> Result<i64> {
        let (n,): (i64,) = sqlx::query_as(
            r#"SELECT COUNT(*)::bigint FROM log_cache WHERE job_run_id = $1"#,
        )
        .bind(job_run_id.as_uuid())
        .fetch_one(self.pool)
        .await?;

        Ok(n)
    }

    /// Remove all cached lines for a job after successful archival.
    pub async fn delete_for_job_run(&self, job_run_id: JobRunId) -> Result<u64> {
        let r = sqlx::query(r#"DELETE FROM log_cache WHERE job_run_id = $1"#)
            .bind(job_run_id.as_uuid())
            .execute(self.pool)
            .await?;

        Ok(r.rows_affected())
    }

    /// Replace cache with lines loaded from object storage; each row gets `expires_at = now + 24h`.
    pub async fn bulk_insert_lazy(&self, lines: &[LazyCacheLine]) -> Result<usize> {
        if lines.is_empty() {
            return Ok(0);
        }

        let exp = Utc::now() + Duration::hours(24);
        let mut n = 0usize;
        for line in lines {
            sqlx::query(
                r#"
                INSERT INTO log_cache (job_run_id, sequence, timestamp, stream, content, run_id, step_run_id, cached_at, expires_at)
                VALUES ($1, $2, $3, $4, $5, $6, $7, NOW(), $8)
                ON CONFLICT (job_run_id, sequence) DO UPDATE SET
                    timestamp = EXCLUDED.timestamp,
                    stream = EXCLUDED.stream,
                    content = EXCLUDED.content,
                    run_id = EXCLUDED.run_id,
                    step_run_id = EXCLUDED.step_run_id,
                    cached_at = NOW(),
                    expires_at = EXCLUDED.expires_at
                "#,
            )
            .bind(line.job_run_id.as_uuid())
            .bind(line.sequence)
            .bind(line.timestamp)
            .bind(&line.stream)
            .bind(&line.content)
            .bind(line.run_id.as_uuid())
            .bind(line.step_run_id.map(|s| s.as_uuid()))
            .bind(exp)
            .execute(self.pool)
            .await?;
            n += 1;
        }

        Ok(n)
    }

    /// When no object store is configured, retain PG cache for 24h after the job ends.
    pub async fn touch_ttl_no_store(&self, job_run_id: JobRunId) -> Result<u64> {
        let r = sqlx::query(
            r#"
            UPDATE log_cache
            SET expires_at = NOW() + interval '24 hours'
            WHERE job_run_id = $1 AND expires_at IS NULL
            "#,
        )
        .bind(job_run_id.as_uuid())
        .execute(self.pool)
        .await?;

        Ok(r.rows_affected())
    }

    /// Purge rows past TTL (lazy-loaded cache only).
    pub async fn delete_expired(&self) -> Result<u64> {
        let r = sqlx::query(
            r#"DELETE FROM log_cache WHERE expires_at IS NOT NULL AND expires_at < NOW()"#,
        )
        .execute(self.pool)
        .await?;

        Ok(r.rows_affected())
    }

    pub async fn insert_archive(
        &self,
        job_run_id: JobRunId,
        run_id: RunId,
        project_id: ProjectId,
        storage_key: &str,
        line_count: i64,
        size_bytes: i64,
        compressed: bool,
        sha256_checksum: Option<&str>,
    ) -> Result<LogArchiveRow> {
        let row = sqlx::query_as::<_, LogArchiveRow>(
            r#"
            INSERT INTO log_archives (job_run_id, run_id, project_id, storage_key, line_count, size_bytes, compressed, sha256_checksum)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (job_run_id) DO UPDATE SET
                run_id = EXCLUDED.run_id,
                project_id = EXCLUDED.project_id,
                storage_key = EXCLUDED.storage_key,
                line_count = EXCLUDED.line_count,
                size_bytes = EXCLUDED.size_bytes,
                compressed = EXCLUDED.compressed,
                sha256_checksum = EXCLUDED.sha256_checksum,
                archived_at = NOW()
            RETURNING id, job_run_id, run_id, project_id, storage_key, line_count, size_bytes, compressed, archived_at, sha256_checksum
            "#,
        )
        .bind(job_run_id.as_uuid())
        .bind(run_id.as_uuid())
        .bind(project_id.as_uuid())
        .bind(storage_key)
        .bind(line_count)
        .bind(size_bytes)
        .bind(compressed)
        .bind(sha256_checksum)
        .fetch_one(self.pool)
        .await?;

        Ok(row)
    }

    pub async fn get_archive_by_job_run(
        &self,
        job_run_id: JobRunId,
    ) -> Result<Option<LogArchiveRow>> {
        let row = sqlx::query_as::<_, LogArchiveRow>(
            r#"
            SELECT id, job_run_id, run_id, project_id, storage_key, line_count, size_bytes, compressed, archived_at, sha256_checksum
            FROM log_archives
            WHERE job_run_id = $1
            "#,
        )
        .bind(job_run_id.as_uuid())
        .fetch_optional(self.pool)
        .await?;

        Ok(row)
    }
}

/// Resolve `project_id` and `run_id` for archival.
pub async fn project_run_for_job_run(
    pool: &PgPool,
    job_run_id: JobRunId,
) -> Result<(RunId, ProjectId)> {
    let row: Option<(Uuid, Uuid)> = sqlx::query_as(
        r#"
        SELECT r.id, p.project_id
        FROM job_runs jr
        JOIN runs r ON jr.run_id = r.id
        JOIN pipelines p ON r.pipeline_id = p.id
        WHERE jr.id = $1
        "#,
    )
    .bind(job_run_id.as_uuid())
    .fetch_optional(pool)
    .await?;

    row.map(|(r, p)| (RunId::from_uuid(r), ProjectId::from_uuid(p)))
        .ok_or_else(|| StoreError::not_found("job_run", job_run_id))
}
