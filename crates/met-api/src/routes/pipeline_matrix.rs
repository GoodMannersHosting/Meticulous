//! Pipeline environment matrix view (Pipeline Environments UX plan).

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::get,
};
use met_core::ids::PipelineId;
use met_store::repos::{EnvironmentRepo, PipelineRepo};
use serde::Serialize;
use tracing::instrument;
use uuid::Uuid;

use crate::{
    error::{ApiError, ApiResult},
    extractors::Auth,
    project_access::effective_project_role_in_user_org,
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new().route("/pipelines/{id}/matrix", get(pipeline_matrix))
}

#[derive(Debug, Serialize)]
struct MatrixResponse {
    workflows: Vec<String>,
    environments: Vec<MatrixEnvironment>,
    cells: Vec<MatrixCell>,
}

#[derive(Debug, Serialize)]
struct MatrixEnvironment {
    id: Option<String>,
    name: String,
    tier: String,
}

#[derive(Debug, Serialize)]
struct MatrixCell {
    workflow: String,
    environment: Option<String>,
    run_id: Option<String>,
    run_number: Option<i64>,
    status: Option<String>,
    started_at: Option<String>,
    finished_at: Option<String>,
    duration_ms: Option<i64>,
    branch: Option<String>,
    triggered_by: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct MatrixCellRow {
    workflow_invocation_id: Option<String>,
    environment_id: Option<Uuid>,
    run_id: Uuid,
    run_number: i64,
    status: String,
    started_at: Option<chrono::DateTime<chrono::Utc>>,
    finished_at: Option<chrono::DateTime<chrono::Utc>>,
    duration_ms: Option<i64>,
    branch: Option<String>,
    triggered_by: String,
}

#[instrument(skip(state))]
async fn pipeline_matrix(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(pipeline_id): Path<PipelineId>,
) -> ApiResult<Json<MatrixResponse>> {
    let pipeline = PipelineRepo::new(state.db()).get(pipeline_id).await?;
    let _role =
        effective_project_role_in_user_org(state.db(), &user, pipeline.project_id).await?;

    let envs = EnvironmentRepo::new(state.db())
        .list_by_project(pipeline.project_id)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;

    let mut environments = vec![MatrixEnvironment {
        id: None,
        name: "Global".to_string(),
        tier: "global".to_string(),
    }];
    for e in &envs {
        environments.push(MatrixEnvironment {
            id: Some(e.id.to_string()),
            name: e.name.clone(),
            tier: e.tier.clone(),
        });
    }

    let cell_rows: Vec<MatrixCellRow> = sqlx::query_as(
        r#"
        WITH ranked AS (
            SELECT
                jr.workflow_invocation_id,
                r.environment_id,
                r.id AS run_id,
                r.run_number,
                r.status::text AS status,
                r.started_at,
                r.finished_at,
                EXTRACT(EPOCH FROM (r.finished_at - r.started_at))::bigint * 1000 AS duration_ms,
                r.branch,
                r.triggered_by,
                ROW_NUMBER() OVER (
                    PARTITION BY jr.workflow_invocation_id, r.environment_id
                    ORDER BY r.run_number DESC
                ) AS rn
            FROM job_runs jr
            INNER JOIN runs r ON r.id = jr.run_id
            WHERE r.pipeline_id = $1
              AND jr.workflow_invocation_id IS NOT NULL
        )
        SELECT workflow_invocation_id, environment_id, run_id, run_number,
               status, started_at, finished_at, duration_ms, branch, triggered_by
        FROM ranked WHERE rn = 1
        ORDER BY workflow_invocation_id, environment_id NULLS FIRST
        "#,
    )
    .bind(pipeline_id.as_uuid())
    .fetch_all(state.db())
    .await
    .map_err(|e| ApiError::internal(e.to_string()))?;

    let mut workflow_set: Vec<String> = Vec::new();
    for row in &cell_rows {
        if let Some(ref wf) = row.workflow_invocation_id {
            if !workflow_set.contains(wf) {
                workflow_set.push(wf.clone());
            }
        }
    }

    let env_name_map: std::collections::HashMap<Uuid, String> = envs
        .iter()
        .map(|e| (e.id, e.name.clone()))
        .collect();

    let cells: Vec<MatrixCell> = cell_rows
        .into_iter()
        .map(|row| MatrixCell {
            workflow: row.workflow_invocation_id.unwrap_or_default(),
            environment: row.environment_id.and_then(|eid| env_name_map.get(&eid).cloned()),
            run_id: Some(row.run_id.to_string()),
            run_number: Some(row.run_number),
            status: Some(row.status),
            started_at: row.started_at.map(|t| t.to_rfc3339()),
            finished_at: row.finished_at.map(|t| t.to_rfc3339()),
            duration_ms: row.duration_ms,
            branch: row.branch,
            triggered_by: Some(row.triggered_by),
        })
        .collect();

    Ok(Json(MatrixResponse {
        workflows: workflow_set,
        environments,
        cells,
    }))
}
