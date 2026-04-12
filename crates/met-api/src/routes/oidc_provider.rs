//! OIDC identity provider discovery endpoints (ADR-017, Phase 2.2).
//!
//! These routes are **public** (no authentication required) per the OIDC spec.
//! They are separate from the OIDC *consumer* routes in `auth.rs`.

use axum::{
    Json, Router,
    extract::State,
    http::header,
    response::IntoResponse,
    routing::get,
};
use met_store::repos::OidcSigningKeyRepo;
use serde::Serialize;

use crate::{
    error::ApiResult,
    state::AppState,
};

/// Mount at the root level (not under `/api/v1`) since OIDC discovery uses
/// well-known paths.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/.well-known/openid-configuration", get(openid_configuration))
        .route("/.well-known/jwks.json", get(jwks))
}

#[derive(Debug, Serialize)]
struct OpenIdConfiguration {
    issuer: String,
    jwks_uri: String,
    response_types_supported: Vec<String>,
    subject_types_supported: Vec<String>,
    id_token_signing_alg_values_supported: Vec<String>,
    claims_supported: Vec<String>,
}

async fn openid_configuration(
    State(state): State<AppState>,
) -> ApiResult<Json<OpenIdConfiguration>> {
    let issuer = state
        .config()
        .cors_origins
        .first()
        .cloned()
        .unwrap_or_else(|| "https://meticulous.example.com".to_string());

    Ok(Json(OpenIdConfiguration {
        jwks_uri: format!("{issuer}/.well-known/jwks.json"),
        issuer,
        response_types_supported: vec!["id_token".to_string()],
        subject_types_supported: vec!["public".to_string()],
        id_token_signing_alg_values_supported: vec!["ES256".to_string()],
        claims_supported: vec![
            "iss".into(), "sub".into(), "aud".into(), "exp".into(), "iat".into(), "jti".into(),
            "org_id".into(), "org_slug".into(), "project_id".into(), "project_slug".into(),
            "pipeline_id".into(), "pipeline_name".into(), "run_id".into(), "job_run_id".into(),
            "ref".into(), "sha".into(), "environment".into(), "runner_environment".into(),
        ],
    }))
}

#[derive(Debug, Serialize)]
struct JwksResponse {
    keys: Vec<serde_json::Value>,
}

async fn jwks(State(state): State<AppState>) -> impl IntoResponse {
    let repo = OidcSigningKeyRepo::new(state.db());
    let keys = match repo.jwks_public_keys().await {
        Ok(rows) => rows.into_iter().map(|r| r.public_key_jwk).collect(),
        Err(e) => {
            tracing::error!("Failed to load JWKS keys: {e}");
            Vec::new()
        }
    };

    let body = JwksResponse { keys };
    (
        [(header::CACHE_CONTROL, "public, max-age=3600")],
        Json(body),
    )
}
