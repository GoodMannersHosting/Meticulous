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
use std::collections::HashMap;
use tracing::instrument;

use crate::{
    error::{ApiError, ApiResult},
    extractors::Auth,
    project_access::is_super_admin,
    state::AppState,
    stored_secret_policy::{self, STORED_SECRET_EXTERNAL_KINDS_KEY},
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
    /// Per-kind toggles for AWS SM, Vault, GCP SM, Azure KV, Kubernetes (`false` = reject create/rotate).
    stored_secret_external_kinds: HashMap<String, bool>,
}

#[instrument(skip(state))]
async fn get_platform_settings(
    State(state): State<AppState>,
    Auth(user): Auth,
) -> ApiResult<Json<PlatformSettingsResponse>> {
    if !is_super_admin(&user) {
        return Err(ApiError::forbidden("requires super_admin"));
    }
    let repo = PlatformSettingsRepo::new(state.db());
    let enabled = repo
        .allow_unauthenticated_access()
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    let stored_secret_external_kinds =
        stored_secret_policy::load_merged_external_kind_policy(state.db())
            .await
            .map_err(|e| ApiError::internal(e.to_string()))?;
    Ok(Json(PlatformSettingsResponse {
        allow_unauthenticated_access: enabled,
        stored_secret_external_kinds,
    }))
}

#[derive(Debug, Deserialize)]
struct UpdatePlatformSettingsRequest {
    #[serde(default)]
    allow_unauthenticated_access: Option<bool>,
    /// Replaces the map of external provider kinds (merge: omitted keys keep previous values).
    #[serde(default)]
    stored_secret_external_kinds: Option<HashMap<String, bool>>,
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
    if let Some(partial) = req.stored_secret_external_kinds {
        let mut merged =
            stored_secret_policy::load_merged_external_kind_policy(state.db())
                .await
                .map_err(|e| ApiError::internal(e.to_string()))?;
        for (k, v) in partial {
            merged.insert(k, v);
        }
        let json = serde_json::to_value(&merged).map_err(|e| ApiError::internal(e.to_string()))?;
        repo.set(STORED_SECRET_EXTERNAL_KINDS_KEY, json, user.user_id)
            .await
            .map_err(|e| ApiError::internal(e.to_string()))?;
    }
    let current = repo
        .allow_unauthenticated_access()
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    let stored_secret_external_kinds =
        stored_secret_policy::load_merged_external_kind_policy(state.db())
            .await
            .map_err(|e| ApiError::internal(e.to_string()))?;
    Ok(Json(PlatformSettingsResponse {
        allow_unauthenticated_access: current,
        stored_secret_external_kinds,
    }))
}
