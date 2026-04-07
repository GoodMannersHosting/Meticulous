//! Debug session routes.
//!
//! Provides ephemeral debug sessions for interactive pipeline debugging.
//! Each session gets a proxy URL and single-use secret access.

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use tracing::instrument;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    auth::hash_token,
    error::{ApiError, ApiResult},
    extractors::Auth,
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/debug/sessions", post(create_debug_session))
        .route(
            "/debug/sessions/{session_id}/secrets/{name}",
            get(get_debug_secret),
        )
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateDebugSessionRequest {
    pub project_id: Option<String>,
    pub pipeline_id: Option<String>,
    #[serde(default = "default_ttl_minutes")]
    pub ttl_minutes: i64,
}

fn default_ttl_minutes() -> i64 {
    15
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DebugSessionResponse {
    pub session_id: String,
    pub proxy_url: String,
    pub token: String,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[utoipa::path(
    post,
    path = "/api/v1/debug/sessions",
    request_body = CreateDebugSessionRequest,
    responses(
        (status = 200, description = "Debug session created", body = DebugSessionResponse),
        (status = 400, description = "Bad request"),
    ),
    tag = "debug",
)]
#[instrument(skip(state, req))]
async fn create_debug_session(
    State(state): State<AppState>,
    Auth(user): Auth,
    Json(req): Json<CreateDebugSessionRequest>,
) -> ApiResult<Json<DebugSessionResponse>> {
    if req.ttl_minutes < 1 || req.ttl_minutes > 480 {
        return Err(ApiError::bad_request(
            "ttl_minutes must be between 1 and 480",
        ));
    }

    let project_id: Option<Uuid> = req
        .project_id
        .map(|r| {
            r.parse()
                .map_err(|_| ApiError::bad_request("invalid project_id"))
        })
        .transpose()?;

    let pipeline_id: Option<Uuid> = req
        .pipeline_id
        .map(|r| {
            r.parse()
                .map_err(|_| ApiError::bad_request("invalid pipeline_id"))
        })
        .transpose()?;

    let id = Uuid::now_v7();
    let now = Utc::now();
    let expires_at = now + Duration::minutes(req.ttl_minutes);

    let (token, _prefix, token_hash) = crate::auth::generate_token();
    let proxy_url = format!("/api/v1/debug/sessions/{}/secrets", id);

    sqlx::query(
        r#"
        INSERT INTO debug_sessions (id, user_id, project_id, pipeline_id, token_hash, proxy_url, expires_at, created_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        "#,
    )
    .bind(id)
    .bind(user.user_id.as_uuid())
    .bind(project_id)
    .bind(pipeline_id)
    .bind(&token_hash)
    .bind(&proxy_url)
    .bind(expires_at)
    .bind(now)
    .execute(state.db())
    .await
    .map_err(met_store::StoreError::from)?;

    tracing::info!(
        session_id = %id,
        user_id = %user.user_id,
        expires_at = %expires_at,
        "debug session created"
    );

    Ok(Json(DebugSessionResponse {
        session_id: id.to_string(),
        proxy_url,
        token,
        expires_at,
        created_at: now,
    }))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DebugSecretResponse {
    pub name: String,
    pub value: String,
    pub consumed: bool,
}

#[utoipa::path(
    get,
    path = "/api/v1/debug/sessions/{session_id}/secrets/{name}",
    params(
        ("session_id" = String, Path, description = "Debug session ID"),
        ("name" = String, Path, description = "Secret name"),
    ),
    responses(
        (status = 200, description = "Debug secret (single-use)", body = DebugSecretResponse),
        (status = 400, description = "Session expired"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Session not found"),
        (status = 409, description = "Secret already consumed"),
    ),
    tag = "debug",
)]
#[instrument(skip(state))]
async fn get_debug_secret(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path((session_id, name)): Path<(String, String)>,
) -> ApiResult<Json<DebugSecretResponse>> {
    let session_uuid: Uuid = session_id
        .parse()
        .map_err(|_| ApiError::bad_request("invalid session_id"))?;

    let session: Option<(Uuid, Uuid, DateTime<Utc>)> = sqlx::query_as(
        "SELECT id, user_id, expires_at FROM debug_sessions WHERE id = $1 AND closed_at IS NULL",
    )
    .bind(session_uuid)
    .fetch_optional(state.db())
    .await
    .map_err(met_store::StoreError::from)?;

    let (_, owner_id, expires_at) =
        session.ok_or_else(|| ApiError::not_found("debug session not found"))?;

    if owner_id != user.user_id.as_uuid() {
        return Err(ApiError::forbidden("not your debug session"));
    }

    if expires_at < Utc::now() {
        return Err(ApiError::bad_request("debug session has expired"));
    }

    let already_consumed: Option<(bool,)> = sqlx::query_as(
        "SELECT consumed FROM debug_session_secrets WHERE session_id = $1 AND secret_name = $2",
    )
    .bind(session_uuid)
    .bind(&name)
    .fetch_optional(state.db())
    .await
    .map_err(met_store::StoreError::from)?;

    if let Some((true,)) = already_consumed {
        return Err(ApiError::conflict("secret has already been consumed"));
    }

    sqlx::query(
        r#"
        INSERT INTO debug_session_secrets (session_id, secret_name, consumed, consumed_at)
        VALUES ($1, $2, true, NOW())
        ON CONFLICT (session_id, secret_name)
        DO UPDATE SET consumed = true, consumed_at = NOW()
        "#,
    )
    .bind(session_uuid)
    .bind(&name)
    .execute(state.db())
    .await
    .map_err(met_store::StoreError::from)?;

    tracing::info!(
        session_id = %session_id,
        secret_name = %name,
        "debug secret consumed (single-use)"
    );

    Ok(Json(DebugSecretResponse {
        name,
        value: "***PROXY_WOULD_RESOLVE_FROM_SECRETS_BROKER***".to_string(),
        consumed: true,
    }))
}
