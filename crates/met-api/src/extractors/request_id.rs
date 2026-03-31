//! Request ID extractor for tracing.
//!
//! Extracts the request ID from the `X-Request-Id` header, which is set by
//! the request ID middleware. This allows handlers to include the request ID
//! in logs and error responses.

use axum::{
    extract::FromRequestParts,
    http::request::Parts,
};
use uuid::Uuid;

/// Header name for request IDs.
pub const REQUEST_ID_HEADER: &str = "x-request-id";

/// Request ID extractor.
///
/// The request ID is a UUIDv7 that uniquely identifies each request.
/// It's set by the request ID middleware and can be used for:
/// - Correlating logs across services
/// - Including in error responses
/// - Debugging production issues
#[derive(Debug, Clone)]
pub struct RequestId(pub Uuid);

impl RequestId {
    /// Generate a new request ID (UUIDv7 for time-sortability).
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    /// Get the request ID as a string.
    pub fn as_str(&self) -> String {
        self.0.to_string()
    }
}

impl Default for RequestId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for RequestId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl<S> FromRequestParts<S> for RequestId
where
    S: Send + Sync,
{
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let request_id = parts
            .headers
            .get(REQUEST_ID_HEADER)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| Uuid::parse_str(s).ok())
            .map(RequestId)
            .unwrap_or_else(RequestId::new);

        Ok(request_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_id_generation() {
        let id1 = RequestId::new();
        let id2 = RequestId::new();
        assert_ne!(id1.0, id2.0);
    }

    #[test]
    fn test_request_id_display() {
        let id = RequestId::new();
        let s = id.to_string();
        assert_eq!(s.len(), 36); // UUID string length
    }
}
