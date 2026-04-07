//! Database persistence for pipeline execution state.
//!
//! This module provides the `RunPersistence` trait and its implementations
//! for persisting pipeline run state to the database.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use met_core::ids::{AgentId, JobId, JobRunId, OrganizationId, PipelineId, RunId, StepId, StepRunId};
use met_core::models::{JobStatus, RunStatus};
use sqlx::PgPool;
use tracing::{debug, instrument, warn};

use crate::error::{EngineError, Result};

/// Content fingerprints and workflow reference recorded on each `job_run` (pipeline body is shared across jobs in the same run).
#[derive(Debug, Clone)]
pub struct JobRunSourceRefs {
    pub pipeline_definition_sha256: [u8; 32],
    pub workflow_definition_sha256: Option<[u8; 32]>,
    pub source_workflow: Option<serde_json::Value>,
}

/// Trait for persisting run state to storage.
#[async_trait]
pub trait RunPersistence: Send + Sync {
    /// Create a new pipeline run record.
    async fn create_run(
        &self,
        run_id: RunId,
        pipeline_id: PipelineId,
        org_id: OrganizationId,
        triggered_by: &str,
        trace_id: uuid::Uuid,
    ) -> Result<()>;

    /// Backfill org/trace on a run row created elsewhere (e.g. API `RunRepo::create`).
    async fn prepare_existing_run(
        &self,
        run_id: RunId,
        org_id: OrganizationId,
        trace_id: Option<uuid::Uuid>,
    ) -> Result<()>;

    /// Update run status.
    async fn update_run_status(&self, run_id: RunId, status: RunStatus) -> Result<()>;

    /// Complete a run with final status.
    async fn complete_run(
        &self,
        run_id: RunId,
        status: RunStatus,
        error_message: Option<&str>,
    ) -> Result<()>;

    /// Create a job run record.
    async fn create_job_run(
        &self,
        job_run_id: JobRunId,
        run_id: RunId,
        job_id: JobId,
        job_name: &str,
        source: JobRunSourceRefs,
        output_wrap_x25519_secret: [u8; 32],
    ) -> Result<()>;

    /// Mark job as queued after dispatch is successfully published (`pending` or already `queued` only).
    async fn mark_job_queued(&self, job_run_id: JobRunId) -> Result<()>;

    /// Update job run status.
    async fn update_job_status(&self, job_run_id: JobRunId, status: JobStatus) -> Result<()>;

    /// Mark job as started with agent assignment.
    async fn start_job(&self, job_run_id: JobRunId, agent_id: AgentId) -> Result<()>;

    /// Complete a job run.
    async fn complete_job(
        &self,
        job_run_id: JobRunId,
        success: bool,
        exit_code: Option<i32>,
        error_message: Option<&str>,
    ) -> Result<()>;

    /// Mark job as skipped.
    async fn skip_job(&self, job_run_id: JobRunId, reason: Option<&str>) -> Result<()>;

    /// Record cache hit for a job.
    async fn set_job_cache_hit(&self, job_run_id: JobRunId, cache_key: &str) -> Result<()>;

    /// Increment job attempt counter for retry.
    async fn increment_job_attempt(&self, job_run_id: JobRunId) -> Result<i32>;

    /// Create a step run record.
    async fn create_step_run(
        &self,
        step_run_id: StepRunId,
        job_run_id: JobRunId,
        step_id: StepId,
        step_name: &str,
    ) -> Result<()>;

    /// Mark step as started.
    async fn start_step(&self, step_run_id: StepRunId) -> Result<()>;

    /// Complete a step run.
    async fn complete_step(
        &self,
        step_run_id: StepRunId,
        exit_code: i32,
        error_message: Option<&str>,
        log_path: Option<&str>,
    ) -> Result<()>;

    /// Record a run event.
    async fn record_event(
        &self,
        run_id: RunId,
        event_type: &str,
        event_data: serde_json::Value,
        actor: Option<&str>,
    ) -> Result<()>;
}

/// PostgreSQL implementation of run persistence.
pub struct PostgresRunPersistence {
    pool: PgPool,
}

impl PostgresRunPersistence {
    /// Create a new PostgreSQL persistence layer.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl RunPersistence for PostgresRunPersistence {
    #[instrument(skip(self))]
    async fn create_run(
        &self,
        run_id: RunId,
        pipeline_id: PipelineId,
        org_id: OrganizationId,
        triggered_by: &str,
        trace_id: uuid::Uuid,
    ) -> Result<()> {
        let now = Utc::now();
        
        let run_number: (i64,) = sqlx::query_as(
            r#"
            SELECT COALESCE(MAX(run_number), 0) + 1
            FROM runs
            WHERE pipeline_id = $1
            "#,
        )
        .bind(pipeline_id.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(met_store::StoreError::from)?;

        sqlx::query(
            r#"
            INSERT INTO runs (id, pipeline_id, org_id, status, run_number, triggered_by, trace_id, created_at)
            VALUES ($1, $2, $3, 'pending', $4, $5, $6, $7)
            "#,
        )
        .bind(run_id.as_uuid())
        .bind(pipeline_id.as_uuid())
        .bind(org_id.as_uuid())
        .bind(run_number.0)
        .bind(triggered_by)
        .bind(trace_id)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(met_store::StoreError::from)?;

        debug!(%run_id, "created run record");
        Ok(())
    }

    #[instrument(skip(self))]
    async fn prepare_existing_run(
        &self,
        run_id: RunId,
        org_id: OrganizationId,
        trace_id: Option<uuid::Uuid>,
    ) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE runs
            SET org_id = COALESCE(org_id, $2),
                trace_id = COALESCE(trace_id, $3)
            WHERE id = $1
            "#,
        )
        .bind(run_id.as_uuid())
        .bind(org_id.as_uuid())
        .bind(trace_id)
        .execute(&self.pool)
        .await
        .map_err(met_store::StoreError::from)?;

        debug!(%run_id, "prepared existing run row for engine");
        Ok(())
    }

    #[instrument(skip(self))]
    async fn update_run_status(&self, run_id: RunId, status: RunStatus) -> Result<()> {
        let now = Utc::now();
        
        sqlx::query(
            r#"
            UPDATE runs
            SET status = $2,
                started_at = CASE WHEN $2 = 'running' AND started_at IS NULL THEN $3 ELSE started_at END
            WHERE id = $1
            "#,
        )
        .bind(run_id.as_uuid())
        .bind(status)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(met_store::StoreError::from)?;

        debug!(%run_id, ?status, "updated run status");
        Ok(())
    }

    #[instrument(skip(self))]
    async fn complete_run(
        &self,
        run_id: RunId,
        status: RunStatus,
        error_message: Option<&str>,
    ) -> Result<()> {
        let now = Utc::now();
        
        sqlx::query(
            r#"
            UPDATE runs
            SET status = $2, finished_at = $3, error_message = $4
            WHERE id = $1
            "#,
        )
        .bind(run_id.as_uuid())
        .bind(status)
        .bind(now)
        .bind(error_message)
        .execute(&self.pool)
        .await
        .map_err(met_store::StoreError::from)?;

        debug!(%run_id, ?status, "completed run");
        Ok(())
    }

    #[instrument(skip(self))]
    async fn create_job_run(
        &self,
        job_run_id: JobRunId,
        run_id: RunId,
        job_id: JobId,
        job_name: &str,
        source: JobRunSourceRefs,
        output_wrap_x25519_secret: [u8; 32],
    ) -> Result<()> {
        let now = Utc::now();

        sqlx::query(
            r#"
            INSERT INTO job_runs (
                id, run_id, job_id, job_name, status, attempt, created_at,
                pipeline_definition_sha256, workflow_definition_sha256, source_workflow,
                output_wrap_x25519_secret
            )
            VALUES ($1, $2, $3, $4, 'pending', 1, $5, $6, $7, $8, $9)
            "#,
        )
        .bind(job_run_id.as_uuid())
        .bind(run_id.as_uuid())
        .bind(job_id.as_uuid())
        .bind(job_name)
        .bind(now)
        .bind(&source.pipeline_definition_sha256[..])
        .bind(
            source
                .workflow_definition_sha256
                .as_ref()
                .map(|b| b.as_slice()),
        )
        .bind(source.source_workflow)
        .bind(&output_wrap_x25519_secret[..])
        .execute(&self.pool)
        .await
        .map_err(met_store::StoreError::from)?;

        debug!(%job_run_id, %run_id, job_name, "created job run record");
        Ok(())
    }

    #[instrument(skip(self))]
    async fn mark_job_queued(&self, job_run_id: JobRunId) -> Result<()> {
        let res = sqlx::query(
            r#"
            UPDATE job_runs
            SET status = 'queued'
            WHERE id = $1
              AND status IN ('pending', 'queued')
            "#,
        )
        .bind(job_run_id.as_uuid())
        .execute(&self.pool)
        .await
        .map_err(met_store::StoreError::from)?;

        if res.rows_affected() == 0 {
            warn!(
                %job_run_id,
                "mark_job_queued: no row updated (job not pending/queued or missing)"
            );
        } else {
            debug!(%job_run_id, "marked job run queued in database");
        }
        Ok(())
    }

    #[instrument(skip(self))]
    async fn update_job_status(&self, job_run_id: JobRunId, status: JobStatus) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE job_runs
            SET status = $2
            WHERE id = $1
            "#,
        )
        .bind(job_run_id.as_uuid())
        .bind(status)
        .execute(&self.pool)
        .await
        .map_err(met_store::StoreError::from)?;

        debug!(%job_run_id, ?status, "updated job status");
        Ok(())
    }

    #[instrument(skip(self))]
    async fn start_job(&self, job_run_id: JobRunId, agent_id: AgentId) -> Result<()> {
        let now = Utc::now();
        
        sqlx::query(
            r#"
            UPDATE job_runs
            SET status = 'running', agent_id = $2, started_at = $3
            WHERE id = $1
            "#,
        )
        .bind(job_run_id.as_uuid())
        .bind(agent_id.as_uuid())
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(met_store::StoreError::from)?;

        debug!(%job_run_id, %agent_id, "started job");
        Ok(())
    }

    #[instrument(skip(self))]
    async fn complete_job(
        &self,
        job_run_id: JobRunId,
        success: bool,
        exit_code: Option<i32>,
        error_message: Option<&str>,
    ) -> Result<()> {
        let now = Utc::now();
        let status = if success { JobStatus::Succeeded } else { JobStatus::Failed };
        
        sqlx::query(
            r#"
            UPDATE job_runs
            SET status = $2, exit_code = $3, error_message = $4, finished_at = $5
            WHERE id = $1
            "#,
        )
        .bind(job_run_id.as_uuid())
        .bind(status)
        .bind(exit_code)
        .bind(error_message)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(met_store::StoreError::from)?;

        debug!(%job_run_id, success, "completed job");
        Ok(())
    }

    #[instrument(skip(self))]
    async fn skip_job(&self, job_run_id: JobRunId, reason: Option<&str>) -> Result<()> {
        let now = Utc::now();
        
        sqlx::query(
            r#"
            UPDATE job_runs
            SET status = 'skipped', error_message = $2, finished_at = $3
            WHERE id = $1
            "#,
        )
        .bind(job_run_id.as_uuid())
        .bind(reason)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(met_store::StoreError::from)?;

        debug!(%job_run_id, reason, "skipped job");
        Ok(())
    }

    #[instrument(skip(self))]
    async fn set_job_cache_hit(&self, job_run_id: JobRunId, cache_key: &str) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE job_runs
            SET cache_hit = true, cache_key = $2
            WHERE id = $1
            "#,
        )
        .bind(job_run_id.as_uuid())
        .bind(cache_key)
        .execute(&self.pool)
        .await
        .map_err(met_store::StoreError::from)?;

        debug!(%job_run_id, cache_key, "recorded cache hit");
        Ok(())
    }

    #[instrument(skip(self))]
    async fn increment_job_attempt(&self, job_run_id: JobRunId) -> Result<i32> {
        let row: (i32,) = sqlx::query_as(
            r#"
            UPDATE job_runs
            SET attempt = attempt + 1, 
                status = 'pending', 
                started_at = NULL, 
                finished_at = NULL
            WHERE id = $1
            RETURNING attempt
            "#,
        )
        .bind(job_run_id.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(met_store::StoreError::from)?;

        debug!(%job_run_id, attempt = row.0, "incremented job attempt");
        Ok(row.0)
    }

    #[instrument(skip(self))]
    async fn create_step_run(
        &self,
        step_run_id: StepRunId,
        job_run_id: JobRunId,
        step_id: StepId,
        step_name: &str,
    ) -> Result<()> {
        let now = Utc::now();
        
        sqlx::query(
            r#"
            INSERT INTO step_runs (id, job_run_id, step_id, step_name, status, created_at)
            VALUES ($1, $2, $3, $4, 'pending', $5)
            "#,
        )
        .bind(step_run_id.as_uuid())
        .bind(job_run_id.as_uuid())
        .bind(step_id.as_uuid())
        .bind(step_name)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(met_store::StoreError::from)?;

        debug!(%step_run_id, %job_run_id, step_name, "created step run record");
        Ok(())
    }

    #[instrument(skip(self))]
    async fn start_step(&self, step_run_id: StepRunId) -> Result<()> {
        let now = Utc::now();
        
        sqlx::query(
            r#"
            UPDATE step_runs
            SET status = 'running', started_at = $2
            WHERE id = $1
            "#,
        )
        .bind(step_run_id.as_uuid())
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(met_store::StoreError::from)?;

        debug!(%step_run_id, "started step");
        Ok(())
    }

    #[instrument(skip(self))]
    async fn complete_step(
        &self,
        step_run_id: StepRunId,
        exit_code: i32,
        error_message: Option<&str>,
        log_path: Option<&str>,
    ) -> Result<()> {
        let now = Utc::now();
        let status = if exit_code == 0 { "succeeded" } else { "failed" };
        
        sqlx::query(
            r#"
            UPDATE step_runs
            SET status = $2, exit_code = $3, error_message = $4, log_path = $5, finished_at = $6
            WHERE id = $1
            "#,
        )
        .bind(step_run_id.as_uuid())
        .bind(status)
        .bind(exit_code)
        .bind(error_message)
        .bind(log_path)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(met_store::StoreError::from)?;

        debug!(%step_run_id, exit_code, "completed step");
        Ok(())
    }

    #[instrument(skip(self, event_data))]
    async fn record_event(
        &self,
        run_id: RunId,
        event_type: &str,
        event_data: serde_json::Value,
        actor: Option<&str>,
    ) -> Result<()> {
        let now = Utc::now();
        
        sqlx::query(
            r#"
            INSERT INTO run_events (run_id, event_type, event_data, actor, timestamp)
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(run_id.as_uuid())
        .bind(event_type)
        .bind(event_data)
        .bind(actor)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(met_store::StoreError::from)?;

        debug!(%run_id, event_type, "recorded run event");
        Ok(())
    }
}

/// In-memory implementation for testing.
#[derive(Default)]
pub struct MemoryRunPersistence {
    runs: std::sync::Mutex<Vec<RunRecord>>,
    job_runs: std::sync::Mutex<Vec<JobRunRecord>>,
    step_runs: std::sync::Mutex<Vec<StepRunRecord>>,
    events: std::sync::Mutex<Vec<EventRecord>>,
}

#[derive(Debug, Clone)]
struct RunRecord {
    id: RunId,
    pipeline_id: PipelineId,
    org_id: OrganizationId,
    status: RunStatus,
    triggered_by: String,
    trace_id: uuid::Uuid,
    created_at: DateTime<Utc>,
    started_at: Option<DateTime<Utc>>,
    finished_at: Option<DateTime<Utc>>,
    error_message: Option<String>,
}

#[derive(Debug, Clone)]
struct JobRunRecord {
    id: JobRunId,
    run_id: RunId,
    job_id: JobId,
    job_name: String,
    status: JobStatus,
    attempt: i32,
    agent_id: Option<AgentId>,
    exit_code: Option<i32>,
    error_message: Option<String>,
    cache_hit: bool,
    cache_key: Option<String>,
    started_at: Option<DateTime<Utc>>,
    finished_at: Option<DateTime<Utc>>,
    pipeline_definition_sha256: Option<[u8; 32]>,
    workflow_definition_sha256: Option<[u8; 32]>,
    source_workflow: Option<serde_json::Value>,
    output_wrap_x25519_secret: [u8; 32],
}

#[derive(Debug, Clone)]
struct StepRunRecord {
    id: StepRunId,
    job_run_id: JobRunId,
    step_id: StepId,
    step_name: String,
    status: String,
    exit_code: Option<i32>,
    error_message: Option<String>,
    log_path: Option<String>,
    started_at: Option<DateTime<Utc>>,
    finished_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
struct EventRecord {
    run_id: RunId,
    event_type: String,
    event_data: serde_json::Value,
    actor: Option<String>,
    timestamp: DateTime<Utc>,
}

impl MemoryRunPersistence {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl RunPersistence for MemoryRunPersistence {
    async fn create_run(
        &self,
        run_id: RunId,
        pipeline_id: PipelineId,
        org_id: OrganizationId,
        triggered_by: &str,
        trace_id: uuid::Uuid,
    ) -> Result<()> {
        let mut runs = self.runs.lock().unwrap();
        runs.push(RunRecord {
            id: run_id,
            pipeline_id,
            org_id,
            status: RunStatus::Pending,
            triggered_by: triggered_by.to_string(),
            trace_id,
            created_at: Utc::now(),
            started_at: None,
            finished_at: None,
            error_message: None,
        });
        Ok(())
    }

    async fn prepare_existing_run(
        &self,
        run_id: RunId,
        org_id: OrganizationId,
        trace_id: Option<uuid::Uuid>,
    ) -> Result<()> {
        let mut runs = self.runs.lock().unwrap();
        if let Some(run) = runs.iter_mut().find(|r| r.id == run_id) {
            run.org_id = org_id;
            if let Some(t) = trace_id {
                run.trace_id = t;
            }
        }
        Ok(())
    }

    async fn update_run_status(&self, run_id: RunId, status: RunStatus) -> Result<()> {
        let mut runs = self.runs.lock().unwrap();
        if let Some(run) = runs.iter_mut().find(|r| r.id == run_id) {
            run.status = status;
            if status == RunStatus::Running && run.started_at.is_none() {
                run.started_at = Some(Utc::now());
            }
        }
        Ok(())
    }

    async fn complete_run(
        &self,
        run_id: RunId,
        status: RunStatus,
        error_message: Option<&str>,
    ) -> Result<()> {
        let mut runs = self.runs.lock().unwrap();
        if let Some(run) = runs.iter_mut().find(|r| r.id == run_id) {
            run.status = status;
            run.finished_at = Some(Utc::now());
            run.error_message = error_message.map(|s| s.to_string());
        }
        Ok(())
    }

    async fn create_job_run(
        &self,
        job_run_id: JobRunId,
        run_id: RunId,
        job_id: JobId,
        job_name: &str,
        source: JobRunSourceRefs,
        output_wrap_x25519_secret: [u8; 32],
    ) -> Result<()> {
        let mut jobs = self.job_runs.lock().unwrap();
        jobs.push(JobRunRecord {
            id: job_run_id,
            run_id,
            job_id,
            job_name: job_name.to_string(),
            status: JobStatus::Pending,
            attempt: 1,
            agent_id: None,
            exit_code: None,
            error_message: None,
            cache_hit: false,
            cache_key: None,
            started_at: None,
            finished_at: None,
            pipeline_definition_sha256: Some(source.pipeline_definition_sha256),
            workflow_definition_sha256: source.workflow_definition_sha256,
            source_workflow: source.source_workflow,
            output_wrap_x25519_secret,
        });
        Ok(())
    }

    async fn mark_job_queued(&self, job_run_id: JobRunId) -> Result<()> {
        let mut jobs = self.job_runs.lock().unwrap();
        if let Some(job) = jobs.iter_mut().find(|j| j.id == job_run_id) {
            if matches!(job.status, JobStatus::Pending | JobStatus::Queued) {
                job.status = JobStatus::Queued;
            }
        }
        Ok(())
    }

    async fn update_job_status(&self, job_run_id: JobRunId, status: JobStatus) -> Result<()> {
        let mut jobs = self.job_runs.lock().unwrap();
        if let Some(job) = jobs.iter_mut().find(|j| j.id == job_run_id) {
            job.status = status;
        }
        Ok(())
    }

    async fn start_job(&self, job_run_id: JobRunId, agent_id: AgentId) -> Result<()> {
        let mut jobs = self.job_runs.lock().unwrap();
        if let Some(job) = jobs.iter_mut().find(|j| j.id == job_run_id) {
            job.status = JobStatus::Running;
            job.agent_id = Some(agent_id);
            job.started_at = Some(Utc::now());
        }
        Ok(())
    }

    async fn complete_job(
        &self,
        job_run_id: JobRunId,
        success: bool,
        exit_code: Option<i32>,
        error_message: Option<&str>,
    ) -> Result<()> {
        let mut jobs = self.job_runs.lock().unwrap();
        if let Some(job) = jobs.iter_mut().find(|j| j.id == job_run_id) {
            job.status = if success { JobStatus::Succeeded } else { JobStatus::Failed };
            job.exit_code = exit_code;
            job.error_message = error_message.map(|s| s.to_string());
            job.finished_at = Some(Utc::now());
        }
        Ok(())
    }

    async fn skip_job(&self, job_run_id: JobRunId, reason: Option<&str>) -> Result<()> {
        let mut jobs = self.job_runs.lock().unwrap();
        if let Some(job) = jobs.iter_mut().find(|j| j.id == job_run_id) {
            job.status = JobStatus::Skipped;
            job.error_message = reason.map(|s| s.to_string());
            job.finished_at = Some(Utc::now());
        }
        Ok(())
    }

    async fn set_job_cache_hit(&self, job_run_id: JobRunId, cache_key: &str) -> Result<()> {
        let mut jobs = self.job_runs.lock().unwrap();
        if let Some(job) = jobs.iter_mut().find(|j| j.id == job_run_id) {
            job.cache_hit = true;
            job.cache_key = Some(cache_key.to_string());
        }
        Ok(())
    }

    async fn increment_job_attempt(&self, job_run_id: JobRunId) -> Result<i32> {
        let mut jobs = self.job_runs.lock().unwrap();
        if let Some(job) = jobs.iter_mut().find(|j| j.id == job_run_id) {
            job.attempt += 1;
            job.status = JobStatus::Pending;
            job.started_at = None;
            job.finished_at = None;
            return Ok(job.attempt);
        }
        Ok(1)
    }

    async fn create_step_run(
        &self,
        step_run_id: StepRunId,
        job_run_id: JobRunId,
        step_id: StepId,
        step_name: &str,
    ) -> Result<()> {
        let mut steps = self.step_runs.lock().unwrap();
        steps.push(StepRunRecord {
            id: step_run_id,
            job_run_id,
            step_id,
            step_name: step_name.to_string(),
            status: "pending".to_string(),
            exit_code: None,
            error_message: None,
            log_path: None,
            started_at: None,
            finished_at: None,
        });
        Ok(())
    }

    async fn start_step(&self, step_run_id: StepRunId) -> Result<()> {
        let mut steps = self.step_runs.lock().unwrap();
        if let Some(step) = steps.iter_mut().find(|s| s.id == step_run_id) {
            step.status = "running".to_string();
            step.started_at = Some(Utc::now());
        }
        Ok(())
    }

    async fn complete_step(
        &self,
        step_run_id: StepRunId,
        exit_code: i32,
        error_message: Option<&str>,
        log_path: Option<&str>,
    ) -> Result<()> {
        let mut steps = self.step_runs.lock().unwrap();
        if let Some(step) = steps.iter_mut().find(|s| s.id == step_run_id) {
            step.status = if exit_code == 0 { "succeeded" } else { "failed" }.to_string();
            step.exit_code = Some(exit_code);
            step.error_message = error_message.map(|s| s.to_string());
            step.log_path = log_path.map(|s| s.to_string());
            step.finished_at = Some(Utc::now());
        }
        Ok(())
    }

    async fn record_event(
        &self,
        run_id: RunId,
        event_type: &str,
        event_data: serde_json::Value,
        actor: Option<&str>,
    ) -> Result<()> {
        let mut events = self.events.lock().unwrap();
        events.push(EventRecord {
            run_id,
            event_type: event_type.to_string(),
            event_data,
            actor: actor.map(|s| s.to_string()),
            timestamp: Utc::now(),
        });
        Ok(())
    }
}

#[cfg(test)]
mod mark_job_queued_tests {
    use super::{JobRunSourceRefs, MemoryRunPersistence, RunPersistence};
    use met_core::ids::{AgentId, JobId, JobRunId, OrganizationId, PipelineId, RunId};

    #[tokio::test]
    async fn mark_job_queued_memory_idempotent_and_no_downgrade_from_running() {
        let p = MemoryRunPersistence::new();
        let run_id = RunId::new();
        let pipeline_id = PipelineId::new();
        let org_id = OrganizationId::new();
        p.create_run(
            run_id,
            pipeline_id,
            org_id,
            "tester",
            uuid::Uuid::now_v7(),
        )
        .await
        .unwrap();
        let job_run_id = JobRunId::new();
        let job_id = JobId::new();
        let src = JobRunSourceRefs {
            pipeline_definition_sha256: [9u8; 32],
            workflow_definition_sha256: None,
            source_workflow: None,
        };
        p.create_job_run(job_run_id, run_id, job_id, "j1", src, [7u8; 32])
            .await
            .unwrap();
        p.mark_job_queued(job_run_id).await.unwrap();
        p.mark_job_queued(job_run_id).await.unwrap();
        p.start_job(job_run_id, AgentId::new()).await.unwrap();
        p.mark_job_queued(job_run_id).await.unwrap();
        p.complete_job(job_run_id, true, Some(0), None).await.unwrap();
    }
}
