//! Integration API authenticated via Meticulous App JWT.

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use chrono::{Duration, Utc};
use met_core::ids::JoinTokenId;
use met_core::models::{
    JoinToken, JoinTokenScope, app_permissions, generate_join_token,
};
use met_controller::nats::subjects;
use met_store::repos::{JoinTokenRepo, MeticulousAppRepo};
use serde::Serialize;
use tracing::instrument;

use crate::auth::app_jwt::integration_jwt_audience_for_ingress;
use crate::auth::hash_token;
use crate::error::{ApiError, ApiResult};
use crate::extractors::AppInstallationAuth;
use crate::routes::admin::{
    CreateJoinTokenRequest, CreateJoinTokenResponse, enrich_join_tokens,
};
use crate::state::AppState;

/// API routes under `/api/v1/integration`.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/integration/public-context", get(integration_public_context))
        .route(
            "/integration/join-tokens",
            post(integration_create_join_token),
        )
        .route(
            "/integration/join-tokens/{id}/revoke",
            post(integration_revoke_join_token),
        )
}

/// Unauthenticated bootstrap: JWT `aud` for integration clients and job-queue identifiers.
#[derive(Debug, Serialize)]
pub struct IntegrationJobQueueInfo {
    pub jetstream_stream: &'static str,
    pub dispatch_subject_prefix: &'static str,
}

#[derive(Debug, Serialize)]
pub struct IntegrationPublicContextResponse {
    pub jwt_audience: String,
    pub job_queue: IntegrationJobQueueInfo,
}

async fn integration_public_context(State(state): State<AppState>) -> Json<IntegrationPublicContextResponse> {
    Json(IntegrationPublicContextResponse {
        jwt_audience: integration_jwt_audience_for_ingress(&state.config().jwt),
        job_queue: IntegrationJobQueueInfo {
            jetstream_stream: subjects::JOBS_STREAM,
            dispatch_subject_prefix: "met.jobs",
        },
    })
}

#[instrument(skip(state, auth, req))]
async fn integration_create_join_token(
    State(state): State<AppState>,
    auth: AppInstallationAuth,
    Json(req): Json<CreateJoinTokenRequest>,
) -> ApiResult<Json<CreateJoinTokenResponse>> {
    let principal = &auth.0;
    if !principal.permissions.iter().any(|p| {
        p == "*"
            || p == app_permissions::JOIN_TOKENS_CREATE
    }) {
        return Err(ApiError::forbidden(
            "installation does not grant join_tokens:create",
        ));
    }

    let description = req.description.as_deref().unwrap_or("").trim().to_string();
    if description.is_empty() {
        return Err(ApiError::bad_request("description is required"));
    }

    if req.scope.as_deref().is_some_and(|s| s != "project") {
        return Err(ApiError::bad_request(
            "integration join tokens must use project scope only (omit scope to use the installation project)",
        ));
    }
    if let Some(sid) = req.scope_id {
        if sid != principal.project_id.as_uuid() {
            return Err(ApiError::forbidden(
                "scope_id must match the installation project",
            ));
        }
    }

    let app = MeticulousAppRepo::new(state.db())
        .get_by_id(principal.app_id)
        .await
        .map_err(|_| ApiError::internal("app row missing"))?;

    let plain_token = generate_join_token();
    let token_hash = hash_token(&plain_token);
    let expires_at = req
        .expires_in_days
        .map(|days| Utc::now() + Duration::days(days));
    let now = Utc::now();

    let token = JoinToken {
        id: JoinTokenId::new(),
        token_hash,
        scope: JoinTokenScope::Project,
        scope_id: Some(principal.project_id.as_uuid()),
        description,
        org_id: None,
        max_uses: 1,
        current_uses: 0,
        labels: req.labels,
        pool_tags: req.pool_tags,
        expires_at,
        revoked: false,
        created_by: app.created_by,
        created_at: now,
        updated_at: now,
        consumed_by_agent_id: None,
        consumed_at: None,
    };

    let repo = JoinTokenRepo::new(state.db());
    let created = repo.create(&token).await?;

    repo.insert_description_history(
        created.id,
        &created.description,
        created.created_by,
        created.created_at,
    )
    .await?;

    let mut items = enrich_join_tokens(&[created], state.db()).await?;
    let resp_token = items.pop().ok_or_else(|| ApiError::internal("join token enrich failed"))?;

    Ok(Json(CreateJoinTokenResponse {
        token: resp_token,
        plain_token,
    }))
}

#[instrument(skip(state, auth))]
async fn integration_revoke_join_token(
    State(state): State<AppState>,
    auth: AppInstallationAuth,
    Path(token_id): Path<JoinTokenId>,
) -> ApiResult<Json<serde_json::Value>> {
    let principal = &auth.0;
    if !principal.permissions.iter().any(|p| {
        p == "*"
            || p == app_permissions::JOIN_TOKENS_REVOKE
    }) {
        return Err(ApiError::forbidden(
            "installation does not grant join_tokens:revoke",
        ));
    }

    let repo = JoinTokenRepo::new(state.db());
    let token = repo
        .get(token_id)
        .await
        .map_err(|_| ApiError::not_found("join token not found"))?;

    if token.scope != JoinTokenScope::Project
        || token.scope_id != Some(principal.project_id.as_uuid())
    {
        return Err(ApiError::forbidden(
            "join token is not scoped to this installation project",
        ));
    }

    repo.revoke(token_id).await?;
    Ok(Json(serde_json::json!({ "message": "join token revoked" })))
}
