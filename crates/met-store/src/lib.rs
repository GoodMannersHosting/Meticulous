//! Database layer for Meticulous CI/CD.
//!
//! This crate provides PostgreSQL database access using `sqlx` with compile-time
//! checked queries, migrations, and repository patterns for each entity type.
#![allow(clippy::too_many_arguments)]

pub mod error;
pub mod pool;
pub mod repos;

pub use error::StoreError;
pub use pool::{PoolConfig, create_pool, run_migrations};

/// Re-export sqlx types for convenience.
pub use sqlx::PgPool;
