//! Readable policy for stored-secret kind pickers (any authenticated user).

use axum::{Json, Router, extract::State, routing::get};
use serde::Serialize;
use std::collections::HashMap;
use tracing::instrument;

use crate::{error::ApiResult, extractors::Auth, state::AppState, stored_secret_policy};

pub fn router() -> Router<AppState> {
    Router::new().route("/stored-secret-policy", get(get_stored_secret_policy))
}

#[derive(Debug, Serialize)]
struct StoredSecretPolicyResponse {
    /// When `false`, creating or rotating this external kind is rejected (`aws_sm`, `vault`, …).
    stored_secret_external_kinds: HashMap<String, bool>,
}

#[instrument(skip(state))]
async fn get_stored_secret_policy(
    State(state): State<AppState>,
    Auth(_user): Auth,
) -> ApiResult<Json<StoredSecretPolicyResponse>> {
    let stored_secret_external_kinds =
        stored_secret_policy::load_merged_external_kind_policy(state.db())
            .await
            .map_err(|e| crate::error::ApiError::internal(e.to_string()))?;
    Ok(Json(StoredSecretPolicyResponse {
        stored_secret_external_kinds,
    }))
}
