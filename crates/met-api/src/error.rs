//! API error types and JSON error responses.
//!
//! All errors returned by the API are converted to a consistent JSON format:
//! ```json
//! {
//!   "error": {
//!     "code": "not_found",
//!     "message": "Pipeline not found",
//!     "request_id": "01234567-89ab-cdef-0123-456789abcdef"
//!   }
//! }
//! ```

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use met_core::MetError;
use serde::Serialize;
use std::borrow::Cow;
use utoipa::ToSchema;

/// Result type for API operations.
pub type ApiResult<T> = Result<T, ApiError>;

/// API error with status code and JSON body.
#[derive(Debug)]
pub struct ApiError {
    /// HTTP status code.
    status: StatusCode,
    /// Error code (machine-readable).
    code: Cow<'static, str>,
    /// Human-readable error message.
    message: String,
    /// Request ID for tracing (populated from context).
    request_id: Option<String>,
}

impl ApiError {
    /// Create a new API error.
    pub fn new(status: StatusCode, code: impl Into<Cow<'static, str>>, message: impl Into<String>) -> Self {
        Self {
            status,
            code: code.into(),
            message: message.into(),
            request_id: None,
        }
    }

    /// Attach a request ID to the error.
    pub fn with_request_id(mut self, request_id: impl Into<String>) -> Self {
        self.request_id = Some(request_id.into());
        self
    }

    /// Create a 400 Bad Request error.
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "bad_request", message)
    }

    /// Create a 401 Unauthorized error.
    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "unauthorized", message)
    }

    /// Create a 403 Forbidden error.
    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, "forbidden", message)
    }

    /// Create a 404 Not Found error.
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, "not_found", message)
    }

    /// Create a 409 Conflict error.
    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, "conflict", message)
    }

    /// Create a 422 Unprocessable Entity error.
    pub fn unprocessable(message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNPROCESSABLE_ENTITY, "validation_error", message)
    }

    /// Create a 429 Too Many Requests error.
    pub fn rate_limited(message: impl Into<String>) -> Self {
        Self::new(StatusCode::TOO_MANY_REQUESTS, "rate_limited", message)
    }

    /// Create a 500 Internal Server Error.
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", message)
    }

    /// Create a 503 Service Unavailable error.
    pub fn unavailable(message: impl Into<String>) -> Self {
        Self::new(StatusCode::SERVICE_UNAVAILABLE, "service_unavailable", message)
    }
}

/// JSON error response body.
#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorResponse {
    pub error: ErrorBody,
}

/// Inner error body.
#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorBody {
    #[schema(value_type = String)]
    pub code: Cow<'static, str>,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = ErrorResponse {
            error: ErrorBody {
                code: self.code,
                message: self.message,
                request_id: self.request_id,
            },
        };

        (self.status, Json(body)).into_response()
    }
}

impl From<MetError> for ApiError {
    fn from(err: MetError) -> Self {
        match err {
            MetError::NotFound { entity, id } => {
                Self::not_found(format!("{entity} with id '{id}' not found"))
            }
            MetError::Unauthorized(msg) => Self::unauthorized(msg),
            MetError::Forbidden(msg) => Self::forbidden(msg),
            MetError::Validation(msg) => Self::unprocessable(msg),
            MetError::Config(msg) => Self::internal(format!("configuration error: {msg}")),
            MetError::Serialization(e) => Self::internal(format!("serialization error: {e}")),
            MetError::Yaml(e) => Self::internal(format!("yaml error: {e}")),
            MetError::Io(e) => Self::internal(format!("io error: {e}")),
            MetError::UuidParse(e) => Self::bad_request(format!("invalid UUID: {e}")),
            MetError::Internal(msg) => Self::internal(msg),
            MetError::Database(e) => {
                tracing::error!(error = %e, "database error");
                Self::internal("database error")
            }
        }
    }
}

impl From<met_store::StoreError> for ApiError {
    fn from(err: met_store::StoreError) -> Self {
        let met_err: MetError = err.into();
        met_err.into()
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}: {}", self.status, self.code, self.message)
    }
}

impl std::error::Error for ApiError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_constructors() {
        let err = ApiError::not_found("Pipeline not found");
        assert_eq!(err.status, StatusCode::NOT_FOUND);
        assert_eq!(err.code, "not_found");

        let err = ApiError::unauthorized("Invalid token");
        assert_eq!(err.status, StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn test_from_met_error() {
        let met_err = MetError::not_found("pipeline", "pipe_123");
        let api_err: ApiError = met_err.into();
        assert_eq!(api_err.status, StatusCode::NOT_FOUND);
    }

    #[test]
    fn test_with_request_id() {
        let err = ApiError::internal("something went wrong")
            .with_request_id("req_abc123");
        assert_eq!(err.request_id, Some("req_abc123".to_string()));
    }
}
