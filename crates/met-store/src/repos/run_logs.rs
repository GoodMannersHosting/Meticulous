//! Log storage repository for run execution logs.

use chrono::{DateTime, Utc};
use met_core::ids::{JobRunId, StepRunId};
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{Result, StoreError};

/// A single log entry for a job/step run.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct LogEntry {
    pub id: Uuid,
    pub job_run_id: Uuid,
    pub step_run_id: Option<Uuid>,
    pub sequence: i64,
    pub stream: String,
    pub content: String,
    pub created_at: DateTime<Utc>,
}

/// Repository for run log operations.
pub struct RunLogRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> RunLogRepo<'a> {
    /// Create a new run log repository.
    #[must_use]
    pub const fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Append a log entry.
    pub async fn append(
        &self,
        job_run_id: JobRunId,
        step_run_id: Option<StepRunId>,
        sequence: i64,
        stream: &str,
        content: &str,
    ) -> Result<LogEntry> {
        let id = Uuid::new_v4();
        let now = Utc::now();

        let entry = sqlx::query_as::<_, LogEntry>(
            r#"
            INSERT INTO run_logs (id, job_run_id, step_run_id, sequence, stream, content, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING id, job_run_id, step_run_id, sequence, stream, content, created_at
            "#,
        )
        .bind(id)
        .bind(job_run_id.as_uuid())
        .bind(step_run_id.map(|s| s.as_uuid()))
        .bind(sequence)
        .bind(stream)
        .bind(content)
        .bind(now)
        .fetch_one(self.pool)
        .await?;

        Ok(entry)
    }

    /// Append multiple log entries in a batch.
    pub async fn append_batch(
        &self,
        entries: &[(JobRunId, Option<StepRunId>, i64, String, String)],
    ) -> Result<usize> {
        if entries.is_empty() {
            return Ok(0);
        }

        let now = Utc::now();
        let mut count = 0usize;

        for (job_run_id, step_run_id, sequence, stream, content) in entries {
            let id = Uuid::new_v4();
            sqlx::query(
                r#"
                INSERT INTO run_logs (id, job_run_id, step_run_id, sequence, stream, content, created_at)
                VALUES ($1, $2, $3, $4, $5, $6, $7)
                "#,
            )
            .bind(id)
            .bind(job_run_id.as_uuid())
            .bind(step_run_id.map(|s| s.as_uuid()))
            .bind(sequence)
            .bind(stream)
            .bind(content)
            .bind(now)
            .execute(self.pool)
            .await?;
            count += 1;
        }

        Ok(count)
    }

    /// Get logs for a job run.
    pub async fn get_by_job_run(
        &self,
        job_run_id: JobRunId,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<LogEntry>> {
        let entries = sqlx::query_as::<_, LogEntry>(
            r#"
            SELECT id, job_run_id, step_run_id, sequence, stream, content, created_at
            FROM run_logs
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

        Ok(entries)
    }

    /// Get logs for a step run.
    pub async fn get_by_step_run(
        &self,
        step_run_id: StepRunId,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<LogEntry>> {
        let entries = sqlx::query_as::<_, LogEntry>(
            r#"
            SELECT id, job_run_id, step_run_id, sequence, stream, content, created_at
            FROM run_logs
            WHERE step_run_id = $1
            ORDER BY sequence ASC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(step_run_id.as_uuid())
        .bind(limit)
        .bind(offset)
        .fetch_all(self.pool)
        .await?;

        Ok(entries)
    }

    /// Get logs after a specific sequence number (for streaming).
    pub async fn get_after_sequence(
        &self,
        job_run_id: JobRunId,
        after_sequence: i64,
        limit: i64,
    ) -> Result<Vec<LogEntry>> {
        let entries = sqlx::query_as::<_, LogEntry>(
            r#"
            SELECT id, job_run_id, step_run_id, sequence, stream, content, created_at
            FROM run_logs
            WHERE job_run_id = $1 AND sequence > $2
            ORDER BY sequence ASC
            LIMIT $3
            "#,
        )
        .bind(job_run_id.as_uuid())
        .bind(after_sequence)
        .bind(limit)
        .fetch_all(self.pool)
        .await?;

        Ok(entries)
    }

    /// Count logs for a job run.
    pub async fn count_by_job_run(&self, job_run_id: JobRunId) -> Result<i64> {
        let (count,): (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*)
            FROM run_logs
            WHERE job_run_id = $1
            "#,
        )
        .bind(job_run_id.as_uuid())
        .fetch_one(self.pool)
        .await?;

        Ok(count)
    }

    /// Delete logs older than a certain date (for retention policy).
    pub async fn delete_older_than(&self, cutoff: DateTime<Utc>) -> Result<u64> {
        let result = sqlx::query(
            r#"
            DELETE FROM run_logs
            WHERE created_at < $1
            "#,
        )
        .bind(cutoff)
        .execute(self.pool)
        .await?;

        Ok(result.rows_affected())
    }
}
