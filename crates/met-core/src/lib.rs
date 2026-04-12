//! Core types, error handling, and configuration for Meticulous CI/CD.
//!
//! This crate provides the foundational types used across all Meticulous components:
//! - Error types and result aliases
//! - Typed ID wrappers for compile-time safety
//! - Domain models (Organization, Project, Pipeline, etc.)
//! - Configuration loading with layered overrides
//! - Event envelopes for NATS messaging

pub mod config;
pub mod error;
pub mod events;
pub mod fuzzy;
pub mod ids;
pub mod models;
pub mod output_ipc;
pub mod redact;
pub mod tokens;

pub use config::MetConfig;
pub use error::{MetError, Result};
pub use ids::*;
pub use tokens::hash_join_token;
