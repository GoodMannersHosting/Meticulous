//! Secret metadata CRUD routes.
//!
//! Secret values are never exposed through the API. Only metadata
//! (name, description, scope, provider) is returned.

use axum::{
    extract::{Path, State},
    routing::{delete, get, patch, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use met_core::ids::{ProjectId, SecretId};
use serde::{Deserialize, Serialize};
use tracing::instrument;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    error::{ApiError, ApiResult},
    extractors::{Auth, Pagination, PaginatedResponse},
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/projects/{project_id}/secrets",
            get(list_secrets).post(create_secret),
        )
        .route(
            "/secrets/{id}",
            patch(update_secret).delete(delete_secret),
        )
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct SecretRow {
    pub id: Uuid,
    pub project_id: Uuid,
    pub org_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub scope: String,
    pub provider: String,
    pub provider_key: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SecretResponse {
    #[schema(value_type = String)]
    pub id: SecretId,
    #[schema(value_type = String)]
    pub project_id: ProjectId,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub scope: String,
    pub provider: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<SecretRow> for SecretResponse {
    fn from(r: SecretRow) -> Self {
        Self {
            id: SecretId::from_uuid(r.id),
            project_id: ProjectId::from_uuid(r.project_id),
            name: r.name,
            description: r.description,
            scope: r.scope,
            provider: r.provider,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/projects/{project_id}/secrets",
    params(
        ("project_id" = String, Path, description = "Project ID"),
        ("cursor" = Option<String>, Query, description = "Pagination cursor"),
        ("limit" = Option<u32>, Query, description = "Items per page"),
    ),
    responses(
        (status = 200, description = "List of secrets", body = serde_json::Value),
        (status = 403, description = "Forbidden"),
    ),
    tag = "secrets",
)]
#[instrument(skip(state))]
async fn list_secrets(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(project_id): Path<ProjectId>,
    pagination: Pagination,
) -> ApiResult<Json<PaginatedResponse<SecretResponse>>> {
    if !user.can_access_project(project_id) {
        return Err(ApiError::forbidden("no access to this project"));
    }

    let rows = sqlx::query_as::<_, SecretRow>(
        r#"
        SELECT id, project_id, org_id, name, description, scope::text, provider, provider_key, created_at, updated_at
        FROM secrets
        WHERE project_id = $1 AND org_id = $2
        ORDER BY name ASC
        LIMIT $3 OFFSET 0
        "#,
    )
    .bind(project_id.as_uuid())
    .bind(user.org_id.as_uuid())
    .bind(pagination.sql_limit())
    .fetch_all(state.db())
    .await
    .map_err(met_store::StoreError::from)?;

    let response = PaginatedResponse::new(
        rows.into_iter().map(SecretResponse::from).collect(),
        pagination.limit,
        |s| s.id.to_string(),
    );

    Ok(Json(response))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateSecretRequest {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default = "default_scope")]
    pub scope: String,
    pub provider: String,
    pub provider_key: String,
}

fn default_scope() -> String {
    "project".to_string()
}

#[utoipa::path(
    post,
    path = "/api/v1/projects/{project_id}/secrets",
    params(("project_id" = String, Path, description = "Project ID")),
    request_body = CreateSecretRequest,
    responses(
        (status = 200, description = "Secret created", body = SecretResponse),
        (status = 400, description = "Bad request"),
        (status = 403, description = "Forbidden"),
    ),
    tag = "secrets",
)]
#[instrument(skip(state, req))]
async fn create_secret(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(project_id): Path<ProjectId>,
    Json(req): Json<CreateSecretRequest>,
) -> ApiResult<Json<SecretResponse>> {
    if !user.can_access_project(project_id) {
        return Err(ApiError::forbidden("no access to this project"));
    }

    if req.name.is_empty() {
        return Err(ApiError::bad_request("name is required"));
    }

    if req.provider.is_empty() {
        return Err(ApiError::bad_request("provider is required"));
    }

    if req.provider_key.is_empty() {
        return Err(ApiError::bad_request("provider_key is required"));
    }

    let id = Uuid::now_v7();
    let now = Utc::now();

    let row = sqlx::query_as::<_, SecretRow>(
        r#"
        INSERT INTO secrets (id, project_id, org_id, name, description, scope, provider, provider_key, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6::secret_scope, $7, $8, $9, $9)
        RETURNING id, project_id, org_id, name, description, scope::text, provider, provider_key, created_at, updated_at
        "#,
    )
    .bind(id)
    .bind(project_id.as_uuid())
    .bind(user.org_id.as_uuid())
    .bind(&req.name)
    .bind(&req.description)
    .bind(&req.scope)
    .bind(&req.provider)
    .bind(&req.provider_key)
    .bind(now)
    .fetch_one(state.db())
    .await
    .map_err(met_store::StoreError::from)?;

    tracing::info!(secret_id = %id, name = %req.name, "secret created");

    Ok(Json(SecretResponse::from(row)))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateSecretRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub provider: Option<String>,
    pub provider_key: Option<String>,
}

#[utoipa::path(
    patch,
    path = "/api/v1/secrets/{id}",
    params(("id" = String, Path, description = "Secret ID")),
    request_body = UpdateSecretRequest,
    responses(
        (status = 200, description = "Secret updated", body = SecretResponse),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Secret not found"),
    ),
    tag = "secrets",
)]
#[instrument(skip(state, req))]
async fn update_secret(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(id): Path<SecretId>,
    Json(req): Json<UpdateSecretRequest>,
) -> ApiResult<Json<SecretResponse>> {
    let existing = sqlx::query_as::<_, SecretRow>(
        r#"
        SELECT id, project_id, org_id, name, description, scope::text, provider, provider_key, created_at, updated_at
        FROM secrets
        WHERE id = $1
        "#,
    )
    .bind(id.as_uuid())
    .fetch_optional(state.db())
    .await
    .map_err(met_store::StoreError::from)?
    .ok_or_else(|| ApiError::not_found("secret not found"))?;

    if existing.org_id != user.org_id.as_uuid() {
        return Err(ApiError::forbidden("cannot modify secrets in other organizations"));
    }

    let name = req.name.unwrap_or(existing.name);
    let description = req.description.or(existing.description);
    let provider = req.provider.unwrap_or(existing.provider);
    let provider_key = req.provider_key.unwrap_or(existing.provider_key);

    let row = sqlx::query_as::<_, SecretRow>(
        r#"
        UPDATE secrets
        SET name = $2, description = $3, provider = $4, provider_key = $5, updated_at = NOW()
        WHERE id = $1
        RETURNING id, project_id, org_id, name, description, scope::text, provider, provider_key, created_at, updated_at
        "#,
    )
    .bind(id.as_uuid())
    .bind(&name)
    .bind(&description)
    .bind(&provider)
    .bind(&provider_key)
    .fetch_one(state.db())
    .await
    .map_err(met_store::StoreError::from)?;

    tracing::info!(secret_id = %id, "secret updated");

    Ok(Json(SecretResponse::from(row)))
}

#[utoipa::path(
    delete,
    path = "/api/v1/secrets/{id}",
    params(("id" = String, Path, description = "Secret ID")),
    responses(
        (status = 200, description = "Secret deleted"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Secret not found"),
    ),
    tag = "secrets",
)]
#[instrument(skip(state))]
async fn delete_secret(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(id): Path<SecretId>,
) -> ApiResult<Json<serde_json::Value>> {
    let existing = sqlx::query_as::<_, SecretRow>(
        r#"
        SELECT id, project_id, org_id, name, description, scope::text, provider, provider_key, created_at, updated_at
        FROM secrets
        WHERE id = $1
        "#,
    )
    .bind(id.as_uuid())
    .fetch_optional(state.db())
    .await
    .map_err(met_store::StoreError::from)?
    .ok_or_else(|| ApiError::not_found("secret not found"))?;

    if existing.org_id != user.org_id.as_uuid() {
        return Err(ApiError::forbidden("cannot delete secrets in other organizations"));
    }

    sqlx::query("DELETE FROM secrets WHERE id = $1")
        .bind(id.as_uuid())
        .execute(state.db())
        .await
        .map_err(met_store::StoreError::from)?;

    tracing::info!(secret_id = %id, "secret deleted");

    Ok(Json(serde_json::json!({ "message": "secret deleted" })))
}
