//! Route assembly and API versioning.
//!
//! All API routes are mounted under `/api/v1`, with health checks at the root level.
//! Auth routes are mounted at `/auth/*` for login, logout, and setup.
//! Admin REST routes are mounted at `/api/v1/admin/*`. SvelteKit serves `/admin/*` pages.

use crate::middleware::{cors_layer, logging_layer, rate_limit_layer};
use crate::openapi::ApiDoc;
use crate::state::AppState;
use axum::Router;
use http::{HeaderName, HeaderValue};
use tower::ServiceBuilder;
use tower_http::{
    compression::CompressionLayer,
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    set_header::SetResponseHeaderLayer,
};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

pub mod admin;
pub mod admin_workflows;
pub mod agents;
pub mod artifacts;
pub mod auth;
pub mod dashboard;
pub mod debug;
pub mod environments;
pub mod health;
pub mod integration;
pub mod members;
pub mod meticulous_apps;
pub mod oauth;
pub mod oidc_provider;
pub mod orgs;
pub mod pipeline_check;
pub mod pipelines;
pub mod platform_settings;
pub mod projects;
pub mod runs;
pub mod security;
pub mod secrets;
pub mod stored_secrets;
pub mod tokens;
pub mod triggers;
pub mod variables;
pub mod webhooks;
pub mod websocket;
pub mod workflows;
pub mod workflows_catalog;
pub mod workspace_config;

/// Build the complete API router with all middleware.
pub fn build_router(state: AppState) -> Router {
    let config = state.config();

    // Build the middleware stack (applied bottom-to-top, so list in reverse order)
    let middleware_stack = ServiceBuilder::new()
        // Outermost: Request ID (set on request, propagate to response)
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .layer(PropagateRequestIdLayer::x_request_id())
        // Logging
        .layer(logging_layer())
        // CORS
        .layer(cors_layer(config))
        // Compression
        .layer(CompressionLayer::new());

    // Build versioned API routes
    let api_v1 = Router::new()
        .nest("/admin", admin::router())
        .merge(dashboard::router())
        .merge(projects::router())
        .merge(pipelines::router())
        .merge(triggers::router())
        .merge(runs::router())
        .merge(security::router())
        .merge(agents::router())
        .merge(tokens::router())
        .merge(orgs::router())
        .merge(secrets::router())
        .merge(stored_secrets::router())
        .merge(variables::router())
        .merge(workspace_config::router())
        .merge(workflows::router())
        .merge(workflows_catalog::router())
        .merge(debug::router())
        .merge(artifacts::router())
        .merge(webhooks::router())
        .merge(websocket::router())
        .merge(integration::router())
        .merge(members::router())
        .merge(platform_settings::router())
        .merge(environments::router())
        .merge(pipeline_check::router());

    let openapi_spec = ApiDoc::openapi();
    let swagger_router: Router<()> = SwaggerUi::new("/docs")
        .url("/api/v1/openapi.json", openapi_spec)
        .into();

    // Assemble the complete router
    let mut router = Router::new()
        // Health checks at root level (no auth required)
        .merge(health::router())
        // Auth routes at root level (login, logout, setup)
        .merge(auth::router())
        // OAuth routes (OIDC, GitHub)
        .merge(oauth::router())
        // OIDC identity provider discovery (ADR-017)
        .merge(oidc_provider::router())
        // API v1 routes (includes `/api/v1/admin/*`)
        .nest("/api/v1", api_v1)
        // Apply middleware stack
        .layer(middleware_stack)
        // Attach state
        .with_state(state.clone())
        // Swagger UI and OpenAPI spec (stateless)
        .merge(swagger_router);

    router = router
        .layer(SetResponseHeaderLayer::overriding(
            HeaderName::from_static("x-content-type-options"),
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            HeaderName::from_static("referrer-policy"),
            HeaderValue::from_static("strict-origin-when-cross-origin"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            HeaderName::from_static("permissions-policy"),
            HeaderValue::from_static("accelerometer=(), camera=(), geolocation=(), gyroscope=(), magnetometer=(), microphone=(), payment=(), usb=()"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            HeaderName::from_static("content-security-policy-report-only"),
            HeaderValue::from_static(
                "default-src 'self'; frame-ancestors 'self'; form-action 'self'; base-uri 'self'; object-src 'none'",
            ),
        ));

    if config.enable_hsts {
        router = router.layer(SetResponseHeaderLayer::overriding(
            HeaderName::from_static("strict-transport-security"),
            HeaderValue::from_static("max-age=86400; includeSubDomains"),
        ));
    }

    // Conditionally add rate limiting
    if let Some(rate_limit) = rate_limit_layer(&config.rate_limit) {
        router = router.layer(rate_limit);
    }

    router
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_router_builds() {
        // Full integration tests require a database connection.
        // This test verifies the module compiles correctly.
    }
}
