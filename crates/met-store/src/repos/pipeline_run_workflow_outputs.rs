//! Persist merged workflow invocation outputs for a pipeline run.

use sqlx::PgPool;
use uuid::Uuid;

use crate::StoreError;

pub struct PipelineRunWorkflowOutputsRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> PipelineRunWorkflowOutputsRepo<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Merge into existing row for `(run_id, workflow_invocation_id)`; **last wins** per JSON key.
    pub async fn upsert_merge(
        &self,
        run_id: Uuid,
        workflow_invocation_id: &str,
        producer_job_run_id: Uuid,
        public_outputs: serde_json::Value,
        secret_envelopes: serde_json::Value,
    ) -> Result<(), StoreError> {
        sqlx::query(
            r#"
            INSERT INTO pipeline_run_workflow_outputs (
                run_id, workflow_invocation_id, producer_job_run_id,
                public_outputs, secret_envelopes, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, NOW())
            ON CONFLICT (run_id, workflow_invocation_id) DO UPDATE SET
                producer_job_run_id = EXCLUDED.producer_job_run_id,
                public_outputs = pipeline_run_workflow_outputs.public_outputs || EXCLUDED.public_outputs,
                secret_envelopes = pipeline_run_workflow_outputs.secret_envelopes || EXCLUDED.secret_envelopes,
                updated_at = NOW()
            "#,
        )
        .bind(run_id)
        .bind(workflow_invocation_id)
        .bind(producer_job_run_id)
        .bind(public_outputs)
        .bind(secret_envelopes)
        .execute(self.pool)
        .await?;
        Ok(())
    }
}
