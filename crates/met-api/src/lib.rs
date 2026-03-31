//! Meticulous REST API server library.
//!
//! This crate provides the HTTP API for managing pipelines, runs, agents,
//! and other Meticulous resources. It includes:
//!
//! - Authentication via JWT and API tokens
//! - RBAC-based authorization
//! - Cursor-based pagination
//! - Structured error responses with request ID tracing
//! - Rate limiting and CORS middleware

pub mod auth;
pub mod config;
pub mod error;
pub mod extractors;
pub mod middleware;
pub mod routes;
pub mod state;

pub use config::ApiConfig;
pub use error::{ApiError, ApiResult};
pub use state::AppState;

/// Package version for health endpoint.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
