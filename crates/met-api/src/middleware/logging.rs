//! Request/response logging middleware using tracing.
//!
//! Logs structured information about each request:
//! - Request: method, path, request ID
//! - Response: status code, latency

use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer};
use tracing::Level;

/// Create a logging layer for request/response tracing.
pub fn logging_layer() -> TraceLayer<
    tower_http::classify::SharedClassifier<tower_http::classify::ServerErrorsAsFailures>,
> {
    TraceLayer::new_for_http()
        .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
        .on_response(DefaultOnResponse::new().level(Level::INFO))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_logging_layer_creation() {
        let _layer = logging_layer();
    }
}
