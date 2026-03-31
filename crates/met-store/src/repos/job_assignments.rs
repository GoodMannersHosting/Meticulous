//! Job assignment repository.

use chrono::Utc;
use met_core::ids::{AgentId, JobAssignmentId, JobRunId};
use met_core::models::{JobAssignment, JobAssignmentStatus};
use sqlx::PgPool;

use crate::error::{Result, StoreError};

/// Repository for job assignment operations.
pub struct JobAssignmentRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> JobAssignmentRepo<'a> {
    /// Create a new job assignment repository.
    #[must_use]
    pub const fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Create a new job assignment.
    pub async fn create(&self, assignment: &JobAssignment) -> Result<JobAssignment> {
        let created = sqlx::query_as::<_, JobAssignment>(
            r#"
            INSERT INTO job_assignments (id, job_run_id, agent_id, status, accepted_at, started_at, completed_at, exit_code, failure_reason, attempt)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            RETURNING id, job_run_id, agent_id, status, accepted_at, started_at, completed_at, exit_code, failure_reason, attempt
            "#,
        )
        .bind(assignment.id.as_uuid())
        .bind(assignment.job_run_id.as_uuid())
        .bind(assignment.agent_id.as_uuid())
        .bind(&assignment.status)
        .bind(assignment.accepted_at)
        .bind(assignment.started_at)
        .bind(assignment.completed_at)
        .bind(assignment.exit_code)
        .bind(&assignment.failure_reason)
        .bind(assignment.attempt)
        .fetch_one(self.pool)
        .await?;

        Ok(created)
    }

    /// Get a job assignment by ID.
    pub async fn get(&self, id: JobAssignmentId) -> Result<JobAssignment> {
        sqlx::query_as::<_, JobAssignment>(
            r#"
            SELECT id, job_run_id, agent_id, status, accepted_at, started_at, completed_at, exit_code, failure_reason, attempt
            FROM job_assignments
            WHERE id = $1
            "#,
        )
        .bind(id.as_uuid())
        .fetch_optional(self.pool)
        .await?
        .ok_or_else(|| StoreError::not_found("job_assignment", id))
    }

    /// Get the latest assignment for a job run.
    pub async fn get_by_job_run(&self, job_run_id: JobRunId) -> Result<Option<JobAssignment>> {
        let assignment = sqlx::query_as::<_, JobAssignment>(
            r#"
            SELECT id, job_run_id, agent_id, status, accepted_at, started_at, completed_at, exit_code, failure_reason, attempt
            FROM job_assignments
            WHERE job_run_id = $1
            ORDER BY attempt DESC
            LIMIT 1
            "#,
        )
        .bind(job_run_id.as_uuid())
        .fetch_optional(self.pool)
        .await?;

        Ok(assignment)
    }

    /// List all assignments for a job run.
    pub async fn list_by_job_run(&self, job_run_id: JobRunId) -> Result<Vec<JobAssignment>> {
        let assignments = sqlx::query_as::<_, JobAssignment>(
            r#"
            SELECT id, job_run_id, agent_id, status, accepted_at, started_at, completed_at, exit_code, failure_reason, attempt
            FROM job_assignments
            WHERE job_run_id = $1
            ORDER BY attempt ASC
            "#,
        )
        .bind(job_run_id.as_uuid())
        .fetch_all(self.pool)
        .await?;

        Ok(assignments)
    }

    /// List active assignments for an agent.
    pub async fn list_active_by_agent(&self, agent_id: AgentId) -> Result<Vec<JobAssignment>> {
        let assignments = sqlx::query_as::<_, JobAssignment>(
            r#"
            SELECT id, job_run_id, agent_id, status, accepted_at, started_at, completed_at, exit_code, failure_reason, attempt
            FROM job_assignments
            WHERE agent_id = $1 AND status IN ('accepted', 'running')
            ORDER BY accepted_at ASC
            "#,
        )
        .bind(agent_id.as_uuid())
        .fetch_all(self.pool)
        .await?;

        Ok(assignments)
    }

    /// Update assignment status to running.
    pub async fn mark_started(&self, id: JobAssignmentId) -> Result<()> {
        let now = Utc::now();
        let result = sqlx::query(
            r#"
            UPDATE job_assignments
            SET status = 'running', started_at = $2
            WHERE id = $1 AND status = 'accepted'
            "#,
        )
        .bind(id.as_uuid())
        .bind(now)
        .execute(self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(StoreError::not_found("job_assignment", id));
        }

        Ok(())
    }

    /// Update assignment status to succeeded.
    pub async fn mark_succeeded(&self, id: JobAssignmentId, exit_code: i32) -> Result<()> {
        let now = Utc::now();
        let result = sqlx::query(
            r#"
            UPDATE job_assignments
            SET status = 'succeeded', completed_at = $2, exit_code = $3
            WHERE id = $1 AND status IN ('accepted', 'running')
            "#,
        )
        .bind(id.as_uuid())
        .bind(now)
        .bind(exit_code)
        .execute(self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(StoreError::not_found("job_assignment", id));
        }

        Ok(())
    }

    /// Update assignment status to failed.
    pub async fn mark_failed(
        &self,
        id: JobAssignmentId,
        exit_code: Option<i32>,
        reason: &str,
    ) -> Result<()> {
        let now = Utc::now();
        let result = sqlx::query(
            r#"
            UPDATE job_assignments
            SET status = 'failed', completed_at = $2, exit_code = $3, failure_reason = $4
            WHERE id = $1 AND status IN ('accepted', 'running')
            "#,
        )
        .bind(id.as_uuid())
        .bind(now)
        .bind(exit_code)
        .bind(reason)
        .execute(self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(StoreError::not_found("job_assignment", id));
        }

        Ok(())
    }

    /// Update assignment status to cancelled.
    pub async fn mark_cancelled(&self, id: JobAssignmentId, reason: &str) -> Result<()> {
        let now = Utc::now();
        let result = sqlx::query(
            r#"
            UPDATE job_assignments
            SET status = 'cancelled', completed_at = $2, failure_reason = $3
            WHERE id = $1 AND status IN ('accepted', 'running')
            "#,
        )
        .bind(id.as_uuid())
        .bind(now)
        .bind(reason)
        .execute(self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(StoreError::not_found("job_assignment", id));
        }

        Ok(())
    }

    /// Update assignment status to timed out.
    pub async fn mark_timed_out(&self, id: JobAssignmentId) -> Result<()> {
        let now = Utc::now();
        let result = sqlx::query(
            r#"
            UPDATE job_assignments
            SET status = 'timed_out', completed_at = $2, failure_reason = 'Job execution timed out'
            WHERE id = $1 AND status IN ('accepted', 'running')
            "#,
        )
        .bind(id.as_uuid())
        .bind(now)
        .execute(self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(StoreError::not_found("job_assignment", id));
        }

        Ok(())
    }

    /// Get the highest attempt number for a job run.
    pub async fn get_max_attempt(&self, job_run_id: JobRunId) -> Result<i32> {
        let (max_attempt,): (Option<i32>,) = sqlx::query_as(
            r#"
            SELECT MAX(attempt)
            FROM job_assignments
            WHERE job_run_id = $1
            "#,
        )
        .bind(job_run_id.as_uuid())
        .fetch_one(self.pool)
        .await?;

        Ok(max_attempt.unwrap_or(0))
    }

    /// Count assignments by status.
    pub async fn count_by_status(&self, status: JobAssignmentStatus) -> Result<i64> {
        let (count,): (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*)
            FROM job_assignments
            WHERE status = $1
            "#,
        )
        .bind(status)
        .fetch_one(self.pool)
        .await?;

        Ok(count)
    }
}
