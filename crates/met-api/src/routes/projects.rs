//! Project CRUD routes.

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{delete, get, patch, post},
};
use met_core::{
    ids::ProjectId,
    models::{CreateProject, OwnerType, Project, UpdateProject},
};
use met_store::repos::ProjectRepo;
use serde::{Deserialize, Serialize};
use tracing::instrument;
use utoipa::ToSchema;

use crate::{
    error::{ApiError, ApiResult},
    extractors::{Auth, PaginatedResponse, Pagination},
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/projects", get(list_projects).post(create_project))
        .route(
            "/projects/{id}",
            get(get_project)
                .patch(update_project)
                .delete(delete_project),
        )
        .route("/projects/by-slug/{slug}", get(get_project_by_slug))
        .route("/projects/{id}/archive", post(archive_project))
        .route("/projects/{id}/unarchive", post(unarchive_project))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ProjectResponse {
    #[serde(flatten)]
    #[schema(value_type = Object)]
    pub project: Project,
}

#[utoipa::path(
    get,
    path = "/api/v1/projects",
    params(
        ("cursor" = Option<String>, Query, description = "Pagination cursor"),
        ("limit" = Option<u32>, Query, description = "Items per page"),
    ),
    responses(
        (status = 200, description = "List of projects", body = serde_json::Value),
        (status = 401, description = "Unauthorized"),
    ),
    tag = "projects",
)]
#[instrument(skip(state))]
async fn list_projects(
    State(state): State<AppState>,
    Auth(user): Auth,
    pagination: Pagination,
) -> ApiResult<Json<PaginatedResponse<ProjectResponse>>> {
    let repo = ProjectRepo::new(state.db());

    let projects = repo
        .list_by_org(user.org_id, pagination.sql_limit(), 0)
        .await?;

    let response = PaginatedResponse::new(
        projects
            .into_iter()
            .map(|p| ProjectResponse { project: p })
            .collect(),
        pagination.limit,
        |p| p.project.id.to_string(),
    );

    Ok(Json(response))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateProjectRequest {
    pub name: String,
    pub slug: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default = "default_owner_type")]
    #[schema(value_type = String)]
    pub owner_type: OwnerType,
    #[serde(default)]
    pub owner_id: Option<String>,
}

fn default_owner_type() -> OwnerType {
    OwnerType::User
}

#[utoipa::path(
    post,
    path = "/api/v1/projects",
    request_body = CreateProjectRequest,
    responses(
        (status = 200, description = "Project created", body = ProjectResponse),
        (status = 400, description = "Bad request"),
        (status = 401, description = "Unauthorized"),
    ),
    tag = "projects",
)]
#[instrument(skip(state, req))]
async fn create_project(
    State(state): State<AppState>,
    Auth(user): Auth,
    Json(req): Json<CreateProjectRequest>,
) -> ApiResult<Json<ProjectResponse>> {
    let repo = ProjectRepo::new(state.db());

    if req.name.is_empty() {
        return Err(ApiError::bad_request("name is required"));
    }

    if req.slug.is_empty() {
        return Err(ApiError::bad_request("slug is required"));
    }

    if !req
        .slug
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Err(ApiError::bad_request(
            "slug must contain only alphanumeric characters, hyphens, and underscores",
        ));
    }

    let owner_id = req.owner_id.unwrap_or_else(|| user.user_id.to_string());

    let create = CreateProject {
        name: req.name,
        slug: req.slug,
        description: req.description,
        owner_type: req.owner_type,
        owner_id,
    };

    let project = repo.create(user.org_id, &create).await?;

    tracing::info!(
        project_id = %project.id,
        project_slug = %project.slug,
        "project created"
    );

    Ok(Json(ProjectResponse { project }))
}

#[utoipa::path(
    get,
    path = "/api/v1/projects/{id}",
    params(("id" = String, Path, description = "Project ID")),
    responses(
        (status = 200, description = "Project details", body = ProjectResponse),
        (status = 404, description = "Project not found"),
    ),
    tag = "projects",
)]
#[instrument(skip(state))]
async fn get_project(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(id): Path<ProjectId>,
) -> ApiResult<Json<ProjectResponse>> {
    if !user.can_access_project(id) {
        return Err(ApiError::forbidden("no access to this project"));
    }
    let repo = ProjectRepo::new(state.db());
    let project = repo.get(id).await?;
    Ok(Json(ProjectResponse { project }))
}

#[instrument(skip(state))]
async fn get_project_by_slug(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(slug): Path<String>,
) -> ApiResult<Json<ProjectResponse>> {
    let repo = ProjectRepo::new(state.db());
    let project = repo.get_by_slug(user.org_id, &slug).await?;
    Ok(Json(ProjectResponse { project }))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateProjectRequest {
    pub name: Option<String>,
    pub slug: Option<String>,
    pub description: Option<String>,
}

#[utoipa::path(
    patch,
    path = "/api/v1/projects/{id}",
    params(("id" = String, Path, description = "Project ID")),
    request_body = UpdateProjectRequest,
    responses(
        (status = 200, description = "Project updated", body = ProjectResponse),
        (status = 400, description = "Bad request"),
        (status = 404, description = "Project not found"),
    ),
    tag = "projects",
)]
#[instrument(skip(state, req))]
async fn update_project(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(id): Path<ProjectId>,
    Json(req): Json<UpdateProjectRequest>,
) -> ApiResult<Json<ProjectResponse>> {
    if !user.can_access_project(id) {
        return Err(ApiError::forbidden("no access to this project"));
    }

    let repo = ProjectRepo::new(state.db());

    if let Some(ref name) = req.name {
        if name.is_empty() {
            return Err(ApiError::bad_request("name cannot be empty"));
        }
    }

    if let Some(ref slug) = req.slug {
        if slug.is_empty() {
            return Err(ApiError::bad_request("slug cannot be empty"));
        }
        if !slug
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            return Err(ApiError::bad_request(
                "slug must contain only alphanumeric characters, hyphens, and underscores",
            ));
        }
    }

    let update = UpdateProject {
        name: req.name,
        slug: req.slug,
        description: req.description,
    };

    let project = repo.update(id, &update).await?;

    tracing::info!(project_id = %id, "project updated");

    Ok(Json(ProjectResponse { project }))
}

#[utoipa::path(
    delete,
    path = "/api/v1/projects/{id}",
    params(("id" = String, Path, description = "Project ID")),
    responses(
        (status = 200, description = "Project deleted"),
        (status = 404, description = "Project not found"),
    ),
    tag = "projects",
)]
#[instrument(skip(state))]
async fn delete_project(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(id): Path<ProjectId>,
) -> ApiResult<()> {
    if !user.can_access_project(id) {
        return Err(ApiError::forbidden("no access to this project"));
    }
    let repo = ProjectRepo::new(state.db());
    repo.delete(id).await?;

    tracing::info!(project_id = %id, "project soft-deleted");

    Ok(())
}

#[instrument(skip(state))]
async fn archive_project(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(id): Path<ProjectId>,
) -> ApiResult<Json<ProjectResponse>> {
    if !user.can_access_project(id) {
        return Err(ApiError::forbidden("no access to this project"));
    }
    let repo = ProjectRepo::new(state.db());
    let project = repo.archive(id).await?;

    tracing::info!(project_id = %id, "project archived");

    Ok(Json(ProjectResponse { project }))
}

#[instrument(skip(state))]
async fn unarchive_project(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(id): Path<ProjectId>,
) -> ApiResult<Json<ProjectResponse>> {
    if !user.can_access_project(id) {
        return Err(ApiError::forbidden("no access to this project"));
    }
    let repo = ProjectRepo::new(state.db());
    let project = repo.unarchive(id).await?;

    tracing::info!(project_id = %id, "project unarchived");

    Ok(Json(ProjectResponse { project }))
}
