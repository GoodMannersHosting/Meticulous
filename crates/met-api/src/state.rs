//! Application state shared across all handlers.
//!
//! `AppState` is passed to Axum handlers via the `State` extractor and contains
//! shared resources like database pools, configuration, and service clients.

use crate::config::ApiConfig;
use crate::middleware::CredentialRateLimiter;
use met_controller::nats::NatsDispatcher;
use met_engine::Engine;
use met_secrets::BuiltinStoredCrypto;
use met_objstore::S3ObjectStore;
use met_store::PgPool;
use std::sync::Arc;
use tokio::sync::Semaphore;

/// Non-secret object storage settings for admin diagnostics.
#[derive(Debug, Clone)]
pub struct ObjectStoragePublicConfig {
    pub endpoint: String,
    pub bucket: String,
    pub path_style: bool,
}

/// Shared state for all API handlers.
///
/// This struct is cloned for each request, so all fields use `Arc` or are `Clone`.
#[derive(Clone)]
pub struct AppState {
    /// PostgreSQL connection pool.
    pub db: PgPool,

    /// API configuration.
    pub config: Arc<ApiConfig>,

    /// Encrypts stored secret payloads (same master key as engine/controller).
    pub stored_secret_crypto: Option<Arc<BuiltinStoredCrypto>>,

    /// Shared pipeline engine (NATS + Postgres); unset if initialization failed.
    pub engine: Option<Arc<Engine>>,

    /// When [`Self::engine`] is `None`, the error from `Engine::new` (for 503 bodies and ops).
    pub engine_init_error: Option<String>,

    /// Limits concurrent in-process pipeline runs started from the API.
    pub engine_run_semaphore: Arc<Semaphore>,

    /// Second NATS connection for admin/ops (e.g. JOBS_DLQ preview). Does not receive advisories.
    pub nats_ops: Option<Arc<NatsDispatcher>>,

    /// S3-compatible settings (no credentials) for Platform Health.
    pub object_storage: ObjectStoragePublicConfig,

    /// Object store client when initialization succeeded.
    pub object_store: Option<Arc<S3ObjectStore>>,

    /// Per-credential dual-window rate limits (user JWT/API token vs app JWT), from org policy.
    pub credential_rate_limit: Option<Arc<CredentialRateLimiter>>,
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppState")
            .field("engine_initialized", &self.engine.is_some())
            .field("nats_ops", &self.nats_ops.is_some())
            .field("object_store", &self.object_store.is_some())
            .field("credential_rate_limit", &self.credential_rate_limit.is_some())
            .finish_non_exhaustive()
    }
}

impl AppState {
    /// Create a new `AppState` instance.
    pub fn new(
        db: PgPool,
        config: ApiConfig,
        stored_secret_crypto: Option<Arc<BuiltinStoredCrypto>>,
        engine: Option<Arc<Engine>>,
        engine_init_error: Option<String>,
        max_concurrent_engine_runs: usize,
        nats_ops: Option<Arc<NatsDispatcher>>,
        object_storage: ObjectStoragePublicConfig,
        object_store: Option<Arc<S3ObjectStore>>,
    ) -> Self {
        let permits = max_concurrent_engine_runs.max(1);
        Self {
            db,
            config: Arc::new(config),
            stored_secret_crypto,
            engine,
            engine_init_error,
            engine_run_semaphore: Arc::new(Semaphore::new(permits)),
            nats_ops,
            object_storage,
            object_store,
            credential_rate_limit: Some(Arc::new(CredentialRateLimiter::new())),
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
