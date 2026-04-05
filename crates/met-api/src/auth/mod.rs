//! Authentication and authorization modules.
//!
//! This module provides:
//! - JWT token validation for user sessions
//! - JWT token creation for login
//! - API token validation for programmatic access
//! - RBAC (Role-Based Access Control) enforcement
//! - Password hashing and verification

pub mod api_token;
pub mod jwt;
pub mod password;
pub mod rbac;

pub use api_token::{ApiTokenValidator, generate_token, hash_token};
pub use jwt::{JwtValidator, create_jwt};
pub use password::{PasswordError, hash_password, verify_password};
pub use rbac::{ApiRole, Authorized, authorize, authorize_project};
