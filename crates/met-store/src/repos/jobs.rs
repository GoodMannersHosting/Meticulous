//! Pipeline job definitions (`jobs` table — DAG structure).

use met_core::ids::{JobId, PipelineId};
use sqlx::PgPool;

use crate::error::Result;

/// One job row used to render a pipeline DAG.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct JobDagNode {
    pub id: JobId,
    pub name: String,
    pub depends_on: Vec<String>,
}

/// Repository for `jobs` table reads.
pub struct JobRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> JobRepo<'a> {
    #[must_use]
    pub const fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn list_dag_for_pipeline(&self, pipeline_id: PipelineId) -> Result<Vec<JobDagNode>> {
        let rows = sqlx::query_as::<_, JobDagNode>(
            r#"
            SELECT id, name, depends_on
            FROM jobs
            WHERE pipeline_id = $1
            ORDER BY created_at, name
            "#,
        )
        .bind(pipeline_id.as_uuid())
        .fetch_all(self.pool)
        .await?;

        Ok(rows)
    }
}
