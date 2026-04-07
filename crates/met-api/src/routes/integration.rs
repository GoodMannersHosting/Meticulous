//! Integration API authenticated via Meticulous App JWT.

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{delete, get, post},
};
use chrono::{Duration, Utc};
use met_controller::nats::subjects;
use met_core::ids::{AgentId, JoinTokenId};
use met_core::models::{JoinToken, JoinTokenScope, app_permissions, generate_join_token};
use met_store::StoreError;
use met_store::repos::{AgentRepo, JoinTokenRepo, MeticulousAppRepo, ProjectRepo};
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::auth::app_jwt::integration_jwt_audience_for_ingress;
use crate::auth::hash_token;
use crate::error::{ApiError, ApiResult};
use crate::extractors::AppInstallationAuth;
use crate::routes::admin::{CreateJoinTokenRequest, CreateJoinTokenResponse, enrich_join_tokens};
use crate::state::AppState;

/// API routes under `/api/v1/integration`.
pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/integration/public-context",
            get(integration_public_context),
        )
        .route(
            "/integration/join-tokens",
            post(integration_create_join_token),
        )
        .route(
            "/integration/join-tokens/{id}/revoke",
            post(integration_revoke_join_token),
        )
        .route(
            "/integration/agents/cleanup-by-kubernetes-pod",
            post(integration_cleanup_agent_by_kubernetes_pod),
        )
        .route("/integration/agents/{id}", delete(integration_delete_agent))
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

async fn integration_public_context(
    State(state): State<AppState>,
) -> Json<IntegrationPublicContextResponse> {
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
    if !principal
        .permissions
        .iter()
        .any(|p| p == "*" || p == app_permissions::JOIN_TOKENS_CREATE)
    {
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
    let resp_token = items
        .pop()
        .ok_or_else(|| ApiError::internal("join token enrich failed"))?;

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
    if !principal
        .permissions
        .iter()
        .any(|p| p == "*" || p == app_permissions::JOIN_TOKENS_REVOKE)
    {
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

#[derive(Debug, Deserialize)]
pub struct CleanupAgentByKubernetesPodRequest {
    pub kubernetes_pod_uid: String,
}

#[instrument(skip(state, auth))]
async fn integration_cleanup_agent_by_kubernetes_pod(
    State(state): State<AppState>,
    auth: AppInstallationAuth,
    Json(req): Json<CleanupAgentByKubernetesPodRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let principal = &auth.0;
    if !principal
        .permissions
        .iter()
        .any(|p| p == "*" || p == app_permissions::AGENTS_DELETE)
    {
        return Err(ApiError::forbidden(
            "installation does not grant agents:delete",
        ));
    }

    let pod_uid = req.kubernetes_pod_uid.trim();
    if pod_uid.is_empty() {
        return Err(ApiError::bad_request("kubernetes_pod_uid is required"));
    }

    let org_id = ProjectRepo::new(state.db())
        .get(principal.project_id)
        .await?
        .org_id;

    let repo = AgentRepo::new(state.db());
    let Some(agent_id) = repo
        .find_active_id_by_kubernetes_pod_uid(org_id, pod_uid)
        .await?
    else {
        return Ok(Json(serde_json::json!({
            "message": "no matching agent",
            "agent_id": serde_json::Value::Null
        })));
    };

    match repo.soft_delete(org_id, agent_id).await {
        Ok(()) => Ok(Json(serde_json::json!({
            "message": "agent removed",
            "agent_id": agent_id.to_string()
        }))),
        Err(StoreError::Constraint(msg)) => Err(ApiError::conflict(msg)),
        Err(e) => Err(e.into()),
    }
}

#[instrument(skip(state, auth))]
async fn integration_delete_agent(
    State(state): State<AppState>,
    auth: AppInstallationAuth,
    Path(agent_id): Path<AgentId>,
) -> ApiResult<Json<serde_json::Value>> {
    let principal = &auth.0;
    if !principal
        .permissions
        .iter()
        .any(|p| p == "*" || p == app_permissions::AGENTS_DELETE)
    {
        return Err(ApiError::forbidden(
            "installation does not grant agents:delete",
        ));
    }

    let org_id = ProjectRepo::new(state.db())
        .get(principal.project_id)
        .await?
        .org_id;

    let repo = AgentRepo::new(state.db());
    let agent = repo.get(agent_id).await?;
    if agent.org_id != org_id {
        return Err(ApiError::forbidden(
            "agent is outside this installation org",
        ));
    }

    match repo.soft_delete(org_id, agent_id).await {
        Ok(()) => Ok(Json(serde_json::json!({
            "message": "agent removed",
            "agent_id": agent_id.to_string()
        }))),
        Err(StoreError::NotFound { .. }) => Ok(Json(serde_json::json!({
            "message": "agent already removed",
            "agent_id": agent_id.to_string()
        }))),
        Err(StoreError::Constraint(msg)) => Err(ApiError::conflict(msg)),
        Err(e) => Err(e.into()),
    }
}
