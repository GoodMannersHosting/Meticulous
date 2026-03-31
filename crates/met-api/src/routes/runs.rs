//! Pipeline run routes.

use axum::{
    extract::{Path, Query, State, WebSocketUpgrade},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use met_core::{
    ids::{PipelineId, RunId},
    models::{Run, RunStatus},
};
use met_store::repos::{JobRunRepo, RunRepo, StepRunRepo};
use serde::{Deserialize, Serialize};
use tracing::instrument;
use utoipa::ToSchema;

use crate::{
    error::{ApiError, ApiResult},
    extractors::{Auth, Pagination, PaginatedResponse},
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/runs", get(list_runs))
        .route("/runs/{id}", get(get_run))
        .route("/runs/{id}/jobs", get(get_run_jobs))
        .route("/runs/{id}/jobs/{job_run_id}/steps", get(get_job_steps))
        .route("/runs/{id}/jobs/{job_run_id}/logs", get(get_job_logs))
        .route("/runs/{id}/dag", get(get_run_dag))
        .route("/runs/{id}/cancel", post(cancel_run))
        .route("/runs/{id}/retry", post(retry_run))
        .route("/runs/{id}/events", get(run_events_websocket))
}

#[derive(Debug, Deserialize)]
pub struct ListRunsQuery {
    pipeline_id: Option<PipelineId>,
    status: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RunResponse {
    #[serde(flatten)]
    #[schema(value_type = Object)]
    pub run: Run,
    pub duration_ms: Option<i64>,
}

impl From<Run> for RunResponse {
    fn from(run: Run) -> Self {
        let duration_ms = run.duration().map(|d| d.num_milliseconds());
        Self { run, duration_ms }
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/runs",
    params(
        ("pipeline_id" = Option<String>, Query, description = "Filter by pipeline ID"),
        ("status" = Option<String>, Query, description = "Filter by run status"),
        ("cursor" = Option<String>, Query, description = "Pagination cursor"),
        ("limit" = Option<u32>, Query, description = "Items per page"),
    ),
    responses(
        (status = 200, description = "List of runs", body = serde_json::Value),
        (status = 401, description = "Unauthorized"),
    ),
    tag = "runs",
)]
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

#[utoipa::path(
    get,
    path = "/api/v1/runs/{id}",
    params(("id" = String, Path, description = "Run ID")),
    responses(
        (status = 200, description = "Run details", body = RunResponse),
        (status = 404, description = "Run not found"),
    ),
    tag = "runs",
)]
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

#[derive(Debug, Serialize, ToSchema)]
pub struct CancelRunResponse {
    #[schema(value_type = String)]
    pub run_id: RunId,
    pub status: String,
    pub message: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/runs/{id}/cancel",
    params(("id" = String, Path, description = "Run ID")),
    responses(
        (status = 200, description = "Run cancelled", body = CancelRunResponse),
        (status = 404, description = "Run not found"),
        (status = 409, description = "Run already in terminal state"),
    ),
    tag = "runs",
)]
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

#[derive(Debug, Serialize, ToSchema)]
pub struct RetryRunResponse {
    #[schema(value_type = String)]
    pub original_run_id: RunId,
    #[schema(value_type = String)]
    pub new_run_id: RunId,
    pub run_number: i64,
}

#[utoipa::path(
    post,
    path = "/api/v1/runs/{id}/retry",
    params(("id" = String, Path, description = "Run ID")),
    responses(
        (status = 200, description = "Run retried", body = RetryRunResponse),
        (status = 404, description = "Run not found"),
        (status = 409, description = "Run still in progress"),
    ),
    tag = "runs",
)]
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

/// Get detailed run with all job runs.
#[derive(Debug, Serialize)]
pub struct RunWithJobsResponse {
    #[serde(flatten)]
    pub run: Run,
    pub duration_ms: Option<i64>,
    pub jobs: Vec<JobRunResponse>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct JobRunResponse {
    #[schema(value_type = String)]
    pub id: met_core::ids::JobRunId,
    #[schema(value_type = String)]
    pub job_id: met_core::ids::JobId,
    pub job_name: String,
    pub status: String,
    pub attempt: i32,
    #[schema(value_type = Option<String>)]
    pub agent_id: Option<met_core::ids::AgentId>,
    pub exit_code: Option<i32>,
    pub error_message: Option<String>,
    pub cache_hit: bool,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub finished_at: Option<chrono::DateTime<chrono::Utc>>,
    pub duration_ms: Option<i64>,
}

#[utoipa::path(
    get,
    path = "/api/v1/runs/{id}/jobs",
    params(("id" = String, Path, description = "Run ID")),
    responses(
        (status = 200, description = "Job runs for this run", body = Vec<JobRunResponse>),
        (status = 404, description = "Run not found"),
    ),
    tag = "runs",
)]
#[instrument(skip(state))]
async fn get_run_jobs(
    State(state): State<AppState>,
    Auth(_user): Auth,
    Path(id): Path<RunId>,
) -> ApiResult<Json<Vec<JobRunResponse>>> {
    let job_repo = JobRunRepo::new(state.db());
    let job_runs = job_repo.list_by_run(id).await?;

    let response: Vec<JobRunResponse> = job_runs
        .into_iter()
        .map(|j| {
            let duration_ms = j.duration().map(|d| d.num_milliseconds());
            JobRunResponse {
                id: j.id,
                job_id: j.job_id,
                job_name: j.job_name,
                status: format!("{:?}", j.status).to_lowercase(),
                attempt: j.attempt,
                agent_id: j.agent_id,
                exit_code: j.exit_code,
                error_message: j.error_message,
                cache_hit: j.cache_hit,
                started_at: j.started_at,
                finished_at: j.finished_at,
                duration_ms,
            }
        })
        .collect();

    Ok(Json(response))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct StepRunResponse {
    #[schema(value_type = String)]
    pub id: met_core::ids::StepRunId,
    #[schema(value_type = String)]
    pub step_id: met_core::ids::StepId,
    pub step_name: String,
    pub status: String,
    pub exit_code: Option<i32>,
    pub error_message: Option<String>,
    pub log_path: Option<String>,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub finished_at: Option<chrono::DateTime<chrono::Utc>>,
    pub duration_ms: Option<i64>,
}

#[utoipa::path(
    get,
    path = "/api/v1/runs/{id}/jobs/{job_run_id}/steps",
    params(
        ("id" = String, Path, description = "Run ID"),
        ("job_run_id" = String, Path, description = "Job run ID"),
    ),
    responses(
        (status = 200, description = "Step runs for this job", body = Vec<StepRunResponse>),
        (status = 404, description = "Not found"),
    ),
    tag = "runs",
)]
#[instrument(skip(state))]
async fn get_job_steps(
    State(state): State<AppState>,
    Auth(_user): Auth,
    Path((run_id, job_run_id)): Path<(RunId, met_core::ids::JobRunId)>,
) -> ApiResult<Json<Vec<StepRunResponse>>> {
    let step_repo = StepRunRepo::new(state.db());
    let step_runs = step_repo.list_by_job_run(job_run_id).await?;

    let response: Vec<StepRunResponse> = step_runs
        .into_iter()
        .map(|s| {
            let duration_ms = s.duration().map(|d| d.num_milliseconds());
            StepRunResponse {
                id: s.id,
                step_id: s.step_id,
                step_name: s.step_name,
                status: format!("{:?}", s.status).to_lowercase(),
                exit_code: s.exit_code,
                error_message: s.error_message,
                log_path: s.log_path,
                started_at: s.started_at,
                finished_at: s.finished_at,
                duration_ms,
            }
        })
        .collect();

    Ok(Json(response))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct LogsQuery {
    pub offset: Option<u64>,
    pub limit: Option<u64>,
    pub stream: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LogsResponse {
    pub content: String,
    pub offset: u64,
    pub has_more: bool,
}

#[utoipa::path(
    get,
    path = "/api/v1/runs/{id}/jobs/{job_run_id}/logs",
    params(
        ("id" = String, Path, description = "Run ID"),
        ("job_run_id" = String, Path, description = "Job run ID"),
        ("offset" = Option<u64>, Query, description = "Log offset"),
        ("limit" = Option<u64>, Query, description = "Log line limit"),
        ("stream" = Option<String>, Query, description = "Log stream filter"),
    ),
    responses(
        (status = 200, description = "Job logs", body = LogsResponse),
        (status = 404, description = "Not found"),
    ),
    tag = "runs",
)]
#[instrument(skip(state))]
async fn get_job_logs(
    State(state): State<AppState>,
    Auth(_user): Auth,
    Path((_run_id, job_run_id)): Path<(RunId, met_core::ids::JobRunId)>,
    Query(query): Query<LogsQuery>,
) -> ApiResult<Json<LogsResponse>> {
    let offset = query.offset.unwrap_or(0);
    let limit = query.limit.unwrap_or(10000);

    let rows: Vec<(String,)> = sqlx::query_as(
        r#"
        SELECT content
        FROM run_logs
        WHERE job_run_id = $1
        ORDER BY sequence ASC
        OFFSET $2
        LIMIT $3
        "#,
    )
    .bind(job_run_id.as_uuid())
    .bind(offset as i64)
    .bind(limit as i64 + 1)
    .fetch_all(state.db())
    .await
    .map_err(met_store::StoreError::from)?;

    let has_more = rows.len() > limit as usize;
    let content = rows
        .into_iter()
        .take(limit as usize)
        .map(|(c,)| c)
        .collect::<Vec<_>>()
        .join("\n");

    Ok(Json(LogsResponse {
        content,
        offset,
        has_more,
    }))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DagNodeResponse {
    pub job_id: String,
    pub job_name: String,
    pub status: String,
    pub depends_on: Vec<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RunDagResponse {
    #[schema(value_type = String)]
    pub run_id: RunId,
    pub nodes: Vec<DagNodeResponse>,
}

#[utoipa::path(
    get,
    path = "/api/v1/runs/{id}/dag",
    params(("id" = String, Path, description = "Run ID")),
    responses(
        (status = 200, description = "Run DAG", body = RunDagResponse),
        (status = 404, description = "Run not found"),
    ),
    tag = "runs",
)]
#[instrument(skip(state))]
async fn get_run_dag(
    State(state): State<AppState>,
    Auth(_user): Auth,
    Path(id): Path<RunId>,
) -> ApiResult<Json<RunDagResponse>> {
    let _run = RunRepo::new(state.db()).get(id).await?;
    let job_repo = JobRunRepo::new(state.db());
    let job_runs = job_repo.list_by_run(id).await?;

    let nodes: Vec<DagNodeResponse> = job_runs
        .into_iter()
        .map(|j| DagNodeResponse {
            job_id: j.job_id.to_string(),
            job_name: j.job_name,
            status: format!("{:?}", j.status).to_lowercase(),
            depends_on: Vec::new(),
        })
        .collect();

    Ok(Json(RunDagResponse {
        run_id: id,
        nodes,
    }))
}

async fn run_events_websocket(
    State(_state): State<AppState>,
    Path(id): Path<RunId>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_run_events(socket, id))
}

async fn handle_run_events(
    mut socket: axum::extract::ws::WebSocket,
    run_id: RunId,
) {
    use axum::extract::ws::Message;

    let hello = serde_json::json!({
        "type": "connected",
        "run_id": run_id.to_string(),
    });

    if socket.send(Message::Text(hello.to_string().into())).await.is_err() {
        return;
    }

    loop {
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
        
        let ping = serde_json::json!({ "type": "ping" });
        if socket.send(Message::Text(ping.to_string().into())).await.is_err() {
            break;
        }
    }
}
