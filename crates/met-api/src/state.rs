//! Application state shared across all handlers.
//!
//! `AppState` is passed to Axum handlers via the `State` extractor and contains
//! shared resources like database pools, configuration, and service clients.

use crate::config::ApiConfig;
use met_secrets::BuiltinStoredCrypto;
use met_store::PgPool;
use std::sync::Arc;

/// Shared application state for all API handlers.
///
/// This struct is cloned for each request, so all fields use `Arc` or are `Clone`.
#[derive(Clone, Debug)]
pub struct AppState {
    /// PostgreSQL connection pool.
    pub db: PgPool,

    /// API configuration.
    pub config: Arc<ApiConfig>,

    /// Encrypts stored secret payloads (same master key as engine/controller).
    pub stored_secret_crypto: Option<Arc<BuiltinStoredCrypto>>,
}

impl AppState {
    /// Create a new `AppState` instance.
    pub fn new(db: PgPool, config: ApiConfig, stored_secret_crypto: Option<Arc<BuiltinStoredCrypto>>) -> Self {
        Self {
            db,
            config: Arc::new(config),
            stored_secret_crypto,
        }
    }

    /// Get a reference to the database pool.
    pub fn db(&self) -> &PgPool {
        &self.db
    }

    /// Get a reference to the API configuration.
    pub fn config(&self) -> &ApiConfig {
        &self.config
    }
}
