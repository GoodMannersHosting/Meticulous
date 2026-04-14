//! Run repository.

use chrono::{DateTime, Utc};
use met_core::ids::{
    AgentId, JobId, JobRunId, OrganizationId, PipelineId, ProjectId, RunId, StepId, StepRunId,
    TriggerId,
};
use met_core::models::{JobRun, JobStatus, Run, RunStatus, StepRun};
use sqlx::PgPool;
use std::collections::HashMap;
use uuid::Uuid;

use crate::error::{Result, StoreError};

/// A run returned from a project-scoped list, including the pipeline display name.
#[derive(Debug, Clone)]
pub struct RunWithPipelineName {
    pub run: Run,
    pub pipeline_name: String,
}

/// Run row for org-wide (or multi-project) lists, including project and pipeline labels.
#[derive(Debug, Clone)]
pub struct RunWithPipelineAndProjectName {
    pub run: Run,
    pub pipeline_name: String,
    pub project_name: String,
    pub project_id: ProjectId,
}

#[derive(sqlx::FromRow)]
struct RunProjectListRow {
    id: RunId,
    pipeline_id: PipelineId,
    parent_run_id: Option<RunId>,
    trigger_id: Option<TriggerId>,
    status: RunStatus,
    run_number: i64,
    commit_sha: Option<String>,
    branch: Option<String>,
    webhook_remote_addr: Option<String>,
    triggered_by: String,
    created_at: DateTime<Utc>,
    started_at: Option<DateTime<Utc>>,
    finished_at: Option<DateTime<Utc>>,
    pipeline_name: String,
}

#[derive(sqlx::FromRow)]
struct RunOrgListRow {
    id: RunId,
    pipeline_id: PipelineId,
    parent_run_id: Option<RunId>,
    trigger_id: Option<TriggerId>,
    status: RunStatus,
    run_number: i64,
    commit_sha: Option<String>,
    branch: Option<String>,
    webhook_remote_addr: Option<String>,
    triggered_by: String,
    created_at: DateTime<Utc>,
    started_at: Option<DateTime<Utc>>,
    finished_at: Option<DateTime<Utc>>,
    pipeline_name: String,
    project_name: String,
    project_id: ProjectId,
}

impl From<RunOrgListRow> for RunWithPipelineAndProjectName {
    fn from(row: RunOrgListRow) -> Self {
        Self {
            pipeline_name: row.pipeline_name,
            project_name: row.project_name,
            project_id: row.project_id,
            run: Run {
                id: row.id,
                pipeline_id: row.pipeline_id,
                parent_run_id: row.parent_run_id,
                trigger_id: row.trigger_id,
                status: row.status,
                run_number: row.run_number,
                commit_sha: row.commit_sha,
                branch: row.branch,
                webhook_remote_addr: row.webhook_remote_addr,
                triggered_by: row.triggered_by,
                created_at: row.created_at,
                started_at: row.started_at,
                finished_at: row.finished_at,
            },
        }
    }
}

impl From<RunProjectListRow> for RunWithPipelineName {
    fn from(row: RunProjectListRow) -> Self {
        Self {
            pipeline_name: row.pipeline_name,
            run: Run {
                id: row.id,
                pipeline_id: row.pipeline_id,
                parent_run_id: row.parent_run_id,
                trigger_id: row.trigger_id,
                status: row.status,
                run_number: row.run_number,
                commit_sha: row.commit_sha,
                branch: row.branch,
                webhook_remote_addr: row.webhook_remote_addr,
                triggered_by: row.triggered_by,
                created_at: row.created_at,
                started_at: row.started_at,
                finished_at: row.finished_at,
            },
        }
    }
}

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
        parent_run_id: Option<RunId>,
        webhook_remote_addr: Option<&str>,
    ) -> Result<Run> {
        let id = RunId::new();
        let now = Utc::now();
        let run_number = self.next_run_number(pipeline_id).await?;

        let run = sqlx::query_as::<_, Run>(
            r#"
            INSERT INTO runs (id, pipeline_id, parent_run_id, org_id, trigger_id, status, run_number, triggered_by, 
                              trace_id, commit_sha, branch, trigger_data, webhook_remote_addr, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
            RETURNING id, pipeline_id, parent_run_id, trigger_id, status, run_number, commit_sha, branch, 
                      webhook_remote_addr, triggered_by, created_at, started_at, finished_at
            "#,
        )
        .bind(id.as_uuid())
        .bind(pipeline_id.as_uuid())
        .bind(parent_run_id.map(|p| p.as_uuid()))
        .bind(org_id.as_uuid())
        .bind(trigger_id.map(|t| t.as_uuid()))
        .bind(RunStatus::Pending)
        .bind(run_number)
        .bind(triggered_by)
        .bind(trace_id)
        .bind(commit_sha)
        .bind(branch)
        .bind(trigger_data)
        .bind(webhook_remote_addr)
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
        parent_run_id: Option<RunId>,
    ) -> Result<Run> {
        let id = RunId::new();
        let now = Utc::now();

        // Get the next run number for this pipeline
        let run_number = self.next_run_number(pipeline_id).await?;

        let run = sqlx::query_as::<_, Run>(
            r#"
            INSERT INTO runs (id, pipeline_id, parent_run_id, trigger_id, status, run_number, triggered_by, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING id, pipeline_id, parent_run_id, trigger_id, status, run_number, commit_sha, branch, webhook_remote_addr, triggered_by, created_at, started_at, finished_at
            "#,
        )
        .bind(id.as_uuid())
        .bind(pipeline_id.as_uuid())
        .bind(parent_run_id.map(|p| p.as_uuid()))
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
            SELECT id, pipeline_id, parent_run_id, trigger_id, status, run_number, commit_sha, branch, webhook_remote_addr, triggered_by, created_at, started_at, finished_at
            FROM runs
            WHERE id = $1
            "#,
        )
        .bind(id.as_uuid())
        .fetch_optional(self.pool)
        .await?
        .ok_or_else(|| StoreError::not_found("run", id))
    }

    /// Look up a run by stable pipeline run number (for compare-to-previous and deep links).
    pub async fn find_by_pipeline_and_run_number(
        &self,
        pipeline_id: PipelineId,
        run_number: i64,
    ) -> Result<Option<Run>> {
        Ok(sqlx::query_as::<_, Run>(
            r#"
            SELECT id, pipeline_id, parent_run_id, trigger_id, status, run_number, commit_sha, branch, webhook_remote_addr, triggered_by, created_at, started_at, finished_at
            FROM runs
            WHERE pipeline_id = $1 AND run_number = $2
            "#,
        )
        .bind(pipeline_id.as_uuid())
        .bind(run_number)
        .fetch_optional(self.pool)
        .await?)
    }

    /// List runs for a pipeline.
    pub async fn list_by_pipeline(
        &self,
        pipeline_id: PipelineId,
        status: Option<RunStatus>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Run>> {
        let runs = if let Some(st) = status {
            sqlx::query_as::<_, Run>(
                r#"
                SELECT id, pipeline_id, parent_run_id, trigger_id, status, run_number, commit_sha, branch, webhook_remote_addr, triggered_by, created_at, started_at, finished_at
                FROM runs
                WHERE pipeline_id = $1 AND status = $2
                ORDER BY created_at DESC
                LIMIT $3 OFFSET $4
                "#,
            )
            .bind(pipeline_id.as_uuid())
            .bind(st)
            .bind(limit)
            .bind(offset)
            .fetch_all(self.pool)
            .await?
        } else {
            sqlx::query_as::<_, Run>(
                r#"
                SELECT id, pipeline_id, parent_run_id, trigger_id, status, run_number, commit_sha, branch, webhook_remote_addr, triggered_by, created_at, started_at, finished_at
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
            .await?
        };

        Ok(runs)
    }

    /// List runs for all pipelines in a project (non-deleted project only), with each pipeline's name.
    pub async fn list_by_project(
        &self,
        project_id: ProjectId,
        status: Option<RunStatus>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<RunWithPipelineName>> {
        let rows = if let Some(st) = status {
            sqlx::query_as::<_, RunProjectListRow>(
                r#"
                SELECT r.id, r.pipeline_id, r.parent_run_id, r.trigger_id, r.status, r.run_number, r.commit_sha, r.branch,
                       r.webhook_remote_addr, r.triggered_by, r.created_at, r.started_at, r.finished_at,
                       p.name AS pipeline_name
                FROM runs r
                INNER JOIN pipelines p ON p.id = r.pipeline_id
                INNER JOIN projects pr ON pr.id = p.project_id
                WHERE p.project_id = $1
                  AND pr.deleted_at IS NULL
                  AND r.status = $2
                ORDER BY r.created_at DESC
                LIMIT $3 OFFSET $4
                "#,
            )
            .bind(project_id.as_uuid())
            .bind(st)
            .bind(limit)
            .bind(offset)
            .fetch_all(self.pool)
            .await?
        } else {
            sqlx::query_as::<_, RunProjectListRow>(
                r#"
                SELECT r.id, r.pipeline_id, r.parent_run_id, r.trigger_id, r.status, r.run_number, r.commit_sha, r.branch,
                       r.webhook_remote_addr, r.triggered_by, r.created_at, r.started_at, r.finished_at,
                       p.name AS pipeline_name
                FROM runs r
                INNER JOIN pipelines p ON p.id = r.pipeline_id
                INNER JOIN projects pr ON pr.id = p.project_id
                WHERE p.project_id = $1
                  AND pr.deleted_at IS NULL
                ORDER BY r.created_at DESC
                LIMIT $2 OFFSET $3
                "#,
            )
            .bind(project_id.as_uuid())
            .bind(limit)
            .bind(offset)
            .fetch_all(self.pool)
            .await?
        };

        Ok(rows.into_iter().map(RunWithPipelineName::from).collect())
    }

    /// List runs across all non-deleted projects in an organization.
    pub async fn list_by_organization(
        &self,
        org_id: OrganizationId,
        status: Option<RunStatus>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<RunWithPipelineAndProjectName>> {
        let rows = if let Some(st) = status {
            sqlx::query_as::<_, RunOrgListRow>(
                r#"
                SELECT r.id, r.pipeline_id, r.parent_run_id, r.trigger_id, r.status, r.run_number, r.commit_sha, r.branch,
                       r.webhook_remote_addr, r.triggered_by, r.created_at, r.started_at, r.finished_at,
                       p.name AS pipeline_name, pr.name AS project_name, pr.id AS project_id
                FROM runs r
                INNER JOIN pipelines p ON p.id = r.pipeline_id
                INNER JOIN projects pr ON pr.id = p.project_id
                WHERE pr.org_id = $1
                  AND pr.deleted_at IS NULL
                  AND r.status = $2
                ORDER BY r.created_at DESC
                LIMIT $3 OFFSET $4
                "#,
            )
            .bind(org_id.as_uuid())
            .bind(st)
            .bind(limit)
            .bind(offset)
            .fetch_all(self.pool)
            .await?
        } else {
            sqlx::query_as::<_, RunOrgListRow>(
                r#"
                SELECT r.id, r.pipeline_id, r.parent_run_id, r.trigger_id, r.status, r.run_number, r.commit_sha, r.branch,
                       r.webhook_remote_addr, r.triggered_by, r.created_at, r.started_at, r.finished_at,
                       p.name AS pipeline_name, pr.name AS project_name, pr.id AS project_id
                FROM runs r
                INNER JOIN pipelines p ON p.id = r.pipeline_id
                INNER JOIN projects pr ON pr.id = p.project_id
                WHERE pr.org_id = $1
                  AND pr.deleted_at IS NULL
                ORDER BY r.created_at DESC
                LIMIT $2 OFFSET $3
                "#,
            )
            .bind(org_id.as_uuid())
            .bind(limit)
            .bind(offset)
            .fetch_all(self.pool)
            .await?
        };

        Ok(rows
            .into_iter()
            .map(RunWithPipelineAndProjectName::from)
            .collect())
    }

    /// List runs for any pipeline whose project is in `project_ids`.
    pub async fn list_by_project_ids(
        &self,
        project_ids: &[ProjectId],
        status: Option<RunStatus>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<RunWithPipelineAndProjectName>> {
        if project_ids.is_empty() {
            return Ok(vec![]);
        }

        let ids: Vec<Uuid> = project_ids.iter().map(|p| p.as_uuid()).collect();

        let rows = if let Some(st) = status {
            sqlx::query_as::<_, RunOrgListRow>(
                r#"
                SELECT r.id, r.pipeline_id, r.parent_run_id, r.trigger_id, r.status, r.run_number, r.commit_sha, r.branch,
                       r.webhook_remote_addr, r.triggered_by, r.created_at, r.started_at, r.finished_at,
                       p.name AS pipeline_name, pr.name AS project_name, pr.id AS project_id
                FROM runs r
                INNER JOIN pipelines p ON p.id = r.pipeline_id
                INNER JOIN projects pr ON pr.id = p.project_id
                WHERE p.project_id = ANY($1)
                  AND pr.deleted_at IS NULL
                  AND r.status = $2
                ORDER BY r.created_at DESC
                LIMIT $3 OFFSET $4
                "#,
            )
            .bind(&ids[..])
            .bind(st)
            .bind(limit)
            .bind(offset)
            .fetch_all(self.pool)
            .await?
        } else {
            sqlx::query_as::<_, RunOrgListRow>(
                r#"
                SELECT r.id, r.pipeline_id, r.parent_run_id, r.trigger_id, r.status, r.run_number, r.commit_sha, r.branch,
                       r.webhook_remote_addr, r.triggered_by, r.created_at, r.started_at, r.finished_at,
                       p.name AS pipeline_name, pr.name AS project_name, pr.id AS project_id
                FROM runs r
                INNER JOIN pipelines p ON p.id = r.pipeline_id
                INNER JOIN projects pr ON pr.id = p.project_id
                WHERE p.project_id = ANY($1)
                  AND pr.deleted_at IS NULL
                ORDER BY r.created_at DESC
                LIMIT $2 OFFSET $3
                "#,
            )
            .bind(&ids[..])
            .bind(limit)
            .bind(offset)
            .fetch_all(self.pool)
            .await?
        };

        Ok(rows
            .into_iter()
            .map(RunWithPipelineAndProjectName::from)
            .collect())
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
            RETURNING id, pipeline_id, parent_run_id, trigger_id, status, run_number, commit_sha, branch, webhook_remote_addr, triggered_by, created_at, started_at, finished_at
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
                   error_message, cache_hit, log_path, cache_key, outputs, started_at, finished_at, created_at,
                   pipeline_definition_sha256, workflow_definition_sha256, source_workflow,
                   agent_snapshot, agent_snapshot_captured_at, output_wrap_x25519_secret
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
            RETURNING id, pipeline_id, parent_run_id, trigger_id, status, run_number, commit_sha, branch, 
                      webhook_remote_addr, triggered_by, created_at, started_at, finished_at
            "#,
        )
        .bind(id.as_uuid())
        .bind(now)
        .fetch_one(self.pool)
        .await?;

        Ok(run)
    }

    /// Complete a run with final status.
    pub async fn complete_run(
        &self,
        id: RunId,
        status: RunStatus,
        error_message: Option<&str>,
    ) -> Result<Run> {
        let now = Utc::now();

        let run = sqlx::query_as::<_, Run>(
            r#"
            UPDATE runs
            SET status = $2, finished_at = $3, error_message = $4
            WHERE id = $1
            RETURNING id, pipeline_id, parent_run_id, trigger_id, status, run_number, commit_sha, branch, 
                      webhook_remote_addr, triggered_by, created_at, started_at, finished_at
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
            SELECT id, pipeline_id, parent_run_id, trigger_id, status, run_number, commit_sha, branch, 
                   webhook_remote_addr, triggered_by, created_at, started_at, finished_at
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

    /// Delete at most `batch_size` runs for the given project that were created before `before`.
    ///
    /// Only terminal runs (`succeeded`, `failed`, `cancelled`) are deleted so that runs that are
    /// still being processed are never removed mid-flight.  Cascade `ON DELETE` constraints on
    /// `job_runs`, `step_runs`, `run_events`, `run_logs`, `run_binary_executions`,
    /// `run_network_connections`, `run_syscall_events`, `run_runtime_script_artifacts`,
    /// `log_cache`, `log_archives`, `pipeline_run_workflow_outputs`, and `oidc_token_audit` take
    /// care of all child rows automatically.
    ///
    /// Returns the number of rows deleted.
    pub async fn delete_old_runs_for_project(
        &self,
        project_id: ProjectId,
        before: DateTime<Utc>,
        batch_size: i64,
    ) -> Result<u64> {
        let result = sqlx::query(
            r#"
            WITH to_delete AS (
                SELECT r.id
                FROM runs r
                JOIN pipelines p ON r.pipeline_id = p.id
                WHERE p.project_id = $1
                  AND r.created_at < $2
                  AND r.status IN ('succeeded', 'failed', 'cancelled')
                LIMIT $3
            )
            DELETE FROM runs WHERE id IN (SELECT id FROM to_delete)
            "#,
        )
        .bind(project_id.as_uuid())
        .bind(before)
        .bind(batch_size)
        .execute(self.pool)
        .await?;

        Ok(result.rows_affected())
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

/// Verified job/run context for OIDC workload identity tokens (ADR-017).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct OidcJobIdentityRow {
    pub org_id: Uuid,
    pub org_slug: String,
    pub project_id: Uuid,
    pub project_slug: String,
    pub pipeline_id: Uuid,
    pub pipeline_name: String,
    pub run_id: Uuid,
    pub job_run_id: Uuid,
    pub branch: Option<String>,
    pub commit_sha: Option<String>,
    pub environment_name: Option<String>,
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

/// Aggregated `job_runs` stats for one pipeline run (UI badges).
#[derive(Debug, Clone, Copy)]
pub struct JobRunRollup {
    pub job_count: i64,
    pub any_running: bool,
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
    pub async fn create(&self, run_id: RunId, job_id: JobId, job_name: &str) -> Result<JobRun> {
        let id = JobRunId::new();
        let now = Utc::now();

        let job_run = sqlx::query_as::<_, JobRun>(
            r#"
            INSERT INTO job_runs (id, run_id, job_id, job_name, status, attempt, created_at)
            VALUES ($1, $2, $3, $4, 'pending', 1, $5)
            RETURNING id, run_id, job_id, job_name, agent_id, status, attempt, exit_code, 
                      error_message, cache_hit, log_path, cache_key, outputs, started_at, finished_at, created_at,
                      pipeline_definition_sha256, workflow_definition_sha256, source_workflow,
                      agent_snapshot, agent_snapshot_captured_at, output_wrap_x25519_secret
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
                   error_message, cache_hit, log_path, cache_key, outputs, started_at, finished_at, created_at,
                   pipeline_definition_sha256, workflow_definition_sha256, source_workflow,
                   agent_snapshot, agent_snapshot_captured_at, output_wrap_x25519_secret
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
                   error_message, cache_hit, log_path, cache_key, outputs, started_at, finished_at, created_at,
                   pipeline_definition_sha256, workflow_definition_sha256, source_workflow,
                   agent_snapshot, agent_snapshot_captured_at, output_wrap_x25519_secret
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

    /// Roll up job-run counts and whether any job is `running` per run (for run-list badges).
    pub async fn rollup_by_run_ids(
        &self,
        run_ids: &[RunId],
    ) -> Result<HashMap<RunId, JobRunRollup>> {
        if run_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let uuids: Vec<Uuid> = run_ids.iter().map(|r| r.as_uuid()).collect();
        let rows: Vec<(Uuid, i64, bool)> = sqlx::query_as(
            r#"
            SELECT run_id, COUNT(*)::bigint, BOOL_OR(status = 'running')
            FROM job_runs
            WHERE run_id = ANY($1)
            GROUP BY run_id
            "#,
        )
        .bind(&uuids[..])
        .fetch_all(self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(rid, job_count, any_running)| {
                (
                    RunId::from_uuid(rid),
                    JobRunRollup {
                        job_count,
                        any_running,
                    },
                )
            })
            .collect())
    }

    /// When a run is already terminal but `job_runs` / `step_runs` were never updated (missed
    /// controller ack, crash, etc.), move non-terminal rows to a consistent terminal status.
    ///
    /// Returns `(steps_updated, jobs_updated)`.
    pub async fn reconcile_stale_jobs_and_steps_for_terminal_run(
        &self,
        run_id: RunId,
        run_status: RunStatus,
        run_finished_at: Option<DateTime<Utc>>,
    ) -> Result<(u64, u64)> {
        if !run_status.is_terminal() {
            return Ok((0, 0));
        }

        let ts = run_finished_at.unwrap_or_else(Utc::now);
        let mut steps_total: u64 = 0;
        let mut jobs_total: u64 = 0;

        // step_runs
        steps_total += sqlx::query(
            r#"
UPDATE step_runs sr
SET status = 'cancelled',
    finished_at = COALESCE(sr.finished_at, r.finished_at, $2),
    error_message = COALESCE(sr.error_message, 'Run cancelled')
FROM job_runs jr
JOIN runs r ON r.id = jr.run_id
WHERE sr.job_run_id = jr.id
  AND jr.run_id = $1
  AND r.status = 'cancelled'
  AND sr.status IN ('pending', 'queued', 'running')
"#,
        )
        .bind(run_id.as_uuid())
        .bind(ts)
        .execute(self.pool)
        .await?
        .rows_affected();

        steps_total += sqlx::query(
            r#"
UPDATE step_runs sr
SET status = 'timed_out',
    finished_at = COALESCE(sr.finished_at, r.finished_at, $2),
    error_message = COALESCE(sr.error_message, 'Run timed out')
FROM job_runs jr
JOIN runs r ON r.id = jr.run_id
WHERE sr.job_run_id = jr.id
  AND jr.run_id = $1
  AND r.status = 'timed_out'
  AND sr.status IN ('pending', 'queued', 'running')
"#,
        )
        .bind(run_id.as_uuid())
        .bind(ts)
        .execute(self.pool)
        .await?
        .rows_affected();

        steps_total += sqlx::query(
            r#"
UPDATE step_runs sr
SET status = 'failed',
    exit_code = COALESCE(sr.exit_code, 1),
    finished_at = COALESCE(sr.finished_at, r.finished_at, $2),
    error_message = COALESCE(sr.error_message, 'Run ended while step was running')
FROM job_runs jr
JOIN runs r ON r.id = jr.run_id
WHERE sr.job_run_id = jr.id
  AND jr.run_id = $1
  AND r.status = 'failed'
  AND sr.status = 'running'
"#,
        )
        .bind(run_id.as_uuid())
        .bind(ts)
        .execute(self.pool)
        .await?
        .rows_affected();

        steps_total += sqlx::query(
            r#"
UPDATE step_runs sr
SET status = 'cancelled',
    finished_at = COALESCE(sr.finished_at, r.finished_at, $2),
    error_message = COALESCE(
        sr.error_message,
        NULLIF(TRIM(COALESCE(r.error_message, '')), ''),
        'Run failed before this step executed'
    )
FROM job_runs jr
JOIN runs r ON r.id = jr.run_id
WHERE sr.job_run_id = jr.id
  AND jr.run_id = $1
  AND r.status = 'failed'
  AND sr.status IN ('pending', 'queued')
"#,
        )
        .bind(run_id.as_uuid())
        .bind(ts)
        .execute(self.pool)
        .await?
        .rows_affected();

        steps_total += sqlx::query(
            r#"
UPDATE step_runs sr
SET status = 'failed',
    exit_code = COALESCE(sr.exit_code, 1),
    finished_at = COALESCE(sr.finished_at, r.finished_at, $2),
    error_message = COALESCE(sr.error_message, 'Run was marked succeeded while step was still running')
FROM job_runs jr
JOIN runs r ON r.id = jr.run_id
WHERE sr.job_run_id = jr.id
  AND jr.run_id = $1
  AND r.status = 'succeeded'
  AND sr.status = 'running'
"#,
        )
        .bind(run_id.as_uuid())
        .bind(ts)
        .execute(self.pool)
        .await?
        .rows_affected();

        steps_total += sqlx::query(
            r#"
UPDATE step_runs sr
SET status = 'skipped',
    finished_at = COALESCE(sr.finished_at, r.finished_at, $2),
    error_message = COALESCE(sr.error_message, 'Run completed; step was not executed')
FROM job_runs jr
JOIN runs r ON r.id = jr.run_id
WHERE sr.job_run_id = jr.id
  AND jr.run_id = $1
  AND r.status = 'succeeded'
  AND sr.status IN ('pending', 'queued')
"#,
        )
        .bind(run_id.as_uuid())
        .bind(ts)
        .execute(self.pool)
        .await?
        .rows_affected();

        // job_runs
        jobs_total += sqlx::query(
            r#"
UPDATE job_runs jr
SET status = 'cancelled',
    finished_at = COALESCE(jr.finished_at, r.finished_at, $2),
    error_message = COALESCE(jr.error_message, 'Run cancelled')
FROM runs r
WHERE jr.run_id = r.id
  AND jr.run_id = $1
  AND r.status = 'cancelled'
  AND jr.status IN ('pending', 'queued', 'running')
"#,
        )
        .bind(run_id.as_uuid())
        .bind(ts)
        .execute(self.pool)
        .await?
        .rows_affected();

        jobs_total += sqlx::query(
            r#"
UPDATE job_runs jr
SET status = 'timed_out',
    finished_at = COALESCE(jr.finished_at, r.finished_at, $2),
    error_message = COALESCE(jr.error_message, 'Run timed out')
FROM runs r
WHERE jr.run_id = r.id
  AND jr.run_id = $1
  AND r.status = 'timed_out'
  AND jr.status IN ('pending', 'queued', 'running')
"#,
        )
        .bind(run_id.as_uuid())
        .bind(ts)
        .execute(self.pool)
        .await?
        .rows_affected();

        jobs_total += sqlx::query(
            r#"
UPDATE job_runs jr
SET status = 'failed',
    exit_code = COALESCE(jr.exit_code, 1),
    finished_at = COALESCE(jr.finished_at, r.finished_at, $2),
    error_message = COALESCE(jr.error_message, 'Run ended while job was running')
FROM runs r
WHERE jr.run_id = r.id
  AND jr.run_id = $1
  AND r.status = 'failed'
  AND jr.status = 'running'
"#,
        )
        .bind(run_id.as_uuid())
        .bind(ts)
        .execute(self.pool)
        .await?
        .rows_affected();

        jobs_total += sqlx::query(
            r#"
UPDATE job_runs jr
SET status = 'cancelled',
    finished_at = COALESCE(jr.finished_at, r.finished_at, $2),
    error_message = COALESCE(
        jr.error_message,
        NULLIF(TRIM(COALESCE(r.error_message, '')), ''),
        'Run failed before this job executed'
    )
FROM runs r
WHERE jr.run_id = r.id
  AND jr.run_id = $1
  AND r.status = 'failed'
  AND jr.status IN ('pending', 'queued')
"#,
        )
        .bind(run_id.as_uuid())
        .bind(ts)
        .execute(self.pool)
        .await?
        .rows_affected();

        jobs_total += sqlx::query(
            r#"
UPDATE job_runs jr
SET status = 'failed',
    exit_code = COALESCE(jr.exit_code, 1),
    finished_at = COALESCE(jr.finished_at, r.finished_at, $2),
    error_message = COALESCE(jr.error_message, 'Run was marked succeeded while job was still running')
FROM runs r
WHERE jr.run_id = r.id
  AND jr.run_id = $1
  AND r.status = 'succeeded'
  AND jr.status = 'running'
"#,
        )
        .bind(run_id.as_uuid())
        .bind(ts)
        .execute(self.pool)
        .await?
        .rows_affected();

        jobs_total += sqlx::query(
            r#"
UPDATE job_runs jr
SET status = 'skipped',
    finished_at = COALESCE(jr.finished_at, r.finished_at, $2),
    error_message = COALESCE(jr.error_message, 'Run completed; job was not executed')
FROM runs r
WHERE jr.run_id = r.id
  AND jr.run_id = $1
  AND r.status = 'succeeded'
  AND jr.status IN ('pending', 'queued')
"#,
        )
        .bind(run_id.as_uuid())
        .bind(ts)
        .execute(self.pool)
        .await?
        .rows_affected();

        Ok((steps_total, jobs_total))
    }

    /// Update job run status to queued.
    pub async fn mark_queued(&self, id: JobRunId) -> Result<JobRun> {
        self.update_status(id, JobStatus::Queued, None, None).await
    }

    /// Update job run status to running with agent assignment.
    ///
    /// When `agent_snapshot` is `Some`, rows `agent_snapshot` / `agent_snapshot_captured_at` are updated
    /// (point-in-time audit). When `None`, existing snapshot columns are preserved.
    pub async fn mark_running(
        &self,
        id: JobRunId,
        agent_id: AgentId,
        agent_snapshot: Option<serde_json::Value>,
    ) -> Result<JobRun> {
        let now = Utc::now();

        let job_run = sqlx::query_as::<_, JobRun>(
            r#"
            UPDATE job_runs
            SET status = 'running',
                agent_id = $2,
                started_at = $3,
                agent_snapshot = COALESCE($4::jsonb, agent_snapshot),
                agent_snapshot_captured_at = CASE
                    WHEN $4::jsonb IS NOT NULL THEN $3
                    ELSE agent_snapshot_captured_at
                END
            WHERE id = $1
            RETURNING id, run_id, job_id, job_name, agent_id, status, attempt, exit_code, 
                      error_message, cache_hit, log_path, cache_key, outputs, started_at, finished_at, created_at,
                      pipeline_definition_sha256, workflow_definition_sha256, source_workflow,
                      agent_snapshot, agent_snapshot_captured_at, output_wrap_x25519_secret
            "#,
        )
        .bind(id.as_uuid())
        .bind(agent_id.as_uuid())
        .bind(now)
        .bind(agent_snapshot)
        .fetch_one(self.pool)
        .await?;

        Ok(job_run)
    }

    /// Mark job as completed.
    ///
    /// Only transitions from `pending`, `queued`, or `running` are applied. This prevents a late
    /// agent status update from clobbering engine-initiated `cancelled` / `skipped` (fail-fast,
    /// conditions, etc.) and matches the rule that agents complete work that is still active.
    ///
    /// If the job is already in a terminal state (including successful idempotent retries), returns
    /// the current row unchanged.
    pub async fn mark_completed(
        &self,
        id: JobRunId,
        success: bool,
        exit_code: Option<i32>,
        error_message: Option<&str>,
        outputs: Option<serde_json::Value>,
    ) -> Result<JobRun> {
        let now = Utc::now();
        let status = if success {
            JobStatus::Succeeded
        } else {
            JobStatus::Failed
        };

        let job_run = sqlx::query_as::<_, JobRun>(
            r#"
            UPDATE job_runs
            SET status = $2, exit_code = $3, error_message = $4, outputs = COALESCE($5, outputs), finished_at = $6
            WHERE id = $1
              AND status IN ('pending', 'queued', 'running')
            RETURNING id, run_id, job_id, job_name, agent_id, status, attempt, exit_code, 
                      error_message, cache_hit, log_path, cache_key, outputs, started_at, finished_at, created_at,
                      pipeline_definition_sha256, workflow_definition_sha256, source_workflow,
                      agent_snapshot, agent_snapshot_captured_at, output_wrap_x25519_secret
            "#,
        )
        .bind(id.as_uuid())
        .bind(status)
        .bind(exit_code)
        .bind(error_message)
        .bind(outputs)
        .bind(now)
        .fetch_optional(self.pool)
        .await?;

        if let Some(row) = job_run {
            return Ok(row);
        }

        let existing = self.get(id).await?;

        // Idempotent duplicate completion reports from the agent.
        if (success && existing.status == JobStatus::Succeeded)
            || (!success && existing.status == JobStatus::Failed)
        {
            return Ok(existing);
        }

        if matches!(existing.status, JobStatus::Cancelled | JobStatus::Skipped) {
            tracing::warn!(
                job_run_id = %id,
                incoming_success = success,
                existing_status = ?existing.status,
                "ignored agent job completion: job already cancelled or skipped"
            );
        }

        Ok(existing)
    }

    /// Mark job as skipped.
    pub async fn mark_skipped(&self, id: JobRunId, reason: Option<&str>) -> Result<JobRun> {
        self.update_status(id, JobStatus::Skipped, None, reason)
            .await
    }

    /// Mark job as cancelled.
    pub async fn mark_cancelled(&self, id: JobRunId, reason: Option<&str>) -> Result<JobRun> {
        self.update_status(
            id,
            JobStatus::Cancelled,
            None,
            Some(reason.unwrap_or("Cancelled by user")),
        )
        .await
    }

    /// Mark job as timed out.
    pub async fn mark_timed_out(&self, id: JobRunId) -> Result<JobRun> {
        self.update_status(
            id,
            JobStatus::TimedOut,
            None,
            Some("Job execution timed out"),
        )
        .await
    }

    /// Set cache hit information.
    pub async fn set_cache_hit(&self, id: JobRunId, cache_key: &str) -> Result<JobRun> {
        let job_run = sqlx::query_as::<_, JobRun>(
            r#"
            UPDATE job_runs
            SET cache_hit = true, cache_key = $2
            WHERE id = $1
            RETURNING id, run_id, job_id, job_name, agent_id, status, attempt, exit_code, 
                      error_message, cache_hit, log_path, cache_key, outputs, started_at, finished_at, created_at,
                      pipeline_definition_sha256, workflow_definition_sha256, source_workflow,
                      agent_snapshot, agent_snapshot_captured_at, output_wrap_x25519_secret
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
                started_at = NULL, finished_at = NULL, exit_code = NULL, error_message = NULL,
                agent_id = NULL, agent_snapshot = NULL, agent_snapshot_captured_at = NULL
            WHERE id = $1
            RETURNING id, run_id, job_id, job_name, agent_id, status, attempt, exit_code, 
                      error_message, cache_hit, log_path, cache_key, outputs, started_at, finished_at, created_at,
                      pipeline_definition_sha256, workflow_definition_sha256, source_workflow,
                      agent_snapshot, agent_snapshot_captured_at, output_wrap_x25519_secret
            "#,
        )
        .bind(id.as_uuid())
        .fetch_one(self.pool)
        .await?;

        Ok(job_run)
    }

    /// Load org/project/pipeline/run metadata for OIDC workload JWTs. Only returns a row when the
    /// job is **running** on the given agent (caller-controlled `agent_id` is not trusted until
    /// matched against `job_runs.agent_id`).
    pub async fn load_for_oidc_identity_token(
        &self,
        job_run_id: JobRunId,
        agent_id: AgentId,
    ) -> Result<Option<OidcJobIdentityRow>> {
        sqlx::query_as::<_, OidcJobIdentityRow>(
            r#"
            SELECT
                o.id AS org_id,
                o.slug AS org_slug,
                pr.id AS project_id,
                pr.slug AS project_slug,
                p.id AS pipeline_id,
                p.name AS pipeline_name,
                r.id AS run_id,
                jr.id AS job_run_id,
                r.branch,
                r.commit_sha,
                e.name AS environment_name
            FROM job_runs jr
            JOIN runs r ON r.id = jr.run_id
            JOIN pipelines p ON p.id = r.pipeline_id
            JOIN projects pr ON pr.id = p.project_id
            JOIN organizations o ON o.id = COALESCE(r.org_id, pr.org_id)
            LEFT JOIN environments e ON e.id = r.environment_id
            WHERE jr.id = $1
              AND jr.agent_id = $2
              AND jr.status = 'running'
              AND o.deleted_at IS NULL
              AND pr.deleted_at IS NULL
            "#,
        )
        .bind(job_run_id.as_uuid())
        .bind(agent_id.as_uuid())
        .fetch_optional(self.pool)
        .await
        .map_err(Into::into)
    }

    /// Load pipeline definition context for secret resolution (controller / jobs).
    pub async fn get_pipeline_context(
        &self,
        job_run_id: JobRunId,
    ) -> Result<Option<JobRunPipelineContext>> {
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

        Ok(row.map(
            |(org_id, project_id, pipeline_id, definition)| JobRunPipelineContext {
                org_id,
                project_id,
                pipeline_id,
                definition,
            },
        ))
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
                      error_message, cache_hit, log_path, cache_key, outputs, started_at, finished_at, created_at,
                      pipeline_definition_sha256, workflow_definition_sha256, source_workflow,
                      agent_snapshot, agent_snapshot_captured_at, output_wrap_x25519_secret
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

    /// Mark step as cancelled (e.g. agent stopped mid-step).
    pub async fn mark_cancelled(&self, id: StepRunId, reason: Option<&str>) -> Result<StepRun> {
        let now = Utc::now();

        let step_run = sqlx::query_as::<_, StepRun>(
            r#"
            UPDATE step_runs
            SET status = 'cancelled', error_message = $2, finished_at = $3
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
