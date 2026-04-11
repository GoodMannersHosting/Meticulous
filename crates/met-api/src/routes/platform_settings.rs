//! Platform settings routes (ADR-021).
//!
//! Only `super_admin` may toggle `allow_unauthenticated_access`.

use axum::{
    Json, Router,
    extract::State,
    routing::{get, patch},
};
use met_store::repos::PlatformSettingsRepo;
use serde::Deserialize;
use tracing::instrument;

use crate::{
    error::{ApiError, ApiResult},
    extractors::Auth,
    project_access::is_super_admin,
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new().route(
        "/platform/settings",
        get(get_platform_settings).patch(update_platform_settings),
    )
}

#[derive(Debug, serde::Serialize)]
struct PlatformSettingsResponse {
    allow_unauthenticated_access: bool,
}

#[instrument(skip(state))]
async fn get_platform_settings(
    State(state): State<AppState>,
    Auth(user): Auth,
) -> ApiResult<Json<PlatformSettingsResponse>> {
    if !is_super_admin(&user) {
        return Err(ApiError::forbidden("requires super_admin"));
    }
    let enabled = PlatformSettingsRepo::new(state.db())
        .allow_unauthenticated_access()
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    Ok(Json(PlatformSettingsResponse {
        allow_unauthenticated_access: enabled,
    }))
}

#[derive(Debug, Deserialize)]
struct UpdatePlatformSettingsRequest {
    #[serde(default)]
    allow_unauthenticated_access: Option<bool>,
}

#[instrument(skip(state, req))]
async fn update_platform_settings(
    State(state): State<AppState>,
    Auth(user): Auth,
    Json(req): Json<UpdatePlatformSettingsRequest>,
) -> ApiResult<Json<PlatformSettingsResponse>> {
    if !is_super_admin(&user) {
        return Err(ApiError::forbidden("requires super_admin"));
    }
    let repo = PlatformSettingsRepo::new(state.db());
    if let Some(enabled) = req.allow_unauthenticated_access {
        repo.set(
            "allow_unauthenticated_access",
            serde_json::Value::Bool(enabled),
            user.user_id,
        )
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    }
    let current = repo
        .allow_unauthenticated_access()
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    Ok(Json(PlatformSettingsResponse {
        allow_unauthenticated_access: current,
    }))
}
