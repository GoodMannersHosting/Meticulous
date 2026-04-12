//! Custom Axum extractors for the API.
//!
//! This module provides extractors for:
//! - Authentication (JWT and API tokens)
//! - Pagination (cursor-based)
//! - Request ID (for tracing)

pub mod app_installation;
pub mod auth;
pub mod pagination;
pub mod request_id;
pub mod session_or_app;

pub use app_installation::AppInstallationAuth;
pub use auth::{Auth, CurrentUser, OptionalAuth};
pub use pagination::{PaginatedResponse, Pagination, PaginationMeta};
pub use request_id::RequestId;
pub use session_or_app::SessionOrAppAuth;
