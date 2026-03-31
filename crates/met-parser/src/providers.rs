//! Workflow providers for fetching reusable workflow definitions.
//!
//! This module provides implementations for fetching workflows from various sources:
//! - `DatabaseWorkflowProvider`: Fetches global workflows from PostgreSQL (requires `database` feature)
//! - `GitWorkflowProvider`: Fetches project workflows from git repositories

#[cfg(feature = "database")]
mod database;
mod git;

#[cfg(feature = "database")]
pub use database::DatabaseWorkflowProvider;
pub use git::GitWorkflowProvider;
