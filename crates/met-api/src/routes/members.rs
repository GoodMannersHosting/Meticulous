//! Member management routes for projects and pipelines (ADR-021).

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{delete, get, patch},
};
use met_core::ids::{PipelineId, ProjectId};
use met_store::repos::{
    PipelineAccessRepo, PipelineMemberRow, PipelineRepo, ProjectAccessRepo, ProjectMemberRow,
    ProjectRepo,
};
use serde::{Deserialize, Serialize};
use tracing::instrument;
use uuid::Uuid;

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
            patch(update_project_member_role).delete(remove_project_member),
        )
        .route(
            "/pipelines/{pipeline_id}/members",
            get(list_pipeline_members).post(add_pipeline_member),
        )
        .route(
            "/pipelines/{pipeline_id}/members/{principal_id}",
            patch(update_pipeline_member_role).delete(remove_pipeline_member),
        )
        .route("/principals/search", get(search_principals))
}

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
            role: normalize_role_for_api(&row.role),
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
            role: normalize_role_for_api(&row.role),
            inherited: Some(row.inherited),
            display_name: row.display_name,
            created_at: row.created_at.to_rfc3339(),
        }
    }
}

/// DB stores `developer`; API exposes `operator` as the user-facing label.
fn normalize_role_for_api(db_role: &str) -> String {
    match db_role {
        "developer" => "operator".to_string(),
        other => other.to_string(),
    }
}

/// Accept `operator` (canonical), `developer`/`executor` (legacy), and normalize to DB enum.
fn normalize_role_for_db(api_role: &str) -> Result<&'static str, ApiError> {
    match api_role {
        "admin" => Ok("admin"),
        "operator" | "developer" | "executor" => Ok("developer"),
        "readonly" | "read-only" => Ok("readonly"),
        _ => Err(ApiError::bad_request(
            "role must be 'admin', 'operator', or 'readonly'",
        )),
    }
}

fn validate_principal_type(pt: &str) -> Result<(), ApiError> {
    if !matches!(pt, "user" | "group") {
        return Err(ApiError::bad_request(
            "principal_type must be 'user' or 'group'",
        ));
    }
    Ok(())
}

// ---- Project members ----

#[instrument(skip(state))]
async fn list_project_members(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(project_id): Path<ProjectId>,
) -> ApiResult<Json<Vec<MemberResponse>>> {
    let _role = effective_project_role_in_user_org(state.db(), &user, project_id).await?;
    let members = ProjectAccessRepo::new(state.db())
        .list_members(project_id)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    Ok(Json(
        members.into_iter().map(MemberResponse::from).collect(),
    ))
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
    validate_principal_type(&req.principal_type)?;
    let db_role = normalize_role_for_db(&req.role)?;

    ProjectAccessRepo::new(state.db())
        .add_member(project_id, &req.principal_type, req.principal_id, db_role)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;

    Ok(Json(serde_json::json!({ "message": "member saved" })))
}

#[derive(Debug, Deserialize)]
struct UpdateRoleRequest {
    role: String,
}

#[instrument(skip(state, req))]
async fn update_project_member_role(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path((project_id, principal_id)): Path<(ProjectId, Uuid)>,
    Json(req): Json<UpdateRoleRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let role = effective_project_role_in_user_org(state.db(), &user, project_id).await?;
    if !role.can_manage_pipelines() {
        return Err(ApiError::forbidden("requires project admin"));
    }
    let db_role = normalize_role_for_db(&req.role)?;

    let repo = ProjectAccessRepo::new(state.db());
    let members = repo
        .list_members(project_id)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    let member = members
        .iter()
        .find(|m| m.principal_id == principal_id)
        .ok_or_else(|| ApiError::not_found("member not found"))?;

    repo.add_member(project_id, &member.principal_type, principal_id, db_role)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;

    Ok(Json(serde_json::json!({ "message": "role updated" })))
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
    let _role =
        effective_pipeline_role(state.db(), &user, pipeline_id, pipeline.project_id).await?;
    let members = PipelineAccessRepo::new(state.db())
        .list_members(pipeline_id)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    Ok(Json(
        members.into_iter().map(MemberResponse::from).collect(),
    ))
}

#[instrument(skip(state, req))]
async fn add_pipeline_member(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(pipeline_id): Path<PipelineId>,
    Json(req): Json<AddMemberRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let pipeline = PipelineRepo::new(state.db()).get(pipeline_id).await?;
    let role = effective_pipeline_role(state.db(), &user, pipeline_id, pipeline.project_id).await?;
    if !role.can_manage_members() {
        return Err(ApiError::forbidden("requires pipeline admin"));
    }
    validate_principal_type(&req.principal_type)?;
    let db_role = normalize_role_for_db(&req.role)?;

    PipelineAccessRepo::new(state.db())
        .add_member(pipeline_id, &req.principal_type, req.principal_id, db_role)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;

    Ok(Json(serde_json::json!({ "message": "member saved" })))
}

#[instrument(skip(state, req))]
async fn update_pipeline_member_role(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path((pipeline_id, principal_id)): Path<(PipelineId, Uuid)>,
    Json(req): Json<UpdateRoleRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let pipeline = PipelineRepo::new(state.db()).get(pipeline_id).await?;
    let role = effective_pipeline_role(state.db(), &user, pipeline_id, pipeline.project_id).await?;
    if !role.can_manage_members() {
        return Err(ApiError::forbidden("requires pipeline admin"));
    }
    let db_role = normalize_role_for_db(&req.role)?;

    let repo = PipelineAccessRepo::new(state.db());
    let members = repo
        .list_members(pipeline_id)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    let member = members
        .iter()
        .find(|m| m.principal_id == principal_id)
        .ok_or_else(|| ApiError::not_found("member not found"))?;

    if member.inherited {
        return Err(ApiError::bad_request(
            "cannot change role of inherited member; manage at project level",
        ));
    }

    repo.add_member(pipeline_id, &member.principal_type, principal_id, db_role)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;

    Ok(Json(serde_json::json!({ "message": "role updated" })))
}

#[instrument(skip(state))]
async fn remove_pipeline_member(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path((pipeline_id, principal_id)): Path<(PipelineId, Uuid)>,
) -> ApiResult<Json<serde_json::Value>> {
    let pipeline = PipelineRepo::new(state.db()).get(pipeline_id).await?;
    let role = effective_pipeline_role(state.db(), &user, pipeline_id, pipeline.project_id).await?;
    if !role.can_manage_members() {
        return Err(ApiError::forbidden("requires pipeline admin"));
    }
    PipelineAccessRepo::new(state.db())
        .remove_member(pipeline_id, principal_id)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;

    Ok(Json(serde_json::json!({ "message": "member removed" })))
}

// ---- Principal search (users + groups) ----

#[derive(Debug, Deserialize)]
struct SearchQuery {
    q: Option<String>,
}

#[derive(Debug, Serialize)]
struct PrincipalSearchResult {
    id: String,
    name: String,
    principal_type: String,
    email: Option<String>,
}

#[instrument(skip(state))]
async fn search_principals(
    State(state): State<AppState>,
    Auth(user): Auth,
    axum::extract::Query(params): axum::extract::Query<SearchQuery>,
) -> ApiResult<Json<Vec<PrincipalSearchResult>>> {
    let q = params.q.unwrap_or_default();
    let q_pattern = format!("%{}%", q.to_lowercase());

    let mut results = Vec::new();

    let users: Vec<(Uuid, String, String, String)> = sqlx::query_as(
        r#"
        SELECT id, username, email, COALESCE(display_name, username) as name
        FROM users
        WHERE org_id = $1 AND deleted_at IS NULL AND is_active = true
          AND (username ILIKE $2 OR email ILIKE $2 OR display_name ILIKE $2)
        ORDER BY username LIMIT 20
        "#,
    )
    .bind(user.org_id.as_uuid())
    .bind(&q_pattern)
    .fetch_all(state.db())
    .await
    .map_err(|e| ApiError::internal(e.to_string()))?;

    for (id, _username, email, display_name) in users {
        results.push(PrincipalSearchResult {
            id: id.to_string(),
            name: display_name,
            principal_type: "user".to_string(),
            email: Some(email),
        });
    }

    let groups: Vec<(Uuid, String)> = sqlx::query_as(
        r#"
        SELECT id, name FROM groups
        WHERE org_id = $1 AND name ILIKE $2
        ORDER BY name LIMIT 20
        "#,
    )
    .bind(user.org_id.as_uuid())
    .bind(&q_pattern)
    .fetch_all(state.db())
    .await
    .map_err(|e| ApiError::internal(e.to_string()))?;

    for (id, name) in groups {
        results.push(PrincipalSearchResult {
            id: id.to_string(),
            name,
            principal_type: "group".to_string(),
            email: None,
        });
    }

    Ok(Json(results))
}
