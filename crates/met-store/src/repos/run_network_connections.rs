//! Observed network connections during a run (`run_network_connections`).

use chrono::{DateTime, Utc};
use met_core::ids::{JobRunId, RunId};
use sqlx::PgPool;

use crate::error::Result;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RunNetworkConnectionRow {
    pub job_run_id: Option<JobRunId>,
    pub job_name: Option<String>,
    pub dst_ip: String,
    pub dst_port: i32,
    pub protocol: String,
    pub direction: String,
    pub connected_at: DateTime<Utc>,
    pub binary_path: Option<String>,
    pub binary_sha256: Option<String>,
}

pub struct RunNetworkConnectionRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> RunNetworkConnectionRepo<'a> {
    #[must_use]
    pub const fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn list_for_run(
        &self,
        run_id: RunId,
        limit: i64,
    ) -> Result<Vec<RunNetworkConnectionRow>> {
        let rows = sqlx::query_as::<_, RunNetworkConnectionRow>(
            r#"
            SELECT
                n.job_run_id,
                jr.job_name,
                n.dst_ip::text AS dst_ip,
                n.dst_port,
                n.protocol,
                n.direction,
                n.connected_at,
                n.binary_path,
                n.binary_sha256
            FROM run_network_connections n
            LEFT JOIN job_runs jr ON jr.id = n.job_run_id
            WHERE n.run_id = $1
            ORDER BY n.connected_at ASC
            LIMIT $2
            "#,
        )
        .bind(run_id.as_uuid())
        .bind(limit)
        .fetch_all(self.pool)
        .await?;

        Ok(rows)
    }
}
