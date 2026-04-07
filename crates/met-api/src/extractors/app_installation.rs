//! Extractor for Meticulous App installation JWT (`Bearer` only).

use axum::{
    extract::{FromRef, FromRequestParts},
    http::{header::AUTHORIZATION, request::Parts},
};

use crate::auth::{AppInstallationPrincipal, verify_app_installation_jwt};
use crate::error::ApiError;
use crate::state::AppState;

/// Verified installation principal from an app-issued JWT.
#[derive(Debug, Clone)]
pub struct AppInstallationAuth(pub AppInstallationPrincipal);

impl std::ops::Deref for AppInstallationAuth {
    type Target = AppInstallationPrincipal;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<S> FromRequestParts<S> for AppInstallationAuth
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
        let token = auth_header
            .strip_prefix("Bearer ")
            .ok_or_else(|| {
                ApiError::unauthorized(
                    "Meticulous App integration routes require Authorization: Bearer <jwt>",
                )
            })?;

        let principal =
            verify_app_installation_jwt(token, &app_state.config().jwt, app_state.db()).await?;
        Ok(AppInstallationAuth(principal))
    }
}
