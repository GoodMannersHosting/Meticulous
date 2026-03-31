//! Pipeline CRUD routes.

use axum::{
    extract::{Path, State},
    routing::{delete, get, post, put},
    Json, Router,
};
use met_core::{
    ids::{PipelineId, ProjectId},
    models::{CreatePipeline, Pipeline, UpdatePipeline},
};
use met_store::repos::PipelineRepo;
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
        .route("/pipelines", get(list_pipelines).post(create_pipeline))
        .route(
            "/pipelines/{id}",
            get(get_pipeline).put(update_pipeline).delete(delete_pipeline),
        )
        .route("/pipelines/by-slug/{project_id}/{slug}", get(get_pipeline_by_slug))
        .route("/pipelines/{id}/trigger", post(trigger_pipeline))
        .route("/pipelines/{id}/validate", post(validate_pipeline))
}

#[derive(Debug, Deserialize)]
pub struct ListPipelinesQuery {
    project_id: Option<ProjectId>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PipelineResponse {
    #[serde(flatten)]
    #[schema(value_type = Object)]
    pub pipeline: Pipeline,
}

#[utoipa::path(
    get,
    path = "/api/v1/pipelines",
    params(
        ("project_id" = Option<String>, Query, description = "Filter by project ID"),
        ("cursor" = Option<String>, Query, description = "Pagination cursor"),
        ("limit" = Option<u32>, Query, description = "Items per page"),
    ),
    responses(
        (status = 200, description = "List of pipelines", body = serde_json::Value),
        (status = 401, description = "Unauthorized"),
    ),
    tag = "pipelines",
)]
#[instrument(skip(state))]
async fn list_pipelines(
    State(state): State<AppState>,
    Auth(user): Auth,
    pagination: Pagination,
    axum::extract::Query(query): axum::extract::Query<ListPipelinesQuery>,
) -> ApiResult<Json<PaginatedResponse<PipelineResponse>>> {
    let repo = PipelineRepo::new(state.db());

    let project_id = query.project_id.ok_or_else(|| {
        ApiError::bad_request("project_id query parameter is required")
    })?;

    let pipelines = repo
        .list_by_project(project_id, pagination.sql_limit(), 0)
        .await?;

    let response = PaginatedResponse::new(
        pipelines.into_iter().map(|p| PipelineResponse { pipeline: p }).collect(),
        pagination.limit,
        |p| p.pipeline.id.to_string(),
    );

    Ok(Json(response))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreatePipelineRequest {
    #[schema(value_type = String)]
    pub project_id: ProjectId,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub definition: serde_json::Value,
    pub definition_path: Option<String>,
}

#[utoipa::path(
    post,
    path = "/api/v1/pipelines",
    request_body = CreatePipelineRequest,
    responses(
        (status = 200, description = "Pipeline created", body = PipelineResponse),
        (status = 400, description = "Bad request"),
        (status = 401, description = "Unauthorized"),
    ),
    tag = "pipelines",
)]
#[instrument(skip(state, req))]
async fn create_pipeline(
    State(state): State<AppState>,
    Auth(user): Auth,
    Json(req): Json<CreatePipelineRequest>,
) -> ApiResult<Json<PipelineResponse>> {
    let repo = PipelineRepo::new(state.db());

    let create = CreatePipeline {
        name: req.name,
        slug: req.slug,
        description: req.description,
        definition: req.definition,
        definition_path: req.definition_path,
    };

    let pipeline = repo.create(req.project_id, &create).await?;

    Ok(Json(PipelineResponse { pipeline }))
}

#[utoipa::path(
    get,
    path = "/api/v1/pipelines/{id}",
    params(("id" = String, Path, description = "Pipeline ID")),
    responses(
        (status = 200, description = "Pipeline details", body = PipelineResponse),
        (status = 404, description = "Pipeline not found"),
    ),
    tag = "pipelines",
)]
#[instrument(skip(state))]
async fn get_pipeline(
    State(state): State<AppState>,
    Auth(_user): Auth,
    Path(id): Path<PipelineId>,
) -> ApiResult<Json<PipelineResponse>> {
    let repo = PipelineRepo::new(state.db());
    let pipeline = repo.get(id).await?;
    Ok(Json(PipelineResponse { pipeline }))
}

#[instrument(skip(state))]
async fn get_pipeline_by_slug(
    State(state): State<AppState>,
    Auth(_user): Auth,
    Path((project_id, slug)): Path<(ProjectId, String)>,
) -> ApiResult<Json<PipelineResponse>> {
    let repo = PipelineRepo::new(state.db());
    let pipeline = repo.get_by_slug(project_id, &slug).await?;
    Ok(Json(PipelineResponse { pipeline }))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdatePipelineRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub definition: Option<serde_json::Value>,
    pub enabled: Option<bool>,
}

#[utoipa::path(
    put,
    path = "/api/v1/pipelines/{id}",
    params(("id" = String, Path, description = "Pipeline ID")),
    request_body = UpdatePipelineRequest,
    responses(
        (status = 200, description = "Pipeline updated", body = PipelineResponse),
        (status = 400, description = "Bad request"),
        (status = 404, description = "Pipeline not found"),
    ),
    tag = "pipelines",
)]
#[instrument(skip(state, req))]
async fn update_pipeline(
    State(state): State<AppState>,
    Auth(_user): Auth,
    Path(id): Path<PipelineId>,
    Json(req): Json<UpdatePipelineRequest>,
) -> ApiResult<Json<PipelineResponse>> {
    let repo = PipelineRepo::new(state.db());

    let update = UpdatePipeline {
        name: req.name,
        description: req.description,
        definition: req.definition,
        enabled: req.enabled,
    };

    let pipeline = repo.update(id, &update).await?;

    Ok(Json(PipelineResponse { pipeline }))
}

#[utoipa::path(
    delete,
    path = "/api/v1/pipelines/{id}",
    params(("id" = String, Path, description = "Pipeline ID")),
    responses(
        (status = 200, description = "Pipeline deleted"),
        (status = 404, description = "Pipeline not found"),
    ),
    tag = "pipelines",
)]
#[instrument(skip(state))]
async fn delete_pipeline(
    State(state): State<AppState>,
    Auth(_user): Auth,
    Path(id): Path<PipelineId>,
) -> ApiResult<()> {
    let repo = PipelineRepo::new(state.db());
    repo.delete(id).await?;
    Ok(())
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct TriggerPipelineRequest {
    pub branch: Option<String>,
    pub commit_sha: Option<String>,
    pub variables: Option<std::collections::HashMap<String, String>>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TriggerPipelineResponse {
    #[schema(value_type = String)]
    pub run_id: met_core::ids::RunId,
    pub run_number: i64,
    pub status: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/pipelines/{id}/trigger",
    params(("id" = String, Path, description = "Pipeline ID")),
    request_body = TriggerPipelineRequest,
    responses(
        (status = 200, description = "Pipeline triggered", body = TriggerPipelineResponse),
        (status = 404, description = "Pipeline not found"),
    ),
    tag = "pipelines",
)]
#[instrument(skip(state))]
async fn trigger_pipeline(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(id): Path<PipelineId>,
    Json(_req): Json<TriggerPipelineRequest>,
) -> ApiResult<Json<TriggerPipelineResponse>> {
    use met_store::repos::RunRepo;

    let pipeline_repo = PipelineRepo::new(state.db());
    let _pipeline = pipeline_repo.get(id).await?;

    let run_repo = RunRepo::new(state.db());
    let run = run_repo.create(id, None, &user.email).await?;

    Ok(Json(TriggerPipelineResponse {
        run_id: run.id,
        run_number: run.run_number,
        status: format!("{:?}", run.status).to_lowercase(),
    }))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ValidatePipelineRequest {
    pub definition: serde_json::Value,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ValidatePipelineResponse {
    pub valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

#[utoipa::path(
    post,
    path = "/api/v1/pipelines/{id}/validate",
    params(("id" = String, Path, description = "Pipeline ID")),
    request_body = ValidatePipelineRequest,
    responses(
        (status = 200, description = "Validation result", body = ValidatePipelineResponse),
        (status = 404, description = "Pipeline not found"),
    ),
    tag = "pipelines",
)]
#[instrument(skip(state, req))]
async fn validate_pipeline(
    State(state): State<AppState>,
    Auth(_user): Auth,
    Path(id): Path<PipelineId>,
    Json(req): Json<ValidatePipelineRequest>,
) -> ApiResult<Json<ValidatePipelineResponse>> {
    let _pipeline = PipelineRepo::new(state.db()).get(id).await?;

    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    if req.definition.is_null() {
        errors.push("definition cannot be null".to_string());
    }

    if let Some(obj) = req.definition.as_object() {
        if !obj.contains_key("jobs") && !obj.contains_key("steps") {
            warnings.push("definition should contain 'jobs' or 'steps'".to_string());
        }
    } else {
        errors.push("definition must be a JSON object".to_string());
    }

    Ok(Json(ValidatePipelineResponse {
        valid: errors.is_empty(),
        errors,
        warnings,
    }))
}
