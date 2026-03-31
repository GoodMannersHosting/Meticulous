//! Pipeline run routes.

use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use met_core::{
    ids::{PipelineId, RunId},
    models::{Run, RunStatus},
};
use met_store::repos::RunRepo;
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::{
    error::{ApiError, ApiResult},
    extractors::{Auth, Pagination, PaginatedResponse},
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/runs", get(list_runs))
        .route("/runs/{id}", get(get_run))
        .route("/runs/{id}/cancel", post(cancel_run))
        .route("/runs/{id}/retry", post(retry_run))
}

#[derive(Debug, Deserialize)]
pub struct ListRunsQuery {
    pipeline_id: Option<PipelineId>,
    status: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RunResponse {
    #[serde(flatten)]
    pub run: Run,
    pub duration_ms: Option<i64>,
}

impl From<Run> for RunResponse {
    fn from(run: Run) -> Self {
        let duration_ms = run.duration().map(|d| d.num_milliseconds());
        Self { run, duration_ms }
    }
}

#[instrument(skip(state))]
async fn list_runs(
    State(state): State<AppState>,
    Auth(_user): Auth,
    pagination: Pagination,
    axum::extract::Query(query): axum::extract::Query<ListRunsQuery>,
) -> ApiResult<Json<PaginatedResponse<RunResponse>>> {
    let repo = RunRepo::new(state.db());

    let pipeline_id = query.pipeline_id.ok_or_else(|| {
        ApiError::bad_request("pipeline_id query parameter is required")
    })?;

    let runs = repo
        .list_by_pipeline(pipeline_id, pagination.sql_limit(), 0)
        .await?;

    let response = PaginatedResponse::new(
        runs.into_iter().map(RunResponse::from).collect(),
        pagination.limit,
        |r| r.run.id.to_string(),
    );

    Ok(Json(response))
}

#[instrument(skip(state))]
async fn get_run(
    State(state): State<AppState>,
    Auth(_user): Auth,
    Path(id): Path<RunId>,
) -> ApiResult<Json<RunResponse>> {
    let repo = RunRepo::new(state.db());
    let run = repo.get(id).await?;
    Ok(Json(RunResponse::from(run)))
}

#[derive(Debug, Serialize)]
pub struct CancelRunResponse {
    pub run_id: RunId,
    pub status: String,
    pub message: String,
}

#[instrument(skip(state))]
async fn cancel_run(
    State(state): State<AppState>,
    Auth(_user): Auth,
    Path(id): Path<RunId>,
) -> ApiResult<Json<CancelRunResponse>> {
    let repo = RunRepo::new(state.db());

    let run = repo.get(id).await?;

    if run.status.is_terminal() {
        return Err(ApiError::conflict(format!(
            "Run {} is already in terminal state: {:?}",
            id, run.status
        )));
    }

    let updated = repo.update_status(id, RunStatus::Cancelled).await?;

    Ok(Json(CancelRunResponse {
        run_id: id,
        status: format!("{:?}", updated.status).to_lowercase(),
        message: "Run cancelled successfully".to_string(),
    }))
}

#[derive(Debug, Serialize)]
pub struct RetryRunResponse {
    pub original_run_id: RunId,
    pub new_run_id: RunId,
    pub run_number: i64,
}

#[instrument(skip(state))]
async fn retry_run(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(id): Path<RunId>,
) -> ApiResult<Json<RetryRunResponse>> {
    let repo = RunRepo::new(state.db());

    let original_run = repo.get(id).await?;

    if !original_run.status.is_terminal() {
        return Err(ApiError::conflict(format!(
            "Run {} is still in progress: {:?}",
            id, original_run.status
        )));
    }

    let new_run = repo
        .create(original_run.pipeline_id, original_run.trigger_id, &user.email)
        .await?;

    Ok(Json(RetryRunResponse {
        original_run_id: id,
        new_run_id: new_run.id,
        run_number: new_run.run_number,
    }))
}
