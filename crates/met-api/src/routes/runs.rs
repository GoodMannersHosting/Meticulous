//! Pipeline run routes.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use axum::{
    Json, Router,
    extract::{Path, Query, State, WebSocketUpgrade},
    response::IntoResponse,
    routing::{get, post},
};
use met_core::{
    ids::{JobId, JobRunId, PipelineId, ProjectId, RunId, StepRunId},
    models::{JobRun, Run, RunStatus},
};
use met_store::{
    PgPool,
    repos::{
        DefinitionSnapshotRepo, JobAssignmentRepo, JobDagNode, JobRepo, JobRunRepo, LogCacheRepo,
        PipelineRepo, ProjectRepo, RunBinaryExecutionAgg, RunBinaryExecutionRepo,
        RunNetworkConnectionRepo, RunRepo, StepRunRepo,
    },
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
async fn reconcile_terminal_run_children(
    db: &PgPool,
    run: &Run,
) -> Result<(), met_store::StoreError> {
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
        .route(
            "/runs/{id}/jobs/{job_run_id}/snapshots",
            get(get_job_run_snapshots),
        )
        .route("/runs/{id}/jobs/{job_run_id}/steps", get(get_job_steps))
        .route(
            "/runs/{id}/jobs/{job_run_id}/assignments",
            get(get_job_assignments),
        )
        .route("/runs/{id}/jobs/{job_run_id}/logs", get(get_job_logs))
        .route("/runs/{id}/dag", get(get_run_dag))
        .route("/runs/{id}/footprint", get(get_run_footprint))
        .route("/runs/{id}/cancel", post(cancel_run))
        .route("/runs/{id}/retry", post(retry_run))
        .route("/runs/{id}/events", get(run_events_websocket))
}

#[derive(Debug, Deserialize)]
pub struct ListRunsQuery {
    pipeline_id: Option<PipelineId>,
    project_id: Option<ProjectId>,
    status: Option<String>,
    /// When set with `pipeline_id`, return at most one run with this `run_number` (for compare / lookup).
    run_number: Option<i64>,
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
    /// Run number of `parent_run_id` when set (for display without a second fetch).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_run_number: Option<i64>,
    /// When set, UIs should prefer this over `run.status` for the primary badge (e.g. run is `running` but no job has reached agent execution yet).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<String>)]
    pub status_display: Option<RunStatus>,
}

impl From<Run> for RunResponse {
    fn from(run: Run) -> Self {
        let duration_ms = run.duration().map(|d| d.num_milliseconds());
        Self {
            run,
            duration_ms,
            pipeline_name: None,
            parent_run_number: None,
            status_display: None,
        }
    }
}

/// When the run row is `running` but every `job_run` is still `pending` or `queued`, show **Queued** on run lists/detail.
async fn enrich_run_responses_status_display(
    pool: &PgPool,
    responses: &mut [RunResponse],
) -> ApiResult<()> {
    let ids: Vec<RunId> = responses.iter().map(|r| r.run.id).collect();
    if ids.is_empty() {
        return Ok(());
    }
    let rollups = JobRunRepo::new(pool).rollup_by_run_ids(&ids).await?;
    for resp in responses.iter_mut() {
        if resp.run.status != RunStatus::Running {
            continue;
        }
        if let Some(rollup) = rollups.get(&resp.run.id) {
            if rollup.job_count > 0 && !rollup.any_running {
                resp.status_display = Some(RunStatus::Queued);
            }
        }
    }
    Ok(())
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
        ("run_number" = Option<i64>, Query, description = "When set with `pipeline_id`, return the single run with this run number (for compare-to-previous)"),
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

    if query.run_number.is_some() && query.project_id.is_some() {
        return Err(ApiError::bad_request(
            "`run_number` can only be used with `pipeline_id`",
        ));
    }

    if let Some(run_number) = query.run_number {
        let Some(pipeline_id) = query.pipeline_id else {
            return Err(ApiError::bad_request("`run_number` requires `pipeline_id`"));
        };
        let pipeline = PipelineRepo::new(state.db()).get(pipeline_id).await?;
        if !user.can_access_project(pipeline.project_id) {
            return Err(ApiError::forbidden("no access to this project"));
        }
        let mut items: Vec<RunResponse> = repo
            .find_by_pipeline_and_run_number(pipeline_id, run_number)
            .await?
            .into_iter()
            .map(RunResponse::from)
            .collect();
        enrich_run_responses_status_display(state.db(), &mut items).await?;
        let count = items.len();
        return Ok(Json(PaginatedResponse {
            data: items,
            pagination: PaginationMeta {
                next_cursor: None,
                has_more: false,
                count,
            },
        }));
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

    enrich_run_responses_status_display(state.db(), &mut items).await?;

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
    trimmed.parse::<i64>().ok().filter(|&o| o >= 0).unwrap_or(0)
}

fn job_run_snapshot_key(bytes: &Option<Vec<u8>>) -> Option<[u8; 32]> {
    let b = bytes.as_ref()?;
    <[u8; 32]>::try_from(b.as_slice()).ok()
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
    let parent_run_number = match run.parent_run_id {
        Some(pid) => repo.get(pid).await.ok().map(|p| p.run_number),
        None => None,
    };
    let duration_ms = run.duration().map(|d| d.num_milliseconds());
    let mut responses = vec![RunResponse {
        run,
        duration_ms,
        pipeline_name: None,
        parent_run_number,
        status_display: None,
    }];
    enrich_run_responses_status_display(state.db(), &mut responses).await?;
    Ok(Json(responses.pop().expect("one element")))
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
    let pipeline = pipeline_repo.get(original_run.pipeline_id).await?;

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
            Some(id),
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
    /// SHA-256 (hex) of the pipeline definition JSON snapshot used for this run (see `definition_snapshots`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pipeline_definition_sha256: Option<String>,
    /// SHA-256 (hex) of the reusable workflow definition when this job came from one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_definition_sha256: Option<String>,
    /// Resolved reusable workflow reference: `scope`, `name`, `version`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_workflow: Option<serde_json::Value>,
    /// Best-effort explanation when a job is pending or queued (omitted when not applicable).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheduling_note: Option<String>,
    /// Agent/host audit JSON captured when the job entered `running` (investigation / compromise lineage).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Object)]
    pub agent_snapshot: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_snapshot_captured_at: Option<chrono::DateTime<chrono::Utc>>,
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
                pipeline_definition_sha256: j
                    .pipeline_definition_sha256
                    .as_ref()
                    .filter(|b| b.len() == 32)
                    .map(hex::encode),
                workflow_definition_sha256: j
                    .workflow_definition_sha256
                    .as_ref()
                    .filter(|b| b.len() == 32)
                    .map(hex::encode),
                source_workflow: j.source_workflow.clone(),
                scheduling_note,
                agent_snapshot: j.agent_snapshot.clone(),
                agent_snapshot_captured_at: j.agent_snapshot_captured_at,
            }
        })
        .collect();

    Ok(Json(response))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct JobRunSnapshotsResponse {
    /// Pipeline definition JSON for this job run (from `definition_snapshots`), if recorded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pipeline_definition: Option<serde_json::Value>,
    /// Reusable workflow definition JSON for this job run, if recorded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_definition: Option<serde_json::Value>,
}

#[utoipa::path(
    get,
    path = "/api/v1/runs/{id}/jobs/{job_run_id}/snapshots",
    params(
        ("id" = String, Path, description = "Run ID"),
        ("job_run_id" = String, Path, description = "Job run ID"),
    ),
    responses(
        (status = 200, description = "Resolved snapshot bodies", body = JobRunSnapshotsResponse),
        (status = 404, description = "Run or job run not found"),
    ),
    tag = "runs",
)]
#[instrument(skip(state))]
async fn get_job_run_snapshots(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path((run_id, job_run_id)): Path<(RunId, JobRunId)>,
) -> ApiResult<Json<JobRunSnapshotsResponse>> {
    let db = state.db();
    let run = RunRepo::new(db).get(run_id).await?;
    reconcile_terminal_run_children(db, &run).await?;
    let pipeline = PipelineRepo::new(db).get(run.pipeline_id).await?;
    if !user.can_access_project(pipeline.project_id) {
        return Err(ApiError::forbidden("no access to this project"));
    }

    let jr = JobRunRepo::new(db).get(job_run_id).await?;
    if jr.run_id != run_id {
        return Err(ApiError::not_found(
            "job run not found for this pipeline run",
        ));
    }

    let pipeline_definition = match job_run_snapshot_key(&jr.pipeline_definition_sha256) {
        Some(k) => DefinitionSnapshotRepo::get_json(db, &k).await?,
        None => None,
    };
    let workflow_definition = match job_run_snapshot_key(&jr.workflow_definition_sha256) {
        Some(k) => DefinitionSnapshotRepo::get_json(db, &k).await?,
        None => None,
    };

    Ok(Json(JobRunSnapshotsResponse {
        pipeline_definition,
        workflow_definition,
    }))
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
        return Err(ApiError::not_found(
            "job run not found for this pipeline run",
        ));
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

#[derive(Debug, Serialize, ToSchema)]
pub struct JobAssignmentResponse {
    #[schema(value_type = String)]
    pub id: met_core::ids::JobAssignmentId,
    #[schema(value_type = String)]
    pub job_run_id: met_core::ids::JobRunId,
    #[schema(value_type = String)]
    pub agent_id: met_core::ids::AgentId,
    pub status: String,
    pub attempt: i32,
    pub accepted_at: chrono::DateTime<chrono::Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_reason: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/v1/runs/{id}/jobs/{job_run_id}/assignments",
    params(
        ("id" = String, Path, description = "Run ID"),
        ("job_run_id" = String, Path, description = "Job run ID"),
    ),
    responses(
        (status = 200, description = "Agent dispatch attempts for this job run", body = Vec<JobAssignmentResponse>),
        (status = 404, description = "Not found"),
    ),
    tag = "runs",
)]
#[instrument(skip(state))]
async fn get_job_assignments(
    State(state): State<AppState>,
    Auth(_user): Auth,
    Path((run_id, job_run_id)): Path<(RunId, JobRunId)>,
) -> ApiResult<Json<Vec<JobAssignmentResponse>>> {
    let run = RunRepo::new(state.db()).get(run_id).await?;
    reconcile_terminal_run_children(state.db(), &run).await?;

    let jr = JobRunRepo::new(state.db()).get(job_run_id).await?;
    if jr.run_id != run_id {
        return Err(ApiError::not_found(
            "job run not found for this pipeline run",
        ));
    }

    let rows = JobAssignmentRepo::new(state.db())
        .list_by_job_run(job_run_id)
        .await?;

    let out: Vec<JobAssignmentResponse> = rows
        .into_iter()
        .map(|a| JobAssignmentResponse {
            id: a.id,
            job_run_id: a.job_run_id,
            agent_id: a.agent_id,
            status: format!("{:?}", a.status).to_lowercase(),
            attempt: a.attempt,
            accepted_at: a.accepted_at,
            started_at: a.started_at,
            completed_at: a.completed_at,
            exit_code: a.exit_code,
            failure_reason: a.failure_reason,
        })
        .collect();

    Ok(Json(out))
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
        return Err(ApiError::not_found(
            "job run not found for this pipeline run",
        ));
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
            // Match `StepRunId` JSON everywhere else (`srun_<uuid>`), not bare `Uuid::to_string()`.
            step_run_id: e.step_run_id.map(|u| StepRunId::from_uuid(u).to_string()),
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
pub struct ExecutedBinarySummary {
    pub binary_path: String,
    pub sha256: String,
    pub execution_count: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DagNodeResponse {
    pub job_id: String,
    pub job_name: String,
    pub status: String,
    pub depends_on: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub executed_binaries: Vec<ExecutedBinarySummary>,
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
    let pipeline = PipelineRepo::new(state.db()).get(run.pipeline_id).await?;

    let job_runs = JobRunRepo::new(state.db()).list_by_run(id).await?;
    let mut runs_by_job_id: HashMap<JobId, Vec<&JobRun>> = HashMap::new();
    for jr in &job_runs {
        runs_by_job_id.entry(jr.job_id).or_default().push(jr);
    }

    let binary_rows = RunBinaryExecutionRepo::new(state.db())
        .list_aggregated_by_run(id)
        .await?;
    let mut binaries_by_job_run: HashMap<JobRunId, Vec<RunBinaryExecutionAgg>> = HashMap::new();
    for row in binary_rows {
        binaries_by_job_run
            .entry(row.job_run_id)
            .or_default()
            .push(row);
    }

    let dag_jobs = JobRepo::new(state.db())
        .list_dag_for_pipeline(run.pipeline_id)
        .await?;

    let nodes = if dag_jobs.is_empty() {
        build_dag_nodes_from_definition(&pipeline.definition, &job_runs, &binaries_by_job_run)
    } else {
        build_dag_nodes_from_db_jobs(dag_jobs, &runs_by_job_id, &binaries_by_job_run)
    };

    Ok(Json(RunDagResponse { run_id: id, nodes }))
}

fn job_defs_from_pipeline_definition(def: &serde_json::Value) -> Vec<(String, Vec<String>)> {
    let Some(arr) = def.get("jobs").and_then(|v| v.as_array()) else {
        return Vec::new();
    };

    arr.iter()
        .filter_map(|j| {
            let name = j.get("name")?.as_str()?.to_string();
            let deps: Vec<String> = j
                .get("depends_on")
                .and_then(|d| d.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(std::string::ToString::to_string))
                        .collect()
                })
                .unwrap_or_default();
            Some((name, deps))
        })
        .collect()
}

fn merge_binaries_for_job_runs(
    job_run_ids: &[JobRunId],
    by_jrid: &HashMap<JobRunId, Vec<RunBinaryExecutionAgg>>,
) -> Vec<ExecutedBinarySummary> {
    let mut acc: BTreeMap<(String, String), i64> = BTreeMap::new();
    for jid in job_run_ids {
        if let Some(rows) = by_jrid.get(jid) {
            for r in rows {
                *acc.entry((r.binary_path.clone(), r.binary_sha256.clone()))
                    .or_default() += r.execution_count;
            }
        }
    }
    acc.into_iter()
        .map(
            |((binary_path, sha256), execution_count)| ExecutedBinarySummary {
                binary_path,
                sha256,
                execution_count,
            },
        )
        .collect()
}

fn latest_job_run<'a>(jrs: &[&'a JobRun]) -> Option<&'a JobRun> {
    jrs.iter().copied().max_by_key(|j| j.attempt)
}

fn build_dag_nodes_from_db_jobs(
    dag_jobs: Vec<JobDagNode>,
    runs_by_job_id: &HashMap<JobId, Vec<&JobRun>>,
    binaries_by_job_run: &HashMap<JobRunId, Vec<RunBinaryExecutionAgg>>,
) -> Vec<DagNodeResponse> {
    dag_jobs
        .into_iter()
        .map(|j| {
            let jrs = runs_by_job_id
                .get(&j.id)
                .map(|v| v.as_slice())
                .unwrap_or(&[]);
            let status = latest_job_run(jrs)
                .map(|jr| format!("{:?}", jr.status).to_lowercase())
                .unwrap_or_else(|| "pending".to_string());
            let jr_ids: Vec<JobRunId> = jrs.iter().map(|jr| jr.id).collect();
            let executed_binaries = merge_binaries_for_job_runs(&jr_ids, binaries_by_job_run);
            DagNodeResponse {
                job_id: j.id.to_string(),
                job_name: j.name,
                status,
                depends_on: j.depends_on,
                executed_binaries,
            }
        })
        .collect()
}

fn build_dag_nodes_from_definition(
    def: &serde_json::Value,
    job_runs: &[JobRun],
    binaries_by_job_run: &HashMap<JobRunId, Vec<RunBinaryExecutionAgg>>,
) -> Vec<DagNodeResponse> {
    job_defs_from_pipeline_definition(def)
        .into_iter()
        .map(|(name, deps)| {
            let jrs_for_name: Vec<&JobRun> =
                job_runs.iter().filter(|jr| jr.job_name == name).collect();
            let status = latest_job_run(&jrs_for_name)
                .map(|jr| format!("{:?}", jr.status).to_lowercase())
                .unwrap_or_else(|| "pending".to_string());
            let jr_ids: Vec<JobRunId> = jrs_for_name.iter().map(|jr| jr.id).collect();
            let executed_binaries = merge_binaries_for_job_runs(&jr_ids, binaries_by_job_run);
            DagNodeResponse {
                job_id: format!("def:{name}"),
                job_name: name,
                status,
                depends_on: deps,
                executed_binaries,
            }
        })
        .collect()
}

const FOOTPRINT_MAX_DIRS: usize = 120;
const FOOTPRINT_MAX_ENTRIES_PER_DIR: usize = 48;
const FOOTPRINT_MAX_NETWORK: i64 = 500;

#[derive(Debug, Serialize, ToSchema)]
pub struct FootprintBinaryRow {
    pub job_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step_name: Option<String>,
    pub binary_path: String,
    pub sha256: String,
    pub execution_count: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct FootprintNetworkRow {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_name: Option<String>,
    pub dst_ip: String,
    pub dst_port: i32,
    pub protocol: String,
    pub direction: String,
    pub connected_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binary_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binary_sha256: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct FootprintDirectoryEntry {
    pub binary_path: String,
    pub sha256: String,
    pub execution_count: i64,
    pub job_names: Vec<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct FootprintDirectoryGroup {
    pub directory: String,
    pub entries: Vec<FootprintDirectoryEntry>,
    pub entries_truncated: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RunFootprintResponse {
    #[schema(value_type = String)]
    pub run_id: RunId,
    pub executed_binaries: Vec<FootprintBinaryRow>,
    pub network_connections: Vec<FootprintNetworkRow>,
    pub filesystem_by_directory: Vec<FootprintDirectoryGroup>,
    pub filesystem_directories_truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filesystem_more_directory_count: Option<u32>,
}

fn footprint_job_step_label(job_name: &str, step_name: Option<&str>) -> String {
    let job = job_name.trim();
    match step_name.map(str::trim).filter(|s| !s.is_empty()) {
        Some(step) if !job.is_empty() => format!("{job} · {step}"),
        Some(step) => step.to_string(),
        None if !job.is_empty() => job.to_string(),
        None => "—".to_string(),
    }
}

fn format_footprint_parent_dir(binary_path: &str) -> String {
    let p = std::path::Path::new(binary_path.trim());
    let Some(parent) = p.parent() else {
        return ".".to_string();
    };
    let s = parent.to_string_lossy();
    if s.is_empty() {
        return ".".to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut prev_slash = false;
    for ch in s.chars() {
        let ch = if ch == '\\' { '/' } else { ch };
        if ch == '/' {
            if !prev_slash {
                out.push('/');
                prev_slash = true;
            }
        } else {
            out.push(ch);
            prev_slash = false;
        }
    }
    while out.ends_with('/') && out.len() > 1 {
        out.pop();
    }
    if out.is_empty() { ".".to_string() } else { out }
}

fn build_footprint_filesystem(
    binaries: &[FootprintBinaryRow],
) -> (Vec<FootprintDirectoryGroup>, bool, Option<u32>) {
    let mut dir_map: BTreeMap<String, BTreeMap<(String, String), (i64, BTreeSet<String>)>> =
        BTreeMap::new();

    for b in binaries {
        let parent = format_footprint_parent_dir(&b.binary_path);
        let label = footprint_job_step_label(&b.job_name, b.step_name.as_deref());

        dir_map
            .entry(parent)
            .or_default()
            .entry((b.binary_path.clone(), b.sha256.clone()))
            .and_modify(|(c, jobs)| {
                *c += b.execution_count;
                if label != "—" {
                    jobs.insert(label.clone());
                }
            })
            .or_insert_with(|| {
                let mut jobs = BTreeSet::new();
                if label != "—" {
                    jobs.insert(label);
                }
                (b.execution_count, jobs)
            });
    }

    let total_dirs = dir_map.len();
    let dirs_truncated = total_dirs > FOOTPRINT_MAX_DIRS;
    let more = dirs_truncated.then(|| (total_dirs - FOOTPRINT_MAX_DIRS) as u32);

    let mut groups: Vec<FootprintDirectoryGroup> = dir_map
        .into_iter()
        .take(FOOTPRINT_MAX_DIRS)
        .map(|(directory, files)| {
            let mut entries: Vec<FootprintDirectoryEntry> = files
                .into_iter()
                .map(
                    |((binary_path, sha256), (execution_count, jobs))| FootprintDirectoryEntry {
                        binary_path,
                        sha256,
                        execution_count,
                        job_names: jobs.into_iter().collect(),
                    },
                )
                .collect();
            entries.sort_by(|a, b| a.binary_path.cmp(&b.binary_path));
            let etrunc = entries.len() > FOOTPRINT_MAX_ENTRIES_PER_DIR;
            if etrunc {
                entries.truncate(FOOTPRINT_MAX_ENTRIES_PER_DIR);
            }
            FootprintDirectoryGroup {
                directory,
                entries,
                entries_truncated: etrunc,
            }
        })
        .collect();
    groups.sort_by(|a, b| a.directory.cmp(&b.directory));
    (groups, dirs_truncated, more)
}

#[utoipa::path(
    get,
    path = "/api/v1/runs/{id}/footprint",
    params(("id" = String, Path, description = "Run ID")),
    responses(
        (status = 200, description = "Execution footprint (binaries, network, directories)", body = RunFootprintResponse),
        (status = 404, description = "Run not found"),
    ),
    tag = "runs",
)]
#[instrument(skip(state))]
async fn get_run_footprint(
    State(state): State<AppState>,
    Auth(_user): Auth,
    Path(id): Path<RunId>,
) -> ApiResult<Json<RunFootprintResponse>> {
    let run = RunRepo::new(state.db()).get(id).await?;
    reconcile_terminal_run_children(state.db(), &run).await?;

    let bin_rows = RunBinaryExecutionRepo::new(state.db())
        .list_footprint_by_run(id)
        .await?;

    let executed_binaries: Vec<FootprintBinaryRow> = bin_rows
        .into_iter()
        .map(|r| FootprintBinaryRow {
            job_name: r.job_name,
            step_name: if r.step_name.is_empty() {
                None
            } else {
                Some(r.step_name)
            },
            binary_path: r.binary_path,
            sha256: r.binary_sha256,
            execution_count: r.execution_count,
        })
        .collect();

    let net_rows = RunNetworkConnectionRepo::new(state.db())
        .list_for_run(id, FOOTPRINT_MAX_NETWORK)
        .await?;

    let network_connections: Vec<FootprintNetworkRow> = net_rows
        .into_iter()
        .map(|n| FootprintNetworkRow {
            job_name: n.job_name,
            dst_ip: n.dst_ip,
            dst_port: n.dst_port,
            protocol: n.protocol,
            direction: n.direction,
            connected_at: n.connected_at.to_rfc3339(),
            binary_path: n.binary_path,
            binary_sha256: n.binary_sha256,
        })
        .collect();

    let (
        filesystem_by_directory,
        filesystem_directories_truncated,
        filesystem_more_directory_count,
    ) = build_footprint_filesystem(&executed_binaries);

    Ok(Json(RunFootprintResponse {
        run_id: id,
        executed_binaries,
        network_connections,
        filesystem_by_directory,
        filesystem_directories_truncated,
        filesystem_more_directory_count,
    }))
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
