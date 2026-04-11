//! Project CRUD routes.

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, patch, post},
};
use chrono::{DateTime, Utc};
use met_core::{
    ids::ProjectId,
    models::{CreateProject, OwnerType, Project, UpdateProject},
};
use met_store::repos::{MeticulousAppRepo, PipelineRepo, ProjectRepo};
use serde::{Deserialize, Serialize};
use tracing::instrument;
use utoipa::ToSchema;

use crate::{
    error::{ApiError, ApiResult},
    extractors::{Auth, PaginatedResponse, Pagination, SessionOrAppAuth},
    project_access::{
        SessionOrApp, effective_project_role_in_user_org,
        effective_project_role_session_or_app_in_user_org,
    },
    state::AppState,
};
use std::collections::HashSet;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/projects", get(list_projects).post(create_project))
        .route(
            "/projects/{id}",
            get(get_project).patch(update_project),
        )
        .route("/projects/by-slug/{slug}", get(get_project_by_slug))
        .route("/projects/{id}/archive", post(archive_project))
        .route("/projects/{id}/unarchive", post(unarchive_project))
        .route(
            "/projects/{id}/meticulous-apps/installations",
            get(list_project_meticulous_installations).post(install_meticulous_app),
        )
        .route(
            "/projects/{id}/meticulous-apps/available",
            get(list_meticulous_apps_available_for_project),
        )
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
    SessionOrAppAuth(caller): SessionOrAppAuth,
    pagination: Pagination,
) -> ApiResult<Json<PaginatedResponse<ProjectResponse>>> {
    let repo = ProjectRepo::new(state.db());

    let mut projects = match &caller {
        SessionOrApp::App(p) => {
            let project = repo.get(p.project_id).await?;
            vec![project]
        }
        SessionOrApp::User(user) => {
            if user.has_permission("*") {
                repo
                    .list_by_org(user.org_id, pagination.sql_limit(), 0)
                    .await?
            } else {
                repo
                    .list_by_org_for_user(user.org_id, user.user_id, pagination.sql_limit(), 0)
                    .await?
            }
        }
    };

    if let SessionOrApp::User(user) = &caller {
        if user.is_api_token {
            if let Some(allowed) = &user.project_ids {
                let set: HashSet<_> = allowed.iter().copied().collect();
                projects.retain(|p| set.contains(&p.id));
            }
        }
    }

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
        visibility: Default::default(),
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
    SessionOrAppAuth(caller): SessionOrAppAuth,
    Path(id): Path<ProjectId>,
) -> ApiResult<Json<ProjectResponse>> {
    let repo = ProjectRepo::new(state.db());
    let project = repo.get(id).await?;
    match &caller {
        SessionOrApp::User(u) if project.org_id != u.org_id => {
            return Err(ApiError::not_found("project not found"));
        }
        SessionOrApp::App(p) if project.org_id != p.org_id => {
            return Err(ApiError::not_found("project not found"));
        }
        _ => {}
    }
    effective_project_role_session_or_app_in_user_org(state.db(), &caller, id).await?;
    Ok(Json(ProjectResponse { project }))
}

#[instrument(skip(state))]
async fn get_project_by_slug(
    State(state): State<AppState>,
    SessionOrAppAuth(caller): SessionOrAppAuth,
    Path(slug): Path<String>,
) -> ApiResult<Json<ProjectResponse>> {
    let repo = ProjectRepo::new(state.db());
    let project = match &caller {
        SessionOrApp::User(u) => repo.get_by_slug(u.org_id, &slug).await?,
        SessionOrApp::App(p) => repo.get_by_slug(p.org_id, &slug).await?,
    };
    effective_project_role_session_or_app_in_user_org(state.db(), &caller, project.id).await?;
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
    let role = effective_project_role_in_user_org(state.db(), &user, id).await?;
    if !role.can_manage_pipelines() {
        return Err(ApiError::forbidden(
            "project administrator role is required to update this project",
        ));
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
        visibility: None,
    };

    let project = repo.update(id, &update).await?;

    tracing::info!(project_id = %id, "project updated");

    Ok(Json(ProjectResponse { project }))
}

#[instrument(skip(state))]
async fn archive_project(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(id): Path<ProjectId>,
) -> ApiResult<Json<ProjectResponse>> {
    let role = effective_project_role_in_user_org(state.db(), &user, id).await?;
    if !role.can_manage_pipelines() {
        return Err(ApiError::forbidden(
            "project administrator role is required to archive this project",
        ));
    }
    let repo = ProjectRepo::new(state.db());
    let project = repo.archive(id).await?;
    PipelineRepo::new(state.db())
        .archive_all_in_project(id)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;

    tracing::info!(project_id = %id, "project archived");

    Ok(Json(ProjectResponse { project }))
}

#[instrument(skip(state))]
async fn unarchive_project(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(id): Path<ProjectId>,
) -> ApiResult<Json<ProjectResponse>> {
    if !user.has_permission("*") {
        return Err(ApiError::forbidden(
            "only organization administrators may unarchive projects (Admin → Archive)",
        ));
    }
    let repo = ProjectRepo::new(state.db());
    let project_row = repo.get(id).await?;
    if project_row.org_id != user.org_id {
        return Err(ApiError::not_found("project not found"));
    }
    let project = repo.unarchive(id).await?;
    PipelineRepo::new(state.db())
        .unarchive_all_in_project(id)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;

    tracing::info!(project_id = %id, "project unarchived");

    Ok(Json(ProjectResponse { project }))
}

#[derive(Debug, Deserialize, Serialize)]
pub struct InstallMeticulousAppRequest {
    /// Public application id string.
    pub application_id: String,
    #[serde(default)]
    pub permissions: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct InstallMeticulousAppResponse {
    pub installation_id: String,
    pub application_id: String,
    pub project_id: String,
    pub permissions: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct MeticulousAppCatalogEntry {
    pub application_id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ProjectMeticulousInstallationResponse {
    pub installation_id: String,
    pub application_id: String,
    pub app_name: String,
    pub permissions: Vec<String>,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revoked_at: Option<DateTime<Utc>>,
}

#[instrument(skip(state))]
async fn list_meticulous_apps_available_for_project(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(project_id): Path<ProjectId>,
) -> ApiResult<Json<Vec<MeticulousAppCatalogEntry>>> {
    let proj = ProjectRepo::new(state.db()).get(project_id).await?;
    if proj.org_id != user.org_id {
        return Err(ApiError::forbidden("project not in your organization"));
    }
    let role = effective_project_role_in_user_org(state.db(), &user, project_id).await?;
    if !role.can_manage_pipelines() {
        return Err(ApiError::forbidden(
            "project administrator role is required to browse Meticulous Apps for install",
        ));
    }
    let apps = MeticulousAppRepo::new(state.db())
        .list_enabled_catalog_for_org(user.org_id)
        .await?;
    Ok(Json(
        apps
            .into_iter()
            .map(|a| MeticulousAppCatalogEntry {
                application_id: a.application_id,
                name: a.name,
                description: a.description,
            })
            .collect(),
    ))
}

#[instrument(skip(state))]
async fn list_project_meticulous_installations(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(project_id): Path<ProjectId>,
) -> ApiResult<Json<Vec<ProjectMeticulousInstallationResponse>>> {
    let proj = ProjectRepo::new(state.db()).get(project_id).await?;
    if proj.org_id != user.org_id {
        return Err(ApiError::forbidden("project not in your organization"));
    }
    effective_project_role_in_user_org(state.db(), &user, project_id).await?;
    let rows = MeticulousAppRepo::new(state.db())
        .list_installation_summaries_for_project(project_id)
        .await?;
    Ok(Json(
        rows
            .into_iter()
            .map(|r| ProjectMeticulousInstallationResponse {
                installation_id: r.id.to_string(),
                application_id: r.application_id,
                app_name: r.name,
                permissions: r.permissions,
                created_at: r.created_at,
                revoked_at: r.revoked_at,
            })
            .collect(),
    ))
}

#[instrument(skip(state, req))]
async fn install_meticulous_app(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(project_id): Path<ProjectId>,
    Json(req): Json<InstallMeticulousAppRequest>,
) -> ApiResult<Json<InstallMeticulousAppResponse>> {
    let proj = ProjectRepo::new(state.db()).get(project_id).await?;
    if proj.org_id != user.org_id {
        return Err(ApiError::forbidden("project not in your organization"));
    }
    let role = effective_project_role_in_user_org(state.db(), &user, project_id).await?;
    if !role.can_manage_pipelines() {
        return Err(ApiError::forbidden(
            "project administrator role is required to install Meticulous Apps",
        ));
    }

    let app_repo = MeticulousAppRepo::new(state.db());
    let app = app_repo
        .get_by_application_id(req.application_id.trim())
        .await?;
    if !app.enabled {
        return Err(ApiError::bad_request(
            "this Meticulous App is disabled by an administrator",
        ));
    }

    let perms = if req.permissions.is_empty() {
        vec!["read".to_string()]
    } else {
        req.permissions
    };

    let inst = app_repo
        .create_installation(app.id, project_id, &perms)
        .await?;

    Ok(Json(InstallMeticulousAppResponse {
        installation_id: inst.id.to_string(),
        application_id: app.application_id.clone(),
        project_id: project_id.to_string(),
        permissions: inst.permissions.clone(),
    }))
}
