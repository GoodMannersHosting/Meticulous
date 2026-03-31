//! Route assembly and API versioning.
//!
//! All API routes are mounted under `/api/v1`, with health checks at the root level.
//! Auth routes are mounted at `/auth/*` for login, logout, and setup.

use crate::middleware::{cors_layer, logging_layer, rate_limit_layer};
use crate::state::AppState;
use axum::Router;
use tower::ServiceBuilder;
use tower_http::{
    compression::CompressionLayer,
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
};

pub mod agents;
pub mod auth;
pub mod health;
pub mod pipelines;
pub mod runs;
pub mod webhooks;
pub mod websocket;

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
        .merge(pipelines::router())
        .merge(runs::router())
        .merge(agents::router())
        .merge(webhooks::router())
        .merge(websocket::router());

    // Assemble the complete router
    let mut router = Router::new()
        // Health checks at root level (no auth required)
        .merge(health::router())
        // Auth routes at root level (login, logout, setup)
        .merge(auth::router())
        // API v1 routes
        .nest("/api/v1", api_v1)
        // Apply middleware stack
        .layer(middleware_stack)
        // Attach state
        .with_state(state.clone());

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
