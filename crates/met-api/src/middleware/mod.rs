//! HTTP middleware stack for the API.
//!
//! Middleware is applied in order (outermost first):
//! 1. Request ID - assigns unique ID to each request
//! 2. Logging - structured request/response logging
//! 3. CORS - Cross-Origin Resource Sharing
//! 4. Rate limiting - token bucket rate limiter
//! 5. Compression - gzip response compression

pub mod cors;
pub mod logging;
pub mod rate_limit;

pub use cors::cors_layer;
pub use logging::logging_layer;
pub use rate_limit::rate_limit_layer;
