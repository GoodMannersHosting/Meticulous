//! Member management routes for projects and pipelines (ADR-021).
//!
//! These routes allow project/pipeline admins to manage membership
//! without requiring platform-admin privileges.

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{delete, get},
};
use met_core::ids::{PipelineId, ProjectId};
use met_store::repos::{
    PipelineAccessRepo, PipelineMemberRow, ProjectAccessRepo, ProjectMemberRow, ProjectRepo,
    PipelineRepo,
};
use serde::{Deserialize, Serialize};
use tracing::instrument;
use uuid::Uuid;

#[derive(Debug, Serialize)]
struct MemberResponse {
    id: String,
    principal_type: String,
    principal_id: String,
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    inherited: Option<bool>,
    display_name: Option<String>,
    created_at: String,
}

impl From<ProjectMemberRow> for MemberResponse {
    fn from(row: ProjectMemberRow) -> Self {
        Self {
            id: row.id.to_string(),
            principal_type: row.principal_type,
            principal_id: row.principal_id.to_string(),
            role: row.role,
            inherited: None,
            display_name: row.display_name,
            created_at: row.created_at.to_rfc3339(),
        }
    }
}

impl From<PipelineMemberRow> for MemberResponse {
    fn from(row: PipelineMemberRow) -> Self {
        Self {
            id: row.id.to_string(),
            principal_type: row.principal_type,
            principal_id: row.principal_id.to_string(),
            role: row.role,
            inherited: Some(row.inherited),
            display_name: row.display_name,
            created_at: row.created_at.to_rfc3339(),
        }
    }
}

use crate::{
    error::{ApiError, ApiResult},
    extractors::Auth,
    project_access::{effective_pipeline_role, effective_project_role_in_user_org},
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/projects/{project_id}/members",
            get(list_project_members).post(add_project_member),
        )
        .route(
            "/projects/{project_id}/members/{principal_id}",
            delete(remove_project_member),
        )
        .route(
            "/pipelines/{pipeline_id}/members",
            get(list_pipeline_members).post(add_pipeline_member),
        )
        .route(
            "/pipelines/{pipeline_id}/members/{principal_id}",
            delete(remove_pipeline_member),
        )
}

// ---- Project members ----

#[instrument(skip(state))]
async fn list_project_members(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(project_id): Path<ProjectId>,
) -> ApiResult<Json<Vec<MemberResponse>>> {
    let role = effective_project_role_in_user_org(state.db(), &user, project_id).await?;
    if !role.can_manage_pipelines() {
        return Err(ApiError::forbidden("requires project admin"));
    }
    let members = ProjectAccessRepo::new(state.db())
        .list_members(project_id)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    Ok(Json(members.into_iter().map(MemberResponse::from).collect()))
}

#[derive(Debug, Deserialize)]
struct AddMemberRequest {
    principal_type: String,
    principal_id: Uuid,
    role: String,
}

#[instrument(skip(state, req))]
async fn add_project_member(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(project_id): Path<ProjectId>,
    Json(req): Json<AddMemberRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let role = effective_project_role_in_user_org(state.db(), &user, project_id).await?;
    if !role.can_manage_pipelines() {
        return Err(ApiError::forbidden("requires project admin"));
    }

    if !matches!(req.principal_type.as_str(), "user" | "group") {
        return Err(ApiError::bad_request("principal_type must be 'user' or 'group'"));
    }
    if !matches!(req.role.as_str(), "admin" | "developer" | "readonly") {
        return Err(ApiError::bad_request(
            "role must be 'admin', 'developer', or 'readonly'",
        ));
    }

    ProjectAccessRepo::new(state.db())
        .add_member(project_id, &req.principal_type, req.principal_id, &req.role)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;

    Ok(Json(serde_json::json!({ "message": "member saved" })))
}

#[instrument(skip(state))]
async fn remove_project_member(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path((project_id, principal_id)): Path<(ProjectId, Uuid)>,
) -> ApiResult<Json<serde_json::Value>> {
    let role = effective_project_role_in_user_org(state.db(), &user, project_id).await?;
    if !role.can_manage_pipelines() {
        return Err(ApiError::forbidden("requires project admin"));
    }

    let project = ProjectRepo::new(state.db()).get(project_id).await?;
    let owner_uuid: Uuid = project
        .owner_id
        .parse()
        .map_err(|_| ApiError::internal("invalid owner_id"))?;
    if principal_id == owner_uuid {
        return Err(ApiError::bad_request(
            "cannot remove the project owner from membership",
        ));
    }

    ProjectAccessRepo::new(state.db())
        .remove_member(project_id, principal_id)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;

    Ok(Json(serde_json::json!({ "message": "member removed" })))
}

// ---- Pipeline members ----

#[instrument(skip(state))]
async fn list_pipeline_members(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(pipeline_id): Path<PipelineId>,
) -> ApiResult<Json<Vec<MemberResponse>>> {
    let pipeline = PipelineRepo::new(state.db()).get(pipeline_id).await?;
    let role =
        effective_pipeline_role(state.db(), &user, pipeline_id, pipeline.project_id).await?;
    if !role.can_manage_members() {
        return Err(ApiError::forbidden("requires pipeline admin"));
    }

    let members = PipelineAccessRepo::new(state.db())
        .list_members(pipeline_id)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    Ok(Json(members.into_iter().map(MemberResponse::from).collect()))
}

#[instrument(skip(state, req))]
async fn add_pipeline_member(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(pipeline_id): Path<PipelineId>,
    Json(req): Json<AddMemberRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let pipeline = PipelineRepo::new(state.db()).get(pipeline_id).await?;
    let role =
        effective_pipeline_role(state.db(), &user, pipeline_id, pipeline.project_id).await?;
    if !role.can_manage_members() {
        return Err(ApiError::forbidden("requires pipeline admin"));
    }

    if !matches!(req.principal_type.as_str(), "user" | "group") {
        return Err(ApiError::bad_request("principal_type must be 'user' or 'group'"));
    }
    if !matches!(req.role.as_str(), "admin" | "developer" | "readonly") {
        return Err(ApiError::bad_request(
            "role must be 'admin', 'developer', or 'readonly'",
        ));
    }

    PipelineAccessRepo::new(state.db())
        .add_member(pipeline_id, &req.principal_type, req.principal_id, &req.role)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;

    Ok(Json(serde_json::json!({ "message": "member saved" })))
}

#[instrument(skip(state))]
async fn remove_pipeline_member(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path((pipeline_id, principal_id)): Path<(PipelineId, Uuid)>,
) -> ApiResult<Json<serde_json::Value>> {
    let pipeline = PipelineRepo::new(state.db()).get(pipeline_id).await?;
    let role =
        effective_pipeline_role(state.db(), &user, pipeline_id, pipeline.project_id).await?;
    if !role.can_manage_members() {
        return Err(ApiError::forbidden("requires pipeline admin"));
    }

    PipelineAccessRepo::new(state.db())
        .remove_member(pipeline_id, principal_id)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;

    Ok(Json(serde_json::json!({ "message": "member removed" })))
}
