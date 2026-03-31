//! CORS (Cross-Origin Resource Sharing) middleware configuration.

use crate::config::ApiConfig;
use http::{HeaderName, HeaderValue, Method};
use tower_http::cors::{Any, CorsLayer};

/// Custom header name for request IDs.
const X_REQUEST_ID: HeaderName = HeaderName::from_static("x-request-id");

/// Create a CORS layer based on configuration.
///
/// In development mode (`cors_allow_any = true`), allows any origin.
/// In production, only allows configured origins.
pub fn cors_layer(config: &ApiConfig) -> CorsLayer {
    let layer = CorsLayer::new()
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([
            http::header::AUTHORIZATION,
            http::header::CONTENT_TYPE,
            http::header::ACCEPT,
            http::header::ORIGIN,
            X_REQUEST_ID,
        ])
        .expose_headers([X_REQUEST_ID])
        .max_age(std::time::Duration::from_secs(3600));

    if config.cors_allow_any {
        layer.allow_origin(Any)
    } else if config.cors_origins.is_empty() {
        layer
    } else {
        let origins: Vec<HeaderValue> = config
            .cors_origins
            .iter()
            .filter_map(|o| o.parse().ok())
            .collect();
        layer.allow_origin(origins)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cors_layer_creation() {
        let config = ApiConfig::default();
        let _layer = cors_layer(&config);
    }

    #[test]
    fn test_cors_allow_any() {
        let mut config = ApiConfig::default();
        config.cors_allow_any = true;
        let _layer = cors_layer(&config);
    }
}
