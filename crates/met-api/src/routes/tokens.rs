//! API token management routes.
//!
//! Provides endpoints for creating, listing, deactivating, reactivating, revoking, and deleting API tokens.

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{delete, get, post},
};
use met_core::ids::{ApiTokenId, PipelineId, ProjectId};
use met_core::models::CreateApiToken;
use met_core::{OrganizationId, UserId};
use met_store::PgPool;
use met_store::repos::{ApiTokenRepo, OrgPolicyRepo, PipelineRepo, ProjectRepo, UserRepo};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use tracing::instrument;
use utoipa::ToSchema;

use crate::{
    auth::generate_token,
    error::{ApiError, ApiResult},
    extractors::{Auth, PaginatedResponse, Pagination},
    state::AppState,
};

/// Build the tokens router.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/tokens", get(list_tokens).post(create_token))
        .route(
            "/tokens/{id}",
            delete(delete_token).post(revoke_token_legacy),
        )
        .route("/tokens/{id}/deactivate", post(deactivate_token))
        .route("/tokens/{id}/reactivate", post(reactivate_token))
        .route("/tokens/{id}/revoke", post(revoke_token))
}

/// Token response (public, without hash).
#[derive(Debug, Serialize, ToSchema)]
pub struct TokenResponse {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub prefix: String,
    pub scopes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pipeline_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deactivated_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revoked_at: Option<String>,
    pub created_at: String,
}

impl From<&met_core::models::ApiToken> for TokenResponse {
    fn from(t: &met_core::models::ApiToken) -> Self {
        Self {
            id: t.id.to_string(),
            name: t.name.clone(),
            description: t.description.clone(),
            prefix: t.prefix.clone(),
            scopes: t.scopes.clone(),
            project_ids: t
                .project_ids
                .as_ref()
                .map(|ids| ids.iter().map(|id| id.to_string()).collect()),
            pipeline_ids: t
                .pipeline_ids
                .as_ref()
                .map(|ids| ids.iter().map(|id| id.to_string()).collect()),
            expires_at: t.expires_at.map(|dt| dt.to_rfc3339()),
            last_used_at: t.last_used_at.map(|dt| dt.to_rfc3339()),
            deactivated_at: t.deactivated_at.map(|dt| dt.to_rfc3339()),
            revoked_at: t.revoked_at.map(|dt| dt.to_rfc3339()),
            created_at: t.created_at.to_rfc3339(),
        }
    }
}

/// Create token request body.
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateTokenRequest {
    /// Display name for the token.
    pub name: String,
    /// Optional description of the token's purpose.
    #[serde(default)]
    pub description: Option<String>,
    /// Permission scopes for this token.
    #[serde(default)]
    pub scopes: Vec<String>,
    /// Project IDs this token can access (None = all projects).
    #[serde(default)]
    #[schema(value_type = Option<Vec<String>>)]
    pub project_ids: Option<Vec<ProjectId>>,
    /// Pipeline IDs this token can access (None = all pipelines in allowed projects).
    #[serde(default)]
    #[schema(value_type = Option<Vec<String>>)]
    pub pipeline_ids: Option<Vec<PipelineId>>,
    /// Expiration in days (None = never expires).
    #[serde(default)]
    pub expires_in_days: Option<i64>,
}

/// Create token response (includes the plain token).
#[derive(Debug, Serialize, ToSchema)]
pub struct CreateTokenResponseBody {
    /// The created token metadata.
    pub token: TokenResponse,
    /// The plain token value (only shown once).
    pub plain_token: String,
}

/// List tokens for the current user.
#[utoipa::path(
    get,
    path = "/api/v1/tokens",
    params(
        ("cursor" = Option<String>, Query, description = "Pagination cursor"),
        ("limit" = Option<u32>, Query, description = "Items per page"),
    ),
    responses(
        (status = 200, description = "List of tokens", body = serde_json::Value),
        (status = 401, description = "Unauthorized"),
    ),
    tag = "tokens",
)]
#[instrument(skip(state))]
async fn list_tokens(
    State(state): State<AppState>,
    Auth(user): Auth,
    pagination: Pagination,
) -> ApiResult<Json<PaginatedResponse<TokenResponse>>> {
    let repo = ApiTokenRepo::new(state.db());
    let tokens = repo
        .list_by_user(user.user_id, pagination.sql_limit(), 0)
        .await?;

    let response = PaginatedResponse::new(
        tokens.iter().map(TokenResponse::from).collect(),
        pagination.limit,
        |t| t.id.clone(),
    );

    Ok(Json(response))
}

/// Create a new API token.
#[utoipa::path(
    post,
    path = "/api/v1/tokens",
    request_body = CreateTokenRequest,
    responses(
        (status = 200, description = "Token created", body = CreateTokenResponseBody),
        (status = 400, description = "Bad request"),
        (status = 401, description = "Unauthorized"),
    ),
    tag = "tokens",
)]
#[instrument(skip(state, req))]
async fn create_token(
    State(state): State<AppState>,
    Auth(user): Auth,
    Json(req): Json<CreateTokenRequest>,
) -> ApiResult<Json<CreateTokenResponseBody>> {
    let body = create_api_token_for_user(
        &state,
        user.org_id,
        user.user_id,
        req,
        true, /* enforce two-active cap for interactive users */
    )
    .await?;
    Ok(Json(body))
}

async fn validate_api_token_project_pipeline_scope(
    db: &PgPool,
    org_id: OrganizationId,
    project_ids: &mut Option<Vec<ProjectId>>,
    pipeline_ids: &Option<Vec<PipelineId>>,
) -> ApiResult<()> {
    let Some(pipes) = pipeline_ids else {
        return Ok(());
    };
    if pipes.is_empty() {
        return Err(ApiError::bad_request(
            "pipeline_ids cannot be empty; omit the field to allow all pipelines within project scope",
        ));
    }
    let pipeline_repo = PipelineRepo::new(db);
    let project_repo = ProjectRepo::new(db);
    let mut inferred: HashSet<ProjectId> = HashSet::new();
    for pid in pipes {
        let p = pipeline_repo.get(*pid).await?;
        let proj = project_repo.get(p.project_id).await?;
        if proj.org_id != org_id {
            return Err(ApiError::bad_request(
                "pipeline is not in your organization",
            ));
        }
        inferred.insert(p.project_id);
    }
    match project_ids {
        None => {
            *project_ids = Some(inferred.into_iter().collect());
        }
        Some(allowed) => {
            for prj in &inferred {
                if !allowed.contains(prj) {
                    return Err(ApiError::bad_request(
                        "each pipeline must belong to a project listed in project_ids",
                    ));
                }
            }
        }
    }
    Ok(())
}

/// Shared creation logic for `POST /tokens` and admin-provisioned service-account tokens.
pub(crate) async fn create_api_token_for_user(
    state: &AppState,
    org_id: OrganizationId,
    owner_user_id: UserId,
    req: CreateTokenRequest,
    enforce_two_active_cap: bool,
) -> ApiResult<CreateTokenResponseBody> {
    if req.name.trim().is_empty() {
        return Err(ApiError::bad_request("token name is required"));
    }

    if req.name.len() > 100 {
        return Err(ApiError::bad_request(
            "token name must be 100 characters or less",
        ));
    }

    let policy_repo = OrgPolicyRepo::new(state.db());
    let policy = policy_repo.get(org_id).await?;
    let max_days = i64::from(policy.max_api_token_ttl_days);

    let valid_scopes = ["read", "write", "admin", "*"];
    for scope in &req.scopes {
        if !valid_scopes.contains(&scope.as_str()) {
            return Err(ApiError::bad_request(format!(
                "invalid scope '{scope}', must be one of: read, write, admin, *",
            )));
        }
    }

    let repo = ApiTokenRepo::new(state.db());
    if enforce_two_active_cap {
        let active = repo.count_valid_active_for_user(owner_user_id).await?;
        if active >= 2 {
            return Err(ApiError::bad_request(
                "at most two active API tokens are allowed per user; deactivate or revoke an existing token first",
            ));
        }
    }

    let mut project_ids = req.project_ids;
    let pipeline_ids = req.pipeline_ids.clone();
    validate_api_token_project_pipeline_scope(state.db(), org_id, &mut project_ids, &pipeline_ids)
        .await?;

    let expires_in_days = match req.expires_in_days {
        None => None,
        Some(d) if d <= 0 => {
            return Err(ApiError::bad_request("expires_in_days must be positive"));
        }
        Some(d) => {
            if d > max_days {
                return Err(ApiError::bad_request(format!(
                    "expires_in_days cannot exceed organization maximum ({max_days} days)"
                )));
            }
            Some(d)
        }
    };

    let (plain_token, prefix, token_hash) = generate_token();

    let expires_in = expires_in_days.map(|days| days * 24 * 60 * 60);

    let description = req.description.filter(|d| !d.trim().is_empty());

    let input = CreateApiToken {
        name: req.name.clone(),
        description,
        scopes: if req.scopes.is_empty() {
            vec!["read".to_string()]
        } else {
            req.scopes.clone()
        },
        project_ids,
        pipeline_ids,
        expires_in,
    };

    let token = repo
        .create(owner_user_id, &input, &token_hash, &prefix)
        .await?;

    tracing::info!(
        user_id = %owner_user_id,
        token_id = %token.id,
        token_name = %token.name,
        "API token created"
    );

    Ok(CreateTokenResponseBody {
        token: TokenResponse::from(&token),
        plain_token,
    })
}

async fn assert_token_manageable(
    db: &PgPool,
    repo: &ApiTokenRepo<'_>,
    token_id: ApiTokenId,
    user: &crate::extractors::CurrentUser,
) -> ApiResult<met_core::models::ApiToken> {
    let token = repo.get(token_id).await?;
    if token.user_id == user.user_id {
        return Ok(token);
    }
    if !user.has_permission("*") {
        return Err(ApiError::forbidden("you can only manage your own tokens"));
    }
    let owner = UserRepo::new(db).get(token.user_id).await?;
    if owner.org_id != user.org_id {
        return Err(ApiError::forbidden(
            "cannot manage tokens for users outside your organization",
        ));
    }
    Ok(token)
}

#[instrument(skip(state))]
async fn deactivate_token(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(token_id): Path<ApiTokenId>,
) -> ApiResult<Json<TokenResponse>> {
    let repo = ApiTokenRepo::new(state.db());
    let _ = assert_token_manageable(state.db(), &repo, token_id, &user).await?;
    repo.set_deactivated(token_id, true).await?;
    let t = repo.get(token_id).await?;
    Ok(Json(TokenResponse::from(&t)))
}

#[instrument(skip(state))]
async fn reactivate_token(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(token_id): Path<ApiTokenId>,
) -> ApiResult<Json<TokenResponse>> {
    let repo = ApiTokenRepo::new(state.db());
    let token = assert_token_manageable(state.db(), &repo, token_id, &user).await?;
    let active = repo.count_valid_active_for_user(token.user_id).await?;
    if active >= 2 {
        return Err(ApiError::bad_request(
            "at most two active API tokens are allowed per user",
        ));
    }
    repo.set_deactivated(token_id, false).await?;
    let t = repo.get(token_id).await?;
    Ok(Json(TokenResponse::from(&t)))
}

/// Permanent revoke (instant kill switch).
#[utoipa::path(
    post,
    path = "/api/v1/tokens/{id}/revoke",
    params(("id" = String, Path, description = "Token ID")),
    responses(
        (status = 200, description = "Token revoked"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Token not found"),
    ),
    tag = "tokens",
)]
#[instrument(skip(state))]
async fn revoke_token(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(token_id): Path<ApiTokenId>,
) -> ApiResult<Json<serde_json::Value>> {
    let repo = ApiTokenRepo::new(state.db());
    let _ = assert_token_manageable(state.db(), &repo, token_id, &user).await?;
    repo.revoke(token_id).await?;
    Ok(Json(serde_json::json!({ "message": "token revoked" })))
}

/// Backwards compatibility: `POST /tokens/{id}` revokes (legacy clients).
#[instrument(skip(state))]
async fn revoke_token_legacy(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(token_id): Path<ApiTokenId>,
) -> ApiResult<Json<serde_json::Value>> {
    revoke_token(State(state), Auth(user), Path(token_id)).await
}

#[utoipa::path(
    delete,
    path = "/api/v1/tokens/{id}",
    params(("id" = String, Path, description = "Token ID")),
    responses(
        (status = 200, description = "Token deleted"),
        (status = 400, description = "Token must be deactivated first"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Token not found"),
    ),
    tag = "tokens",
)]
#[instrument(skip(state))]
async fn delete_token(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(token_id): Path<ApiTokenId>,
) -> ApiResult<Json<serde_json::Value>> {
    let repo = ApiTokenRepo::new(state.db());
    let token = assert_token_manageable(state.db(), &repo, token_id, &user).await?;
    if token.revoked_at.is_some() {
        return Err(ApiError::bad_request("token is already revoked"));
    }
    if token.deactivated_at.is_none() {
        return Err(ApiError::bad_request(
            "deactivate the token before deleting it permanently",
        ));
    }
    repo.delete(token_id).await?;
    Ok(Json(serde_json::json!({ "message": "token deleted" })))
}
