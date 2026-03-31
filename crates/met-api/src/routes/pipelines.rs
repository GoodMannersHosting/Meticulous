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
}

#[derive(Debug, Deserialize)]
pub struct ListPipelinesQuery {
    project_id: Option<ProjectId>,
}

#[derive(Debug, Serialize)]
pub struct PipelineResponse {
    #[serde(flatten)]
    pub pipeline: Pipeline,
}

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

#[derive(Debug, Deserialize)]
pub struct CreatePipelineRequest {
    pub project_id: ProjectId,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub definition: serde_json::Value,
    pub definition_path: Option<String>,
}

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

#[derive(Debug, Deserialize)]
pub struct UpdatePipelineRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub definition: Option<serde_json::Value>,
    pub enabled: Option<bool>,
}

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

#[derive(Debug, Deserialize)]
pub struct TriggerPipelineRequest {
    pub branch: Option<String>,
    pub commit_sha: Option<String>,
    pub variables: Option<std::collections::HashMap<String, String>>,
}

#[derive(Debug, Serialize)]
pub struct TriggerPipelineResponse {
    pub run_id: met_core::ids::RunId,
    pub run_number: i64,
    pub status: String,
}

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
