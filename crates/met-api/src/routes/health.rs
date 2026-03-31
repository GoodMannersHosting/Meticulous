//! Health check endpoints.
//!
//! Provides two health endpoints:
//! - `GET /health` - Basic liveness check
//! - `GET /ready` - Readiness check including database connectivity

use crate::error::ApiError;
use crate::state::AppState;
use crate::VERSION;
use axum::{
    Json, Router,
    extract::State,
    routing::get,
};
use serde::Serialize;
use utoipa::ToSchema;

/// Health check response.
#[derive(Debug, Serialize, ToSchema)]
pub struct HealthResponse {
    /// Service status.
    pub status: &'static str,
    /// Service version.
    pub version: &'static str,
}

/// Readiness check response.
#[derive(Debug, Serialize, ToSchema)]
pub struct ReadyResponse {
    /// Overall readiness status.
    pub status: &'static str,
    /// Service version.
    pub version: &'static str,
    /// Individual component checks.
    pub checks: ReadyChecks,
}

/// Component readiness checks.
#[derive(Debug, Serialize, ToSchema)]
pub struct ReadyChecks {
    /// Database connectivity status.
    pub database: CheckStatus,
}

/// Status of an individual check.
#[derive(Debug, Serialize, ToSchema)]
pub struct CheckStatus {
    /// Whether the check passed.
    pub healthy: bool,
    /// Optional message (usually for errors).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Create the health check router.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/health", get(health_handler))
        .route("/ready", get(ready_handler))
}

/// Basic liveness check.
///
/// Returns 200 OK if the service is running.
/// This endpoint is suitable for Kubernetes liveness probes.
#[utoipa::path(
    get,
    path = "/health",
    responses(
        (status = 200, description = "Service is alive", body = HealthResponse),
    ),
    tag = "health",
)]
async fn health_handler() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: VERSION,
    })
}

/// Readiness check with database connectivity.
///
/// Returns 200 OK if all dependencies are healthy.
/// Returns 503 Service Unavailable if any dependency is unhealthy.
/// This endpoint is suitable for Kubernetes readiness probes.
#[utoipa::path(
    get,
    path = "/ready",
    responses(
        (status = 200, description = "Service is ready", body = ReadyResponse),
        (status = 503, description = "Service is not ready"),
    ),
    tag = "health",
)]
async fn ready_handler(State(state): State<AppState>) -> Result<Json<ReadyResponse>, ApiError> {
    let db_check = check_database(&state).await;
    let all_healthy = db_check.healthy;

    let response = ReadyResponse {
        status: if all_healthy { "ready" } else { "degraded" },
        version: VERSION,
        checks: ReadyChecks { database: db_check },
    };

    if all_healthy {
        Ok(Json(response))
    } else {
        // Return the response body even on error for debugging
        Err(ApiError::unavailable("service not ready"))
    }
}

/// Check database connectivity.
async fn check_database(state: &AppState) -> CheckStatus {
    match sqlx::query("SELECT 1")
        .fetch_one(state.db())
        .await
    {
        Ok(_) => CheckStatus {
            healthy: true,
            message: None,
        },
        Err(e) => CheckStatus {
            healthy: false,
            message: Some(format!("database error: {e}")),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_response_serialization() {
        let response = HealthResponse {
            status: "ok",
            version: "0.1.0",
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"status\":\"ok\""));
    }

    #[test]
    fn test_ready_response_serialization() {
        let response = ReadyResponse {
            status: "ready",
            version: "0.1.0",
            checks: ReadyChecks {
                database: CheckStatus {
                    healthy: true,
                    message: None,
                },
            },
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"healthy\":true"));
    }
}
