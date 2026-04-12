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
pub mod github_scm;
pub mod middleware;
pub mod openapi;
pub mod pipeline_execution;
pub mod project_access;
pub mod routes;
pub mod stored_secret_policy;
pub mod scheduling_hints;
pub mod state;
pub mod trigger_sync;
pub mod workflow_diagnostics;

pub use config::ApiConfig;
pub use error::{ApiError, ApiResult};
pub use openapi::ApiDoc;
pub use state::AppState;

/// Package version for health endpoint.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
