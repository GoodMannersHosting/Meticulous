//! Pipeline environment CRUD and approval routes (ADR-016, Phase 2.1).

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use met_core::ids::ProjectId;
use met_store::repos::{EnvironmentRepo, EnvironmentRow, ProjectRepo};
use serde::{Deserialize, Serialize};
use tracing::instrument;
use uuid::Uuid;

use crate::{
    error::{ApiError, ApiResult},
    extractors::Auth,
    project_access::effective_project_role_in_user_org,
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/projects/{project_id}/environments",
            get(list_environments).post(create_environment),
        )
        .route(
            "/projects/{project_id}/environments/{env_id}",
            get(get_environment).patch(update_environment).delete(delete_environment),
        )
        .route(
            "/runs/{run_id}/environments/{env_name}/approve",
            post(approve_deployment),
        )
        .route(
            "/runs/{run_id}/environments/{env_name}/reject",
            post(reject_deployment),
        )
}

#[derive(Debug, Serialize)]
struct EnvironmentResponse {
    id: String,
    project_id: String,
    name: String,
    display_name: String,
    description: Option<String>,
    require_approval: bool,
    required_approvers: i32,
    approval_timeout_hours: i32,
    allowed_branches: Option<Vec<String>>,
    auto_deploy_branch: Option<String>,
    variables: serde_json::Value,
    tier: String,
    created_at: String,
    updated_at: String,
}

impl From<EnvironmentRow> for EnvironmentResponse {
    fn from(r: EnvironmentRow) -> Self {
        Self {
            id: r.id.to_string(),
            project_id: r.project_id.to_string(),
            name: r.name,
            display_name: r.display_name,
            description: r.description,
            require_approval: r.require_approval,
            required_approvers: r.required_approvers,
            approval_timeout_hours: r.approval_timeout_hours,
            allowed_branches: r.allowed_branches,
            auto_deploy_branch: r.auto_deploy_branch,
            variables: r.variables,
            tier: r.tier,
            created_at: r.created_at.to_rfc3339(),
            updated_at: r.updated_at.to_rfc3339(),
        }
    }
}

#[instrument(skip(state))]
async fn list_environments(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(project_id): Path<ProjectId>,
) -> ApiResult<Json<Vec<EnvironmentResponse>>> {
    let _role = effective_project_role_in_user_org(state.db(), &user, project_id).await?;
    let rows = EnvironmentRepo::new(state.db())
        .list_by_project(project_id)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    Ok(Json(rows.into_iter().map(EnvironmentResponse::from).collect()))
}

#[instrument(skip(state))]
async fn get_environment(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path((project_id, env_id)): Path<(ProjectId, Uuid)>,
) -> ApiResult<Json<EnvironmentResponse>> {
    let _role = effective_project_role_in_user_org(state.db(), &user, project_id).await?;
    let row = EnvironmentRepo::new(state.db())
        .get(env_id)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    Ok(Json(EnvironmentResponse::from(row)))
}

#[derive(Debug, Deserialize)]
struct CreateEnvironmentRequest {
    name: String,
    display_name: String,
    description: Option<String>,
    #[serde(default = "default_tier")]
    tier: String,
}

fn default_tier() -> String {
    "development".to_string()
}

#[instrument(skip(state, req))]
async fn create_environment(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(project_id): Path<ProjectId>,
    Json(req): Json<CreateEnvironmentRequest>,
) -> ApiResult<Json<EnvironmentResponse>> {
    let role = effective_project_role_in_user_org(state.db(), &user, project_id).await?;
    if !role.can_manage_pipelines() {
        return Err(ApiError::forbidden("requires project admin"));
    }
    let project = ProjectRepo::new(state.db()).get(project_id).await?;
    let row = EnvironmentRepo::new(state.db())
        .create(
            project.org_id.into(),
            project_id,
            &req.name,
            &req.display_name,
            req.description.as_deref(),
            &req.tier,
        )
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    Ok(Json(EnvironmentResponse::from(row)))
}

#[derive(Debug, Deserialize)]
struct UpdateEnvironmentRequest {
    display_name: Option<String>,
    description: Option<String>,
    tier: Option<String>,
    require_approval: Option<bool>,
    required_approvers: Option<i32>,
    approval_timeout_hours: Option<i32>,
    allowed_branches: Option<Vec<String>>,
    auto_deploy_branch: Option<String>,
    variables: Option<serde_json::Value>,
}

#[instrument(skip(state, req))]
async fn update_environment(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path((project_id, env_id)): Path<(ProjectId, Uuid)>,
    Json(req): Json<UpdateEnvironmentRequest>,
) -> ApiResult<Json<EnvironmentResponse>> {
    let role = effective_project_role_in_user_org(state.db(), &user, project_id).await?;
    if !role.can_manage_pipelines() {
        return Err(ApiError::forbidden("requires project admin"));
    }
    let row = EnvironmentRepo::new(state.db())
        .update(
            env_id,
            req.display_name.as_deref(),
            req.description.as_deref(),
            req.tier.as_deref(),
            req.require_approval,
            req.required_approvers,
            req.approval_timeout_hours,
            req.allowed_branches.as_deref(),
            req.auto_deploy_branch.as_deref(),
            req.variables.as_ref(),
        )
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    Ok(Json(EnvironmentResponse::from(row)))
}

#[instrument(skip(state))]
async fn delete_environment(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path((project_id, env_id)): Path<(ProjectId, Uuid)>,
) -> ApiResult<Json<serde_json::Value>> {
    let role = effective_project_role_in_user_org(state.db(), &user, project_id).await?;
    if !role.can_manage_pipelines() {
        return Err(ApiError::forbidden("requires project admin"));
    }
    EnvironmentRepo::new(state.db())
        .delete(env_id)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    Ok(Json(serde_json::json!({ "message": "environment deleted" })))
}

#[derive(Debug, Deserialize)]
struct ApprovalRequest {
    comment: Option<String>,
}

#[instrument(skip(state, req))]
async fn approve_deployment(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path((run_id, env_name)): Path<(Uuid, String)>,
    Json(req): Json<ApprovalRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let repo = EnvironmentRepo::new(state.db());
    let approval = repo
        .record_approval(run_id, Uuid::nil(), user.user_id, "approved", req.comment.as_deref())
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    Ok(Json(serde_json::json!({
        "message": "deployment approved",
        "decision": approval.decision,
        "decided_at": approval.decided_at.to_rfc3339(),
    })))
}

#[instrument(skip(state, req))]
async fn reject_deployment(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path((run_id, env_name)): Path<(Uuid, String)>,
    Json(req): Json<ApprovalRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let repo = EnvironmentRepo::new(state.db());
    let approval = repo
        .record_approval(run_id, Uuid::nil(), user.user_id, "rejected", req.comment.as_deref())
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    Ok(Json(serde_json::json!({
        "message": "deployment rejected",
        "decision": approval.decision,
        "decided_at": approval.decided_at.to_rfc3339(),
    })))
}
