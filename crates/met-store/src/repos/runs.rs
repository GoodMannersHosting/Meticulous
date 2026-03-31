//! Run repository.

use chrono::Utc;
use met_core::ids::{PipelineId, RunId, TriggerId};
use met_core::models::{Run, RunStatus};
use sqlx::PgPool;

use crate::error::{Result, StoreError};

/// Repository for run operations.
pub struct RunRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> RunRepo<'a> {
    /// Create a new run repository.
    #[must_use]
    pub const fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Create a new run.
    pub async fn create(
        &self,
        pipeline_id: PipelineId,
        trigger_id: Option<TriggerId>,
        triggered_by: &str,
    ) -> Result<Run> {
        let id = RunId::new();
        let now = Utc::now();

        // Get the next run number for this pipeline
        let run_number = self.next_run_number(pipeline_id).await?;

        let run = sqlx::query_as::<_, Run>(
            r#"
            INSERT INTO runs (id, pipeline_id, trigger_id, status, run_number, triggered_by, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING id, pipeline_id, trigger_id, status, run_number, commit_sha, branch, triggered_by, created_at, started_at, finished_at
            "#,
        )
        .bind(id.as_uuid())
        .bind(pipeline_id.as_uuid())
        .bind(trigger_id.map(|t| t.as_uuid()))
        .bind(RunStatus::Pending)
        .bind(run_number)
        .bind(triggered_by)
        .bind(now)
        .fetch_one(self.pool)
        .await?;

        Ok(run)
    }

    /// Get a run by ID.
    pub async fn get(&self, id: RunId) -> Result<Run> {
        sqlx::query_as::<_, Run>(
            r#"
            SELECT id, pipeline_id, trigger_id, status, run_number, commit_sha, branch, triggered_by, created_at, started_at, finished_at
            FROM runs
            WHERE id = $1
            "#,
        )
        .bind(id.as_uuid())
        .fetch_optional(self.pool)
        .await?
        .ok_or_else(|| StoreError::not_found("run", id))
    }

    /// List runs for a pipeline.
    pub async fn list_by_pipeline(
        &self,
        pipeline_id: PipelineId,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Run>> {
        let runs = sqlx::query_as::<_, Run>(
            r#"
            SELECT id, pipeline_id, trigger_id, status, run_number, commit_sha, branch, triggered_by, created_at, started_at, finished_at
            FROM runs
            WHERE pipeline_id = $1
            ORDER BY created_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(pipeline_id.as_uuid())
        .bind(limit)
        .bind(offset)
        .fetch_all(self.pool)
        .await?;

        Ok(runs)
    }

    /// Update run status.
    pub async fn update_status(&self, id: RunId, status: RunStatus) -> Result<Run> {
        let now = Utc::now();

        // Set started_at when transitioning to Running
        // Set finished_at when transitioning to a terminal status
        let run = sqlx::query_as::<_, Run>(
            r#"
            UPDATE runs
            SET 
                status = $2,
                started_at = CASE WHEN $2 = 'running' AND started_at IS NULL THEN $3 ELSE started_at END,
                finished_at = CASE WHEN $2 IN ('succeeded', 'failed', 'cancelled', 'timed_out') THEN $3 ELSE finished_at END
            WHERE id = $1
            RETURNING id, pipeline_id, trigger_id, status, run_number, commit_sha, branch, triggered_by, created_at, started_at, finished_at
            "#,
        )
        .bind(id.as_uuid())
        .bind(status)
        .bind(now)
        .fetch_one(self.pool)
        .await?;

        Ok(run)
    }

    /// Get the next run number for a pipeline.
    async fn next_run_number(&self, pipeline_id: PipelineId) -> Result<i64> {
        let (next,): (i64,) = sqlx::query_as(
            r#"
            SELECT COALESCE(MAX(run_number), 0) + 1
            FROM runs
            WHERE pipeline_id = $1
            "#,
        )
        .bind(pipeline_id.as_uuid())
        .fetch_one(self.pool)
        .await?;

        Ok(next)
    }

    /// Count runs for a pipeline.
    pub async fn count_by_pipeline(&self, pipeline_id: PipelineId) -> Result<i64> {
        let (count,): (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*)
            FROM runs
            WHERE pipeline_id = $1
            "#,
        )
        .bind(pipeline_id.as_uuid())
        .fetch_one(self.pool)
        .await?;

        Ok(count)
    }

    /// Count runs by status for a pipeline.
    pub async fn count_by_status(&self, pipeline_id: PipelineId, status: RunStatus) -> Result<i64> {
        let (count,): (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*)
            FROM runs
            WHERE pipeline_id = $1 AND status = $2
            "#,
        )
        .bind(pipeline_id.as_uuid())
        .bind(status)
        .fetch_one(self.pool)
        .await?;

        Ok(count)
    }
}
