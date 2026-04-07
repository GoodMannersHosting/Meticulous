//! Persisted exec telemetry: each binary observed during a run (aggregated).

use met_core::ids::{JobRunId, RunId};
use sqlx::PgPool;

use crate::error::Result;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RunBinaryExecutionAgg {
    pub job_run_id: JobRunId,
    pub binary_path: String,
    pub binary_sha256: String,
    pub execution_count: i64,
}

/// Aggregated exec rows with job label (for blast-radius / footprint APIs).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RunBinaryFootprintRow {
    pub job_name: String,
    pub step_name: String,
    pub binary_path: String,
    pub binary_sha256: String,
    pub execution_count: i64,
}

pub struct RunBinaryExecutionRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> RunBinaryExecutionRepo<'a> {
    #[must_use]
    pub const fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Distinct `(job_run_id, path, sha256)` rows with exec counts.
    pub async fn list_aggregated_by_run(
        &self,
        run_id: RunId,
    ) -> Result<Vec<RunBinaryExecutionAgg>> {
        let rows = sqlx::query_as::<_, RunBinaryExecutionAgg>(
            r#"
            SELECT
                job_run_id,
                binary_path,
                binary_sha256,
                COUNT(*)::bigint AS execution_count
            FROM run_binary_executions
            WHERE run_id = $1 AND job_run_id IS NOT NULL
            GROUP BY job_run_id, binary_path, binary_sha256
            ORDER BY job_run_id, binary_path
            "#,
        )
        .bind(run_id.as_uuid())
        .fetch_all(self.pool)
        .await?;

        Ok(rows)
    }

    /// `(job_name, path, sha256)` aggregates for a run.
    pub async fn list_footprint_by_run(&self, run_id: RunId) -> Result<Vec<RunBinaryFootprintRow>> {
        let rows = sqlx::query_as::<_, RunBinaryFootprintRow>(
            r#"
            SELECT
                COALESCE(jr.job_name, '') AS job_name,
                COALESCE(sr.step_name, '') AS step_name,
                rbe.binary_path,
                rbe.binary_sha256,
                COUNT(*)::bigint AS execution_count
            FROM run_binary_executions rbe
            LEFT JOIN job_runs jr ON jr.id = rbe.job_run_id
            LEFT JOIN step_runs sr ON sr.id = rbe.step_run_id
            WHERE rbe.run_id = $1 AND rbe.job_run_id IS NOT NULL
            GROUP BY jr.job_name, sr.step_name, rbe.binary_path, rbe.binary_sha256
            ORDER BY jr.job_name, sr.step_name, rbe.binary_path
            "#,
        )
        .bind(run_id.as_uuid())
        .fetch_all(self.pool)
        .await?;

        Ok(rows)
    }
}
