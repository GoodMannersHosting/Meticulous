//! OIDC identity provider discovery endpoints (ADR-017, Phase 2.2).
//!
//! These routes are **public** (no authentication required) per the OIDC spec.
//! They are separate from the OIDC *consumer* routes in `auth.rs`.

use axum::{Json, Router, extract::State, http::header, response::IntoResponse, routing::get};
use met_store::repos::OidcSigningKeyRepo;
use serde::Serialize;

use crate::{config::ApiConfig, error::ApiResult, state::AppState};

/// OIDC issuer URL without trailing slash (ADR-017). Prefer [`ApiConfig::public_base_url`], then first CORS origin, then a placeholder.
#[must_use]
pub(crate) fn resolve_oidc_issuer_base(config: &ApiConfig) -> String {
    if let Some(ref u) = config.public_base_url {
        let s = u.trim().trim_end_matches('/');
        if !s.is_empty() {
            return s.to_string();
        }
    }
    config
        .cors_origins
        .first()
        .map(|s| s.trim().trim_end_matches('/').to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "https://meticulous.example.com".to_string())
}

/// Mount at the root level (not under `/api/v1`) since OIDC discovery uses
/// well-known paths.
pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/.well-known/openid-configuration",
            get(openid_configuration),
        )
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
    let issuer = resolve_oidc_issuer_base(state.config());

    Ok(Json(OpenIdConfiguration {
        jwks_uri: format!("{issuer}/.well-known/jwks.json"),
        issuer,
        response_types_supported: vec!["id_token".to_string()],
        subject_types_supported: vec!["public".to_string()],
        id_token_signing_alg_values_supported: vec!["ES256".to_string()],
        claims_supported: vec![
            "iss".into(),
            "sub".into(),
            "aud".into(),
            "exp".into(),
            "iat".into(),
            "jti".into(),
            "org_id".into(),
            "org_slug".into(),
            "project_id".into(),
            "project_slug".into(),
            "pipeline_id".into(),
            "pipeline_name".into(),
            "run_id".into(),
            "job_run_id".into(),
            "ref".into(),
            "sha".into(),
            "environment".into(),
            "runner_environment".into(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_issuer_prefers_public_base_url() {
        let mut c = ApiConfig::default();
        c.public_base_url = Some("https://ci.example.com/".to_string());
        assert_eq!(resolve_oidc_issuer_base(&c), "https://ci.example.com");
    }

    #[test]
    fn resolve_issuer_falls_back_to_cors() {
        let mut c = ApiConfig::default();
        c.cors_origins = vec!["http://localhost:8080".to_string()];
        assert_eq!(resolve_oidc_issuer_base(&c), "http://localhost:8080");
    }
}
