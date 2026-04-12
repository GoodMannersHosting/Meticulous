//! Variable CRUD routes.

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{delete, get, patch, post},
};
use chrono::{DateTime, Utc};
use met_core::ids::{PipelineId, ProjectId, VariableId};
use met_store::repos::PipelineRepo;
use serde::{Deserialize, Serialize};
use tracing::instrument;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    error::{ApiError, ApiResult},
    extractors::{Auth, PaginatedResponse, Pagination, SessionOrAppAuth},
    project_access::{
        caller_org_id, effective_project_role_in_user_org,
        effective_project_role_session_or_app_in_user_org,
    },
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/projects/{project_id}/variables",
            get(list_variables).post(create_variable),
        )
        .route(
            "/variables/{id}",
            patch(update_variable).delete(delete_variable),
        )
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct VariableRow {
    pub id: Uuid,
    pub project_id: Uuid,
    pub org_id: Uuid,
    pub pipeline_id: Option<Uuid>,
    pub environment_id: Option<Uuid>,
    pub name: String,
    pub value: String,
    pub scope: String,
    pub is_sensitive: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct VariableResponse {
    #[schema(value_type = String)]
    pub id: VariableId,
    #[schema(value_type = String)]
    pub project_id: ProjectId,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<String>)]
    pub pipeline_id: Option<PipelineId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment_id: Option<Uuid>,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    pub scope: String,
    pub is_sensitive: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<VariableRow> for VariableResponse {
    fn from(r: VariableRow) -> Self {
        let value = if r.is_sensitive { None } else { Some(r.value) };
        Self {
            id: VariableId::from_uuid(r.id),
            project_id: ProjectId::from_uuid(r.project_id),
            pipeline_id: r.pipeline_id.map(PipelineId::from_uuid),
            environment_id: r.environment_id,
            name: r.name,
            value,
            scope: r.scope,
            is_sensitive: r.is_sensitive,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/projects/{project_id}/variables",
    params(
        ("project_id" = String, Path, description = "Project ID"),
        ("cursor" = Option<String>, Query, description = "Pagination cursor"),
        ("limit" = Option<u32>, Query, description = "Items per page"),
    ),
    responses(
        (status = 200, description = "List of variables", body = serde_json::Value),
        (status = 403, description = "Forbidden"),
    ),
    tag = "variables",
)]
#[instrument(skip(state))]
async fn list_variables(
    State(state): State<AppState>,
    SessionOrAppAuth(caller): SessionOrAppAuth,
    Path(project_id): Path<ProjectId>,
    pagination: Pagination,
) -> ApiResult<Json<PaginatedResponse<VariableResponse>>> {
    effective_project_role_session_or_app_in_user_org(state.db(), &caller, project_id).await?;

    let rows = sqlx::query_as::<_, VariableRow>(
        r#"
        SELECT id, project_id, org_id, pipeline_id, name, value, scope::text, is_sensitive, created_at, updated_at
        FROM variables
        WHERE project_id = $1 AND org_id = $2
        ORDER BY pipeline_id NULLS FIRST, name ASC
        LIMIT $3 OFFSET 0
        "#,
    )
    .bind(project_id.as_uuid())
    .bind(caller_org_id(&caller).as_uuid())
    .bind(pagination.sql_limit())
    .fetch_all(state.db())
    .await
    .map_err(met_store::StoreError::from)?;

    let response = PaginatedResponse::new(
        rows.into_iter().map(VariableResponse::from).collect(),
        pagination.limit,
        |v| v.id.to_string(),
    );

    Ok(Json(response))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateVariableRequest {
    pub name: String,
    pub value: String,
    #[serde(default = "default_scope")]
    pub scope: String,
    #[serde(default)]
    pub is_sensitive: bool,
    #[serde(default)]
    #[schema(value_type = Option<String>)]
    pub pipeline_id: Option<PipelineId>,
    #[serde(default)]
    pub environment_id: Option<Uuid>,
}

fn default_scope() -> String {
    "project".to_string()
}

#[utoipa::path(
    post,
    path = "/api/v1/projects/{project_id}/variables",
    params(("project_id" = String, Path, description = "Project ID")),
    request_body = CreateVariableRequest,
    responses(
        (status = 200, description = "Variable created", body = VariableResponse),
        (status = 400, description = "Bad request"),
        (status = 403, description = "Forbidden"),
    ),
    tag = "variables",
)]
#[instrument(skip(state, req))]
async fn create_variable(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(project_id): Path<ProjectId>,
    Json(req): Json<CreateVariableRequest>,
) -> ApiResult<Json<VariableResponse>> {
    let role = effective_project_role_in_user_org(state.db(), &user, project_id).await?;
    if !role.can_write_variables() {
        return Err(ApiError::forbidden(
            "developer or administrator project role is required to create variables",
        ));
    }

    if req.name.is_empty() {
        return Err(ApiError::bad_request("name is required"));
    }

    if let Some(pid) = req.pipeline_id {
        let pl = PipelineRepo::new(state.db())
            .get(pid)
            .await
            .map_err(met_store::StoreError::from)?;
        if pl.project_id.as_uuid() != project_id.as_uuid() {
            return Err(ApiError::bad_request(
                "pipeline does not belong to this project",
            ));
        }
    }

    let id = Uuid::now_v7();
    let now = Utc::now();

    let row = sqlx::query_as::<_, VariableRow>(
        r#"
        INSERT INTO variables (id, project_id, org_id, pipeline_id, name, value, scope, is_sensitive, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7::variable_scope, $8, $9, $9)
        RETURNING id, project_id, org_id, pipeline_id, name, value, scope::text, is_sensitive, created_at, updated_at
        "#,
    )
    .bind(id)
    .bind(project_id.as_uuid())
    .bind(user.org_id.as_uuid())
    .bind(req.pipeline_id.map(|p| p.as_uuid()))
    .bind(&req.name)
    .bind(&req.value)
    .bind(&req.scope)
    .bind(req.is_sensitive)
    .bind(now)
    .fetch_one(state.db())
    .await
    .map_err(met_store::StoreError::from)?;

    tracing::info!(variable_id = %id, name = %req.name, "variable created");

    Ok(Json(VariableResponse::from(row)))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateVariableRequest {
    pub name: Option<String>,
    pub value: Option<String>,
    pub is_sensitive: Option<bool>,
}

#[utoipa::path(
    patch,
    path = "/api/v1/variables/{id}",
    params(("id" = String, Path, description = "Variable ID")),
    request_body = UpdateVariableRequest,
    responses(
        (status = 200, description = "Variable updated", body = VariableResponse),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Variable not found"),
    ),
    tag = "variables",
)]
#[instrument(skip(state, req))]
async fn update_variable(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(id): Path<VariableId>,
    Json(req): Json<UpdateVariableRequest>,
) -> ApiResult<Json<VariableResponse>> {
    let existing = sqlx::query_as::<_, VariableRow>(
        r#"
        SELECT id, project_id, org_id, pipeline_id, name, value, scope::text, is_sensitive, created_at, updated_at
        FROM variables
        WHERE id = $1
        "#,
    )
    .bind(id.as_uuid())
    .fetch_optional(state.db())
    .await
    .map_err(met_store::StoreError::from)?
    .ok_or_else(|| ApiError::not_found("variable not found"))?;

    if existing.org_id != user.org_id.as_uuid() {
        return Err(ApiError::forbidden(
            "cannot modify variables in other organizations",
        ));
    }

    let project_id = ProjectId::from_uuid(existing.project_id);
    let role = effective_project_role_in_user_org(state.db(), &user, project_id).await?;
    if !role.can_write_variables() {
        return Err(ApiError::forbidden(
            "developer or administrator project role is required to update variables",
        ));
    }

    let name = req.name.unwrap_or(existing.name);
    let value = req.value.unwrap_or(existing.value);
    let is_sensitive = req.is_sensitive.unwrap_or(existing.is_sensitive);

    let row = sqlx::query_as::<_, VariableRow>(
        r#"
        UPDATE variables
        SET name = $2, value = $3, is_sensitive = $4, updated_at = NOW()
        WHERE id = $1
        RETURNING id, project_id, org_id, pipeline_id, name, value, scope::text, is_sensitive, created_at, updated_at
        "#,
    )
    .bind(id.as_uuid())
    .bind(&name)
    .bind(&value)
    .bind(is_sensitive)
    .fetch_one(state.db())
    .await
    .map_err(met_store::StoreError::from)?;

    tracing::info!(variable_id = %id, "variable updated");

    Ok(Json(VariableResponse::from(row)))
}

#[utoipa::path(
    delete,
    path = "/api/v1/variables/{id}",
    params(("id" = String, Path, description = "Variable ID")),
    responses(
        (status = 200, description = "Variable deleted"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Variable not found"),
    ),
    tag = "variables",
)]
#[instrument(skip(state))]
async fn delete_variable(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(id): Path<VariableId>,
) -> ApiResult<Json<serde_json::Value>> {
    let existing = sqlx::query_as::<_, VariableRow>(
        r#"
        SELECT id, project_id, org_id, pipeline_id, name, value, scope::text, is_sensitive, created_at, updated_at
        FROM variables
        WHERE id = $1
        "#,
    )
    .bind(id.as_uuid())
    .fetch_optional(state.db())
    .await
    .map_err(met_store::StoreError::from)?
    .ok_or_else(|| ApiError::not_found("variable not found"))?;

    if existing.org_id != user.org_id.as_uuid() {
        return Err(ApiError::forbidden(
            "cannot delete variables in other organizations",
        ));
    }

    let project_id = ProjectId::from_uuid(existing.project_id);
    let role = effective_project_role_in_user_org(state.db(), &user, project_id).await?;
    if !role.can_write_variables() {
        return Err(ApiError::forbidden(
            "developer or administrator project role is required to delete variables",
        ));
    }

    sqlx::query("DELETE FROM variables WHERE id = $1")
        .bind(id.as_uuid())
        .execute(state.db())
        .await
        .map_err(met_store::StoreError::from)?;

    tracing::info!(variable_id = %id, "variable deleted");

    Ok(Json(serde_json::json!({ "message": "variable deleted" })))
}
