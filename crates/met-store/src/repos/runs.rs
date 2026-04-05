//! Run repository.

use chrono::{DateTime, Utc};
use met_core::ids::{AgentId, JobId, JobRunId, OrganizationId, PipelineId, RunId, StepId, StepRunId, TriggerId};
use met_core::models::{JobRun, JobStatus, Run, RunStatus, StepRun};
use sqlx::PgPool;
use uuid::Uuid;

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

    /// Create a new run with full options.
    pub async fn create_full(
        &self,
        pipeline_id: PipelineId,
        org_id: OrganizationId,
        trigger_id: Option<TriggerId>,
        triggered_by: &str,
        trace_id: Option<Uuid>,
        commit_sha: Option<&str>,
        branch: Option<&str>,
        trigger_data: Option<serde_json::Value>,
    ) -> Result<Run> {
        let id = RunId::new();
        let now = Utc::now();
        let run_number = self.next_run_number(pipeline_id).await?;

        let run = sqlx::query_as::<_, Run>(
            r#"
            INSERT INTO runs (id, pipeline_id, org_id, trigger_id, status, run_number, triggered_by, 
                              trace_id, commit_sha, branch, trigger_data, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            RETURNING id, pipeline_id, trigger_id, status, run_number, commit_sha, branch, 
                      triggered_by, created_at, started_at, finished_at
            "#,
        )
        .bind(id.as_uuid())
        .bind(pipeline_id.as_uuid())
        .bind(org_id.as_uuid())
        .bind(trigger_id.map(|t| t.as_uuid()))
        .bind(RunStatus::Pending)
        .bind(run_number)
        .bind(triggered_by)
        .bind(trace_id)
        .bind(commit_sha)
        .bind(branch)
        .bind(trigger_data)
        .bind(now)
        .fetch_one(self.pool)
        .await?;

        Ok(run)
    }

    /// Create a new run (simple version for backward compatibility).
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

    /// Get run with all job runs.
    pub async fn get_with_jobs(&self, id: RunId) -> Result<RunWithJobs> {
        let run = self.get(id).await?;
        
        let job_runs = sqlx::query_as::<_, JobRun>(
            r#"
            SELECT id, run_id, job_id, job_name, agent_id, status, attempt, exit_code, 
                   error_message, cache_hit, cache_key, outputs, started_at, finished_at, created_at
            FROM job_runs
            WHERE run_id = $1
            ORDER BY created_at
            "#,
        )
        .bind(id.as_uuid())
        .fetch_all(self.pool)
        .await?;

        Ok(RunWithJobs { run, job_runs })
    }

    /// Set run to running status.
    pub async fn start_run(&self, id: RunId) -> Result<Run> {
        let now = Utc::now();
        
        let run = sqlx::query_as::<_, Run>(
            r#"
            UPDATE runs
            SET status = 'running', started_at = $2
            WHERE id = $1 AND status IN ('pending', 'queued')
            RETURNING id, pipeline_id, trigger_id, status, run_number, commit_sha, branch, 
                      triggered_by, created_at, started_at, finished_at
            "#,
        )
        .bind(id.as_uuid())
        .bind(now)
        .fetch_one(self.pool)
        .await?;

        Ok(run)
    }

    /// Complete a run with final status.
    pub async fn complete_run(&self, id: RunId, status: RunStatus, error_message: Option<&str>) -> Result<Run> {
        let now = Utc::now();
        
        let run = sqlx::query_as::<_, Run>(
            r#"
            UPDATE runs
            SET status = $2, finished_at = $3, error_message = $4
            WHERE id = $1
            RETURNING id, pipeline_id, trigger_id, status, run_number, commit_sha, branch, 
                      triggered_by, created_at, started_at, finished_at
            "#,
        )
        .bind(id.as_uuid())
        .bind(status)
        .bind(now)
        .bind(error_message)
        .fetch_one(self.pool)
        .await?;

        Ok(run)
    }

    /// List active runs (pending, queued, or running).
    pub async fn list_active(&self, org_id: OrganizationId, limit: i64) -> Result<Vec<Run>> {
        let runs = sqlx::query_as::<_, Run>(
            r#"
            SELECT id, pipeline_id, trigger_id, status, run_number, commit_sha, branch, 
                   triggered_by, created_at, started_at, finished_at
            FROM runs
            WHERE org_id = $1 AND status IN ('pending', 'queued', 'running')
            ORDER BY created_at DESC
            LIMIT $2
            "#,
        )
        .bind(org_id.as_uuid())
        .bind(limit)
        .fetch_all(self.pool)
        .await?;

        Ok(runs)
    }
}

/// Run with associated job runs.
#[derive(Debug)]
pub struct RunWithJobs {
    pub run: Run,
    pub job_runs: Vec<JobRun>,
}

/// Org/project/pipeline + definition for resolving job secrets from a job run id.
#[derive(Debug, Clone)]
pub struct JobRunPipelineContext {
    pub org_id: Uuid,
    pub project_id: Uuid,
    pub pipeline_id: Uuid,
    pub definition: serde_json::Value,
}

/// Rows for the operator job queue: concrete `job_runs` waiting to start, or a **run** that is
/// still `pending`/`queued` before any `job_runs` exist (engine has not scheduled jobs yet).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct JobQueueItemRow {
    pub job_run_id: Option<Uuid>,
    pub run_id: Uuid,
    pub job_id: Option<Uuid>,
    pub job_name: String,
    pub status: JobStatus,
    pub attempt: i32,
    pub job_run_created_at: DateTime<Utc>,
    pub run_number: i64,
    pub run_status: RunStatus,
    pub pipeline_id: Uuid,
    pub pipeline_name: String,
    pub project_id: Uuid,
    pub project_slug: String,
}

/// Repository for job run operations.
pub struct JobRunRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> JobRunRepo<'a> {
    /// Create a new job run repository.
    #[must_use]
    pub const fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Create a new job run.
    pub async fn create(
        &self,
        run_id: RunId,
        job_id: JobId,
        job_name: &str,
    ) -> Result<JobRun> {
        let id = JobRunId::new();
        let now = Utc::now();

        let job_run = sqlx::query_as::<_, JobRun>(
            r#"
            INSERT INTO job_runs (id, run_id, job_id, job_name, status, attempt, created_at)
            VALUES ($1, $2, $3, $4, 'pending', 1, $5)
            RETURNING id, run_id, job_id, job_name, agent_id, status, attempt, exit_code, 
                      error_message, cache_hit, cache_key, outputs, started_at, finished_at, created_at
            "#,
        )
        .bind(id.as_uuid())
        .bind(run_id.as_uuid())
        .bind(job_id.as_uuid())
        .bind(job_name)
        .bind(now)
        .fetch_one(self.pool)
        .await?;

        Ok(job_run)
    }

    /// Get a job run by ID.
    pub async fn get(&self, id: JobRunId) -> Result<JobRun> {
        sqlx::query_as::<_, JobRun>(
            r#"
            SELECT id, run_id, job_id, job_name, agent_id, status, attempt, exit_code, 
                   error_message, cache_hit, cache_key, outputs, started_at, finished_at, created_at
            FROM job_runs
            WHERE id = $1
            "#,
        )
        .bind(id.as_uuid())
        .fetch_optional(self.pool)
        .await?
        .ok_or_else(|| StoreError::not_found("job_run", id))
    }

    /// List queued work: `job_runs` in `pending`/`queued`, plus **runs** in `pending`/`queued` with no
    /// `job_runs` yet (so admins still see backlog while the executor is starting).
    pub async fn list_job_queue_for_org(
        &self,
        org_id: OrganizationId,
        limit: i64,
    ) -> Result<Vec<JobQueueItemRow>> {
        let org_u = org_id.as_uuid();

        let rows = sqlx::query_as::<_, JobQueueItemRow>(
            r#"
            SELECT
                q.job_run_id,
                q.run_id,
                q.job_id,
                q.job_name,
                q.status,
                q.attempt,
                q.job_run_created_at,
                q.run_number,
                q.run_status,
                q.pipeline_id,
                q.pipeline_name,
                q.project_id,
                q.project_slug
            FROM (
                (
                    SELECT
                        jr.id AS job_run_id,
                        jr.run_id,
                        jr.job_id,
                        jr.job_name,
                        jr.status,
                        jr.attempt,
                        jr.created_at AS job_run_created_at,
                        r.run_number,
                        r.status AS run_status,
                        p.id AS pipeline_id,
                        p.name AS pipeline_name,
                        pr.id AS project_id,
                        pr.slug AS project_slug
                    FROM job_runs jr
                    INNER JOIN runs r ON r.id = jr.run_id
                    INNER JOIN pipelines p ON p.id = r.pipeline_id
                    INNER JOIN projects pr ON pr.id = p.project_id
                    WHERE pr.org_id = $1
                      AND pr.deleted_at IS NULL
                      AND jr.status IN ('pending', 'queued')
                      AND r.status IN ('pending', 'queued', 'running')
                )
                UNION ALL
                (
                    SELECT
                        NULL::uuid AS job_run_id,
                        r.id AS run_id,
                        NULL::uuid AS job_id,
                        '(run pending — no job rows yet)'::text AS job_name,
                        'pending'::run_status AS status,
                        0 AS attempt,
                        r.created_at AS job_run_created_at,
                        r.run_number,
                        r.status AS run_status,
                        p.id AS pipeline_id,
                        p.name AS pipeline_name,
                        pr.id AS project_id,
                        pr.slug AS project_slug
                    FROM runs r
                    INNER JOIN pipelines p ON p.id = r.pipeline_id
                    INNER JOIN projects pr ON pr.id = p.project_id
                    WHERE pr.org_id = $1
                      AND pr.deleted_at IS NULL
                      AND r.status IN ('pending', 'queued')
                      AND NOT EXISTS (SELECT 1 FROM job_runs jr WHERE jr.run_id = r.id)
                )
            ) AS q
            ORDER BY q.job_run_created_at ASC
            LIMIT $2
            "#,
        )
        .bind(org_u)
        .bind(limit)
        .fetch_all(self.pool)
        .await?;

        Ok(rows)
    }

    /// List job runs for a run.
    pub async fn list_by_run(&self, run_id: RunId) -> Result<Vec<JobRun>> {
        let job_runs = sqlx::query_as::<_, JobRun>(
            r#"
            SELECT id, run_id, job_id, job_name, agent_id, status, attempt, exit_code, 
                   error_message, cache_hit, cache_key, outputs, started_at, finished_at, created_at
            FROM job_runs
            WHERE run_id = $1
            ORDER BY created_at
            "#,
        )
        .bind(run_id.as_uuid())
        .fetch_all(self.pool)
        .await?;

        Ok(job_runs)
    }

    /// Update job run status to queued.
    pub async fn mark_queued(&self, id: JobRunId) -> Result<JobRun> {
        self.update_status(id, JobStatus::Queued, None, None).await
    }

    /// Update job run status to running with agent assignment.
    pub async fn mark_running(&self, id: JobRunId, agent_id: AgentId) -> Result<JobRun> {
        let now = Utc::now();
        
        let job_run = sqlx::query_as::<_, JobRun>(
            r#"
            UPDATE job_runs
            SET status = 'running', agent_id = $2, started_at = $3
            WHERE id = $1
            RETURNING id, run_id, job_id, job_name, agent_id, status, attempt, exit_code, 
                      error_message, cache_hit, cache_key, outputs, started_at, finished_at, created_at
            "#,
        )
        .bind(id.as_uuid())
        .bind(agent_id.as_uuid())
        .bind(now)
        .fetch_one(self.pool)
        .await?;

        Ok(job_run)
    }

    /// Mark job as completed.
    pub async fn mark_completed(
        &self,
        id: JobRunId,
        success: bool,
        exit_code: Option<i32>,
        error_message: Option<&str>,
        outputs: Option<serde_json::Value>,
    ) -> Result<JobRun> {
        let now = Utc::now();
        let status = if success { JobStatus::Succeeded } else { JobStatus::Failed };
        
        let job_run = sqlx::query_as::<_, JobRun>(
            r#"
            UPDATE job_runs
            SET status = $2, exit_code = $3, error_message = $4, outputs = COALESCE($5, outputs), finished_at = $6
            WHERE id = $1
            RETURNING id, run_id, job_id, job_name, agent_id, status, attempt, exit_code, 
                      error_message, cache_hit, cache_key, outputs, started_at, finished_at, created_at
            "#,
        )
        .bind(id.as_uuid())
        .bind(status)
        .bind(exit_code)
        .bind(error_message)
        .bind(outputs)
        .bind(now)
        .fetch_one(self.pool)
        .await?;

        Ok(job_run)
    }

    /// Mark job as skipped.
    pub async fn mark_skipped(&self, id: JobRunId, reason: Option<&str>) -> Result<JobRun> {
        self.update_status(id, JobStatus::Skipped, None, reason).await
    }

    /// Mark job as cancelled.
    pub async fn mark_cancelled(&self, id: JobRunId) -> Result<JobRun> {
        self.update_status(id, JobStatus::Cancelled, None, Some("Cancelled by user")).await
    }

    /// Mark job as timed out.
    pub async fn mark_timed_out(&self, id: JobRunId) -> Result<JobRun> {
        self.update_status(id, JobStatus::TimedOut, None, Some("Job execution timed out")).await
    }

    /// Set cache hit information.
    pub async fn set_cache_hit(&self, id: JobRunId, cache_key: &str) -> Result<JobRun> {
        let job_run = sqlx::query_as::<_, JobRun>(
            r#"
            UPDATE job_runs
            SET cache_hit = true, cache_key = $2
            WHERE id = $1
            RETURNING id, run_id, job_id, job_name, agent_id, status, attempt, exit_code, 
                      error_message, cache_hit, cache_key, outputs, started_at, finished_at, created_at
            "#,
        )
        .bind(id.as_uuid())
        .bind(cache_key)
        .fetch_one(self.pool)
        .await?;

        Ok(job_run)
    }

    /// Increment attempt counter for retry.
    pub async fn increment_attempt(&self, id: JobRunId) -> Result<JobRun> {
        let job_run = sqlx::query_as::<_, JobRun>(
            r#"
            UPDATE job_runs
            SET attempt = attempt + 1, status = 'pending', 
                started_at = NULL, finished_at = NULL, exit_code = NULL, error_message = NULL
            WHERE id = $1
            RETURNING id, run_id, job_id, job_name, agent_id, status, attempt, exit_code, 
                      error_message, cache_hit, cache_key, outputs, started_at, finished_at, created_at
            "#,
        )
        .bind(id.as_uuid())
        .fetch_one(self.pool)
        .await?;

        Ok(job_run)
    }

    /// Load pipeline definition context for secret resolution (controller / jobs).
    pub async fn get_pipeline_context(&self, job_run_id: JobRunId) -> Result<Option<JobRunPipelineContext>> {
        let row = sqlx::query_as::<_, (Uuid, Uuid, Uuid, serde_json::Value)>(
            r#"
            SELECT COALESCE(r.org_id, pr.org_id), p.project_id, p.id, p.definition
            FROM job_runs jr
            JOIN runs r ON r.id = jr.run_id
            JOIN pipelines p ON p.id = r.pipeline_id
            JOIN projects pr ON pr.id = p.project_id
            WHERE jr.id = $1
            "#,
        )
        .bind(job_run_id.as_uuid())
        .fetch_optional(self.pool)
        .await?;

        Ok(row.map(|(org_id, project_id, pipeline_id, definition)| JobRunPipelineContext {
            org_id,
            project_id,
            pipeline_id,
            definition,
        }))
    }

    /// Update status helper.
    async fn update_status(
        &self,
        id: JobRunId,
        status: JobStatus,
        exit_code: Option<i32>,
        error_message: Option<&str>,
    ) -> Result<JobRun> {
        let now = Utc::now();
        let is_terminal = status.is_terminal();
        
        let job_run = sqlx::query_as::<_, JobRun>(
            r#"
            UPDATE job_runs
            SET status = $2, 
                exit_code = COALESCE($3, exit_code), 
                error_message = COALESCE($4, error_message),
                finished_at = CASE WHEN $5 THEN $6 ELSE finished_at END
            WHERE id = $1
            RETURNING id, run_id, job_id, job_name, agent_id, status, attempt, exit_code, 
                      error_message, cache_hit, cache_key, outputs, started_at, finished_at, created_at
            "#,
        )
        .bind(id.as_uuid())
        .bind(status)
        .bind(exit_code)
        .bind(error_message)
        .bind(is_terminal)
        .bind(now)
        .fetch_one(self.pool)
        .await?;

        Ok(job_run)
    }
}

/// Repository for step run operations.
pub struct StepRunRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> StepRunRepo<'a> {
    /// Create a new step run repository.
    #[must_use]
    pub const fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Create a new step run.
    pub async fn create(
        &self,
        job_run_id: JobRunId,
        step_id: StepId,
        step_name: &str,
    ) -> Result<StepRun> {
        let id = StepRunId::new();
        let now = Utc::now();

        let step_run = sqlx::query_as::<_, StepRun>(
            r#"
            INSERT INTO step_runs (id, job_run_id, step_id, step_name, status, created_at)
            VALUES ($1, $2, $3, $4, 'pending', $5)
            RETURNING id, job_run_id, step_id, step_name, status, exit_code, error_message, 
                      log_path, outputs, started_at, finished_at, created_at
            "#,
        )
        .bind(id.as_uuid())
        .bind(job_run_id.as_uuid())
        .bind(step_id.as_uuid())
        .bind(step_name)
        .bind(now)
        .fetch_one(self.pool)
        .await?;

        Ok(step_run)
    }

    /// Get a step run by ID.
    pub async fn get(&self, id: StepRunId) -> Result<StepRun> {
        sqlx::query_as::<_, StepRun>(
            r#"
            SELECT id, job_run_id, step_id, step_name, status, exit_code, error_message, 
                   log_path, outputs, started_at, finished_at, created_at
            FROM step_runs
            WHERE id = $1
            "#,
        )
        .bind(id.as_uuid())
        .fetch_optional(self.pool)
        .await?
        .ok_or_else(|| StoreError::not_found("step_run", id))
    }

    /// List step runs for a job run.
    pub async fn list_by_job_run(&self, job_run_id: JobRunId) -> Result<Vec<StepRun>> {
        let step_runs = sqlx::query_as::<_, StepRun>(
            r#"
            SELECT id, job_run_id, step_id, step_name, status, exit_code, error_message, 
                   log_path, outputs, started_at, finished_at, created_at
            FROM step_runs
            WHERE job_run_id = $1
            ORDER BY created_at
            "#,
        )
        .bind(job_run_id.as_uuid())
        .fetch_all(self.pool)
        .await?;

        Ok(step_runs)
    }

    /// Mark step as running.
    pub async fn mark_running(&self, id: StepRunId) -> Result<StepRun> {
        let now = Utc::now();
        
        let step_run = sqlx::query_as::<_, StepRun>(
            r#"
            UPDATE step_runs
            SET status = 'running', started_at = $2
            WHERE id = $1
            RETURNING id, job_run_id, step_id, step_name, status, exit_code, error_message, 
                      log_path, outputs, started_at, finished_at, created_at
            "#,
        )
        .bind(id.as_uuid())
        .bind(now)
        .fetch_one(self.pool)
        .await?;

        Ok(step_run)
    }

    /// Mark step as completed.
    pub async fn mark_completed(
        &self,
        id: StepRunId,
        exit_code: i32,
        error_message: Option<&str>,
        log_path: Option<&str>,
        outputs: Option<serde_json::Value>,
    ) -> Result<StepRun> {
        let now = Utc::now();
        let status = if exit_code == 0 { 
            met_core::models::StepStatus::Succeeded 
        } else { 
            met_core::models::StepStatus::Failed 
        };
        
        let step_run = sqlx::query_as::<_, StepRun>(
            r#"
            UPDATE step_runs
            SET status = $2, exit_code = $3, error_message = $4, log_path = $5, 
                outputs = COALESCE($6, outputs), finished_at = $7
            WHERE id = $1
            RETURNING id, job_run_id, step_id, step_name, status, exit_code, error_message, 
                      log_path, outputs, started_at, finished_at, created_at
            "#,
        )
        .bind(id.as_uuid())
        .bind(status)
        .bind(exit_code)
        .bind(error_message)
        .bind(log_path)
        .bind(outputs)
        .bind(now)
        .fetch_one(self.pool)
        .await?;

        Ok(step_run)
    }

    /// Mark step as skipped.
    pub async fn mark_skipped(&self, id: StepRunId, reason: Option<&str>) -> Result<StepRun> {
        let now = Utc::now();
        
        let step_run = sqlx::query_as::<_, StepRun>(
            r#"
            UPDATE step_runs
            SET status = 'skipped', error_message = $2, finished_at = $3
            WHERE id = $1
            RETURNING id, job_run_id, step_id, step_name, status, exit_code, error_message, 
                      log_path, outputs, started_at, finished_at, created_at
            "#,
        )
        .bind(id.as_uuid())
        .bind(reason)
        .bind(now)
        .fetch_one(self.pool)
        .await?;

        Ok(step_run)
    }
}
