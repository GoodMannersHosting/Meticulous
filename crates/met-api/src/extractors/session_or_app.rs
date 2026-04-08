//! User session (`Bearer` HS256 / `Token`) or Meticulous App installation JWT (`Bearer` RS256 / ES256).

use axum::{
    extract::{FromRef, FromRequestParts},
    http::{header::AUTHORIZATION, request::Parts},
};
use jsonwebtoken::{Algorithm, decode_header};

use crate::auth::{ApiTokenValidator, JwtValidator, verify_app_installation_jwt};
use crate::error::ApiError;
use crate::extractors::auth::finalize_authenticated_user;
use crate::project_access::SessionOrApp;
use crate::state::AppState;

/// Resolves [`SessionOrApp`] from `Authorization`.
#[derive(Debug, Clone)]
pub struct SessionOrAppAuth(pub SessionOrApp);

impl std::ops::Deref for SessionOrAppAuth {
    type Target = SessionOrApp;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<S> FromRequestParts<S> for SessionOrAppAuth
where
    S: Send + Sync,
    AppState: FromRef<S>,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let app_state = AppState::from_ref(state);

        let auth_header = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| ApiError::unauthorized("missing authorization header"))?;

        let method = parts.method.clone();
        let path = parts.uri.path().to_string();

        if let Some(token) = auth_header.strip_prefix("Bearer ") {
            let header = decode_header(token)
                .map_err(|e| ApiError::unauthorized(e.to_string()))?;

            match header.alg {
                Algorithm::HS256 => {
                    let validator = JwtValidator::new(&app_state.config.jwt);
                    let user = validator
                        .validate(token)
                        .map_err(|e| ApiError::unauthorized(e.to_string()))?;
                    let user = finalize_authenticated_user(&app_state, user, &method, &path).await?;
                    return Ok(SessionOrAppAuth(SessionOrApp::User(user)));
                }
                Algorithm::RS256 | Algorithm::ES256 => {
                    let principal = verify_app_installation_jwt(
                        token,
                        &app_state.config().jwt,
                        app_state.db(),
                    )
                    .await?;

                    if let Some(limiter) = app_state.credential_rate_limit.as_ref() {
                        let policy = met_store::repos::OrgPolicyRepo::new(app_state.db())
                            .get(principal.org_id)
                            .await
                            .map_err(|e| ApiError::internal(e.to_string()))?;
                        limiter
                            .check_app(principal.installation_id, &policy)
                            .map_err(|_| ApiError::rate_limited("app credential rate limit exceeded"))?;
                    }

                    return Ok(SessionOrAppAuth(SessionOrApp::App(principal)));
                }
                _ => {
                    return Err(ApiError::unauthorized(
                        "unsupported Bearer token algorithm for this endpoint",
                    ));
                }
            }
        }

        if let Some(token) = auth_header.strip_prefix("Token ") {
            let validator = ApiTokenValidator::new(app_state.db());
            let user = validator
                .validate(token)
                .await
                .map_err(|e| ApiError::unauthorized(e.to_string()))?;
            let user = finalize_authenticated_user(&app_state, user, &method, &path).await?;
            return Ok(SessionOrAppAuth(SessionOrApp::User(user)));
        }

        Err(ApiError::unauthorized(
            "invalid authorization header format, expected 'Bearer <jwt>' or 'Token met_<token>'",
        ))
    }
}
