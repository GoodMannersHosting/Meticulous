//! Request/response logging middleware using tracing.
//!
//! Logs structured information about each request:
//! - Request: method, path, request ID
//! - Response: status code, latency
//!
//! Security: Sensitive query parameters (tokens, keys) are redacted from logs.

use axum::http::Request;
use tower_http::trace::{DefaultOnResponse, MakeSpan, TraceLayer};
use tracing::{Level, Span};

/// Query parameter names that should be redacted from logs.
const SENSITIVE_PARAMS: &[&str] = &["token", "api_key", "apikey", "key", "secret", "password"];

/// Custom span maker that sanitizes sensitive query parameters from URIs.
#[derive(Clone, Debug)]
pub struct SanitizedMakeSpan {
    level: Level,
}

impl SanitizedMakeSpan {
    pub fn new() -> Self {
        Self { level: Level::DEBUG }
    }

    pub fn level(mut self, level: Level) -> Self {
        self.level = level;
        self
    }
}

impl Default for SanitizedMakeSpan {
    fn default() -> Self {
        Self::new()
    }
}

impl<B> MakeSpan<B> for SanitizedMakeSpan {
    fn make_span(&mut self, request: &Request<B>) -> Span {
        let uri = sanitize_uri(request.uri());

        macro_rules! make_span {
            ($level:expr) => {
                tracing::span!(
                    $level,
                    "request",
                    method = %request.method(),
                    uri = %uri,
                    version = ?request.version(),
                )
            };
        }

        match self.level {
            Level::ERROR => make_span!(Level::ERROR),
            Level::WARN => make_span!(Level::WARN),
            Level::INFO => make_span!(Level::INFO),
            Level::DEBUG => make_span!(Level::DEBUG),
            Level::TRACE => make_span!(Level::TRACE),
        }
    }
}

/// Sanitize a URI by redacting sensitive query parameters.
fn sanitize_uri(uri: &axum::http::Uri) -> String {
    let path = uri.path();
    
    let Some(query) = uri.query() else {
        return path.to_string();
    };

    let sanitized_params: Vec<String> = query
        .split('&')
        .map(|param| {
            if let Some((key, _)) = param.split_once('=') {
                let key_lower = key.to_lowercase();
                if SENSITIVE_PARAMS.iter().any(|&s| key_lower.contains(s)) {
                    format!("{}=[REDACTED]", key)
                } else {
                    param.to_string()
                }
            } else {
                param.to_string()
            }
        })
        .collect();

    if sanitized_params.is_empty() {
        path.to_string()
    } else {
        format!("{}?{}", path, sanitized_params.join("&"))
    }
}

/// Create a logging layer for request/response tracing.
/// 
/// Uses custom span maker that redacts sensitive query parameters from URIs.
pub fn logging_layer() -> TraceLayer<
    tower_http::classify::SharedClassifier<tower_http::classify::ServerErrorsAsFailures>,
    SanitizedMakeSpan,
> {
    TraceLayer::new_for_http()
        .make_span_with(SanitizedMakeSpan::new().level(Level::INFO))
        .on_response(DefaultOnResponse::new().level(Level::INFO))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_logging_layer_creation() {
        let _layer = logging_layer();
    }

    #[test]
    fn test_sanitize_uri_no_query() {
        let uri: axum::http::Uri = "/api/v1/runs".parse().unwrap();
        assert_eq!(sanitize_uri(&uri), "/api/v1/runs");
    }

    #[test]
    fn test_sanitize_uri_with_token() {
        let uri: axum::http::Uri = "/ws?token=eyJhbGciOiJIUzI1NiJ9.secret".parse().unwrap();
        assert_eq!(sanitize_uri(&uri), "/ws?token=[REDACTED]");
    }

    #[test]
    fn test_sanitize_uri_with_multiple_params() {
        let uri: axum::http::Uri = "/ws?follow=true&token=secret&from_line=0".parse().unwrap();
        assert_eq!(sanitize_uri(&uri), "/ws?follow=true&token=[REDACTED]&from_line=0");
    }

    #[test]
    fn test_sanitize_uri_with_api_key() {
        let uri: axum::http::Uri = "/api?api_key=abc123&format=json".parse().unwrap();
        assert_eq!(sanitize_uri(&uri), "/api?api_key=[REDACTED]&format=json");
    }

    #[test]
    fn test_sanitize_uri_case_insensitive() {
        let uri: axum::http::Uri = "/api?TOKEN=secret&ApiKey=abc".parse().unwrap();
        assert_eq!(sanitize_uri(&uri), "/api?TOKEN=[REDACTED]&ApiKey=[REDACTED]");
    }

    #[test]
    fn test_sanitize_uri_preserves_safe_params() {
        let uri: axum::http::Uri = "/runs?page=1&per_page=10&status=running".parse().unwrap();
        assert_eq!(sanitize_uri(&uri), "/runs?page=1&per_page=10&status=running");
    }
}
