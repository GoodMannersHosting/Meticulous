//! Pipeline run routes.

use axum::{
    Json, Router,
    extract::{Path, Query, State, WebSocketUpgrade},
    response::IntoResponse,
    routing::{get, post},
};
use met_core::{
    ids::{JobRunId, PipelineId, ProjectId, RunId},
    models::{Run, RunStatus},
};
use met_store::{
    PgPool,
    repos::{JobRunRepo, LogCacheRepo, PipelineRepo, ProjectRepo, RunRepo, StepRunRepo},
};

use crate::scheduling_hints;
use serde::{Deserialize, Serialize};
use tracing::instrument;
use utoipa::ToSchema;

use crate::{
    error::{ApiError, ApiResult},
    extractors::{Auth, PaginatedResponse, Pagination, PaginationMeta},
    pipeline_execution,
    state::AppState,
};

/// Backfill non-terminal `job_runs` / `step_runs` when the parent run is already terminal.
async fn reconcile_terminal_run_children(db: &PgPool, run: &Run) -> Result<(), met_store::StoreError> {
    if !run.status.is_terminal() {
        return Ok(());
    }
    let (steps, jobs) = JobRunRepo::new(db)
        .reconcile_stale_jobs_and_steps_for_terminal_run(run.id, run.status, run.finished_at)
        .await?;
    if steps > 0 || jobs > 0 {
        tracing::debug!(
            run_id = %run.id,
            steps_updated = steps,
            jobs_updated = jobs,
            "reconciled stale job/step rows for terminal run"
        );
    }
    Ok(())
}

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
    project_id: Option<ProjectId>,
    status: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RunResponse {
    #[serde(flatten)]
    #[schema(value_type = Object)]
    pub run: Run,
    pub duration_ms: Option<i64>,
    /// Present when listing runs by `project_id` (all pipelines in a project).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pipeline_name: Option<String>,
}

impl From<Run> for RunResponse {
    fn from(run: Run) -> Self {
        let duration_ms = run.duration().map(|d| d.num_milliseconds());
        Self {
            run,
            duration_ms,
            pipeline_name: None,
        }
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/runs",
    params(
        ("pipeline_id" = Option<String>, Query, description = "Filter by pipeline ID (mutually exclusive with `project_id`)"),
        ("project_id" = Option<String>, Query, description = "List runs for all pipelines in this project (mutually exclusive with `pipeline_id`)"),
        ("status" = Option<String>, Query, description = "Filter by run status"),
        ("cursor" = Option<String>, Query, description = "Opaque pagination cursor; for runs this is the row offset as a decimal string (e.g. next page after 20 rows is `cursor=20`)"),
        ("limit" = Option<u32>, Query, description = "Items per page (alias: `per_page`; default and max come from API `http.pagination_*` config)"),
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
    Auth(user): Auth,
    pagination: Pagination,
    axum::extract::Query(query): axum::extract::Query<ListRunsQuery>,
) -> ApiResult<Json<PaginatedResponse<RunResponse>>> {
    let repo = RunRepo::new(state.db());

    if query.pipeline_id.is_some() && query.project_id.is_some() {
        return Err(ApiError::bad_request(
            "provide only one of `pipeline_id` or `project_id`",
        ));
    }

    let status_filter = parse_run_status_filter(query.status.as_deref())?;

    let offset = parse_runs_list_offset(pagination.cursor.as_deref());
    let limit = pagination.sql_limit();

    let mut items: Vec<RunResponse> = match (query.pipeline_id, query.project_id) {
        (Some(pipeline_id), None) => {
            let pipeline = PipelineRepo::new(state.db()).get(pipeline_id).await?;
            if !user.can_access_project(pipeline.project_id) {
                return Err(ApiError::forbidden("no access to this project"));
            }
            repo.list_by_pipeline(pipeline_id, status_filter, limit, offset)
                .await?
                .into_iter()
                .map(RunResponse::from)
                .collect()
        }
        (None, Some(project_id)) => {
            if !user.can_access_project(project_id) {
                return Err(ApiError::forbidden("no access to this project"));
            }
            repo.list_by_project(project_id, status_filter, limit, offset)
                .await?
                .into_iter()
                .map(|row| {
                    let mut resp = RunResponse::from(row.run);
                    resp.pipeline_name = Some(row.pipeline_name);
                    resp
                })
                .collect()
        }
        _ => {
            return Err(ApiError::bad_request(
                "`pipeline_id` or `project_id` query parameter is required",
            ));
        }
    };

    let limit = pagination.limit as usize;
    let fetched = items.len();
    let has_more = fetched > limit;
    if has_more {
        items.pop();
    }

    let count = items.len();
    let next_cursor = if has_more {
        Some((offset as usize + count).to_string())
    } else {
        None
    };

    let response = PaginatedResponse {
        data: items,
        pagination: PaginationMeta {
            next_cursor,
            has_more,
            count,
        },
    };

    Ok(Json(response))
}

fn parse_run_status_filter(raw: Option<&str>) -> ApiResult<Option<RunStatus>> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    serde_json::from_value(serde_json::Value::String(trimmed.to_owned()))
        .map_err(|_| ApiError::bad_request(format!("invalid run status: {trimmed}")))
}

/// `cursor` for run lists is a non-negative SQL `OFFSET` as a decimal string.
fn parse_runs_list_offset(cursor: Option<&str>) -> i64 {
    let Some(raw) = cursor else {
        return 0;
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return 0;
    }
    trimmed
        .parse::<i64>()
        .ok()
        .filter(|&o| o >= 0)
        .unwrap_or(0)
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
    let run_repo = RunRepo::new(state.db());

    let original_run = run_repo.get(id).await?;

    if !original_run.status.is_terminal() {
        return Err(ApiError::conflict(format!(
            "Run {} is still in progress: {:?}",
            id, original_run.status
        )));
    }

    let pipeline_repo = PipelineRepo::new(state.db());
    let pipeline = pipeline_repo
        .get(original_run.pipeline_id)
        .await?;

    if !user.can_access_project(pipeline.project_id) {
        return Err(ApiError::forbidden("no access to this project"));
    }

    let project = ProjectRepo::new(state.db())
        .get(pipeline.project_id)
        .await?;
    let org_id = project.org_id;

    let pipeline_ir = pipeline_execution::load_pipeline_ir_for_execution(
        &state,
        &pipeline,
        org_id,
        original_run.commit_sha.as_deref(),
        original_run.branch.as_deref(),
    )
    .await?;

    let new_run = run_repo
        .create_full(
            original_run.pipeline_id,
            org_id,
            original_run.trigger_id,
            &user.email,
            None,
            original_run.commit_sha.as_deref(),
            original_run.branch.as_deref(),
            None,
        )
        .await?;

    pipeline_execution::start_engine_for_existing_run_from_state(
        &state,
        org_id,
        new_run.id,
        pipeline_ir,
        pipeline.id,
        pipeline.project_id,
        "retry",
        None,
    )
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
    /// Best-effort explanation when a job is pending or queued (omitted when not applicable).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheduling_note: Option<String>,
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
    Auth(user): Auth,
    Path(id): Path<RunId>,
) -> ApiResult<Json<Vec<JobRunResponse>>> {
    let run = RunRepo::new(state.db()).get(id).await?;
    reconcile_terminal_run_children(state.db(), &run).await?;
    let pipeline = PipelineRepo::new(state.db()).get(run.pipeline_id).await?;

    let job_repo = JobRunRepo::new(state.db());
    let job_runs = job_repo.list_by_run(id).await?;
    let hint_jobs = job_runs.clone();

    let ir = scheduling_hints::try_parse_pipeline_ir(
        state.db(),
        user.org_id,
        pipeline.project_id,
        &pipeline.definition,
    )
    .await;

    let response: Vec<JobRunResponse> = job_runs
        .into_iter()
        .map(|j| {
            let duration_ms = j.duration().map(|d| d.num_milliseconds());
            let scheduling_note = ir
                .as_ref()
                .and_then(|ir| scheduling_hints::scheduling_hint(ir, &hint_jobs, &j));
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
                scheduling_note,
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
    let run = RunRepo::new(state.db()).get(run_id).await?;
    reconcile_terminal_run_children(state.db(), &run).await?;

    let jr = JobRunRepo::new(state.db()).get(job_run_id).await?;
    if jr.run_id != run_id {
        return Err(ApiError::not_found("job run not found for this pipeline run"));
    }

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
pub struct JobLogLine {
    pub run_id: String,
    pub job_run_id: String,
    pub step_run_id: Option<String>,
    pub line: String,
    pub level: String,
    pub timestamp: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LogsResponse {
    pub content: String,
    pub lines: Vec<JobLogLine>,
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
    Path((run_id, job_run_id)): Path<(RunId, JobRunId)>,
    Query(query): Query<LogsQuery>,
) -> ApiResult<Json<LogsResponse>> {
    let offset = query.offset.unwrap_or(0);
    let limit = query.limit.unwrap_or(10000);

    let run = RunRepo::new(state.db()).get(run_id).await?;
    reconcile_terminal_run_children(state.db(), &run).await?;

    let jr = JobRunRepo::new(state.db()).get(job_run_id).await?;
    if jr.run_id != run_id {
        return Err(ApiError::not_found("job run not found for this pipeline run"));
    }

    let repo = LogCacheRepo::new(state.db());
    let fetch_limit = (limit as i64).saturating_add(1);
    let entries = repo
        .list_for_job_run(
            job_run_id,
            fetch_limit,
            offset as i64,
            query.stream.as_deref(),
        )
        .await?;

    let has_more = entries.len() > limit as usize;
    let taken: Vec<_> = entries.into_iter().take(limit as usize).collect();
    let content = taken
        .iter()
        .map(|e| e.content.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    let lines: Vec<JobLogLine> = taken
        .iter()
        .map(|e| JobLogLine {
            run_id: e.run_id.to_string(),
            job_run_id: e.job_run_id.to_string(),
            step_run_id: e.step_run_id.map(|s| s.to_string()),
            line: e.content.clone(),
            level: if e.stream == "stderr" {
                "stderr".to_string()
            } else {
                "stdout".to_string()
            },
            timestamp: e.timestamp.to_rfc3339(),
        })
        .collect();

    Ok(Json(LogsResponse {
        content,
        lines,
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
    let run = RunRepo::new(state.db()).get(id).await?;
    reconcile_terminal_run_children(state.db(), &run).await?;
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

    Ok(Json(RunDagResponse { run_id: id, nodes }))
}

async fn run_events_websocket(
    State(_state): State<AppState>,
    Path(id): Path<RunId>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_run_events(socket, id))
}

async fn handle_run_events(mut socket: axum::extract::ws::WebSocket, run_id: RunId) {
    use axum::extract::ws::Message;

    let hello = serde_json::json!({
        "type": "connected",
        "run_id": run_id.to_string(),
    });

    if socket
        .send(Message::Text(hello.to_string().into()))
        .await
        .is_err()
    {
        return;
    }

    loop {
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;

        let ping = serde_json::json!({ "type": "ping" });
        if socket
            .send(Message::Text(ping.to_string().into()))
            .await
            .is_err()
        {
            break;
        }
    }
}
