//! API token management routes.
//!
//! Provides endpoints for creating, listing, and revoking API tokens.
//! Tokens are scoped to the authenticated user and can optionally be
//! restricted to specific projects.

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{delete, get, post},
};
use met_core::ids::{ApiTokenId, ProjectId};
use met_core::models::{CreateApiToken, CreateApiTokenResponse};
use met_store::repos::ApiTokenRepo;
use serde::{Deserialize, Serialize};
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
        .route("/tokens/{id}", delete(revoke_token))
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
    pub expires_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<String>,
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
            expires_at: t.expires_at.map(|dt| dt.to_rfc3339()),
            last_used_at: t.last_used_at.map(|dt| dt.to_rfc3339()),
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
    if req.name.is_empty() {
        return Err(ApiError::bad_request("token name is required"));
    }

    if req.name.len() > 100 {
        return Err(ApiError::bad_request(
            "token name must be 100 characters or less",
        ));
    }

    // Validate scopes
    let valid_scopes = ["read", "write", "admin", "*"];
    for scope in &req.scopes {
        if !valid_scopes.contains(&scope.as_str()) {
            return Err(ApiError::bad_request(format!(
                "invalid scope '{}', must be one of: read, write, admin",
                scope
            )));
        }
    }

    // Generate the token
    let (plain_token, prefix, token_hash) = generate_token();

    // Convert days to seconds
    let expires_in = req.expires_in_days.map(|days| days * 24 * 60 * 60);

    let description = req.description.filter(|d| !d.trim().is_empty());

    let input = CreateApiToken {
        name: req.name.clone(),
        description,
        scopes: if req.scopes.is_empty() {
            vec!["read".to_string()]
        } else {
            req.scopes
        },
        project_ids: req.project_ids,
        expires_in,
    };

    let repo = ApiTokenRepo::new(state.db());
    let token = repo
        .create(user.user_id, &input, &token_hash, &prefix)
        .await?;

    tracing::info!(
        user_id = %user.user_id,
        token_id = %token.id,
        token_name = %token.name,
        "API token created"
    );

    Ok(Json(CreateTokenResponseBody {
        token: TokenResponse::from(&token),
        plain_token,
    }))
}

/// Revoke an API token.
#[utoipa::path(
    delete,
    path = "/api/v1/tokens/{id}",
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

    // Get the token to verify ownership
    let token = repo.get(token_id).await?;

    if token.user_id != user.user_id {
        // Check if user is admin
        if !user.has_permission("*") {
            return Err(ApiError::forbidden("you can only revoke your own tokens"));
        }
    }

    repo.revoke(token_id).await?;

    tracing::info!(
        user_id = %user.user_id,
        token_id = %token_id,
        "API token revoked"
    );

    Ok(Json(serde_json::json!({ "message": "token revoked" })))
}
