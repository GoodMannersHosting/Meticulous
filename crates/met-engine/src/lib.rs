//! Pipeline execution engine for Meticulous CI/CD.
//!
//! This crate provides the core execution engine including:
//!
//! - **DAG Executor**: Walks the job dependency graph in topological order
//! - **Job Scheduler**: Dispatches jobs to agents via NATS JetStream
//! - **Caching Layer**: Skips jobs when cache keys match
//! - **Artifact Passing**: Transfers build outputs between jobs
//! - **Conditional Execution**: CEL-based conditions for job/step gating
//! - **Retry/Timeout**: Configurable retry policies and execution timeouts
//! - **Event Broadcasting**: Real-time events for UI and integrations
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                        Engine                                │
//! │  ┌─────────────┐   ┌─────────────┐   ┌─────────────┐       │
//! │  │  Executor   │──▶│  Scheduler  │──▶│    NATS     │       │
//! │  └─────────────┘   └─────────────┘   └─────────────┘       │
//! │        │                  │                                  │
//! │        ▼                  ▼                                  │
//! │  ┌─────────────┐   ┌─────────────┐   ┌─────────────┐       │
//! │  │    CEL      │   │    Cache    │   │  Artifacts  │       │
//! │  └─────────────┘   └─────────────┘   └─────────────┘       │
//! │        │                  │                  │               │
//! │        └──────────────────┼──────────────────┘               │
//! │                           ▼                                  │
//! │                    ┌─────────────┐                          │
//! │                    │   Events    │                          │
//! │                    └─────────────┘                          │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Usage
//!
//! ```ignore
//! use met_engine::{Engine, EngineConfig};
//! use met_parser::PipelineParser;
//!
//! // Parse a pipeline definition
//! let parser = PipelineParser::new();
//! let pipeline_ir = parser.parse_file("pipeline.yaml").await?;
//!
//! // Create the engine
//! let engine = Engine::new(config).await?;
//!
//! // Execute a run
//! let result = engine.execute(pipeline_ir, "manual").await?;
//! ```
#![allow(
    clippy::collapsible_if,
    clippy::question_mark,
    clippy::too_many_arguments,
    dead_code,
    unused_imports,
    unused_variables,
)]

pub mod affinity;
pub mod artifacts;
pub mod cache;
pub mod cel;
pub mod context;
pub mod error;
pub mod events;
pub mod executor;
pub mod log_streaming;
pub mod persistence;
pub mod retry;
pub mod scheduler;
pub mod secrets;
pub mod state;
pub mod workspace_snapshots;

pub use artifacts::{ArtifactBackend, ArtifactManager, ArtifactMetadata, MemoryArtifactStore};
pub use cache::{
    CacheBackend, CacheKey, CacheLookupResult, CacheManager, MemoryCache, ObjectStoreCache,
};
pub use context::{ArtifactRef, CacheHit, ExecutionContext, ResolvedSecret};
pub use error::{EngineError, Result};
pub use events::{EventBroadcaster, subjects as event_subjects};
pub use executor::{ExecutionResult, Executor, ExecutorConfig, RunStartKind, topological_order};
pub use log_streaming::{LogChunk, LogStreamRelay};
pub use persistence::{
    JobRunSourceRefs, MemoryRunPersistence, PostgresRunPersistence, RunPersistence,
};
pub use retry::{RetryExecutor, RetryPolicy, RetryState};
pub use scheduler::{
    JobCompletionNotification, JobDispatchMessage, Scheduler, SchedulerConfig, StepDispatch,
};

mod output_crypto;
pub use secrets::SecretEncryption;
pub use state::{JobState, RunState, StepState};
pub use workspace_snapshots::{
    WorkspaceSnapshotConfig, WorkspaceSnapshotPresigner, WorkspaceSnapshotRecord,
    snapshot_object_key_for_job_run, workspace_snapshot_predecessor,
};

use async_nats::jetstream::Context as JetStreamContext;
use met_core::ids::{OrganizationId, RunId};
use met_parser::PipelineIR;
use met_secrets::BuiltinStoredCrypto;
use sqlx::PgPool;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{info, instrument};

/// Engine configuration.
#[derive(Clone)]
pub struct EngineConfig {
    /// NATS connection URL.
    pub nats_url: String,
    /// Path to NATS `.creds` (JWT auth); `None` for anonymous brokers (development only).
    pub nats_credentials_file: Option<PathBuf>,
    /// Database connection pool.
    pub pool: PgPool,
    /// Executor configuration.
    pub executor: ExecutorConfig,
    /// Scheduler configuration.
    pub scheduler: SchedulerConfig,
    /// Object storage prefix for caching.
    pub cache_prefix: String,
    /// Base64 master key for `builtin_secrets` rows (env `MET_BUILTIN_SECRETS_MASTER_KEY`).
    pub builtin_secrets_master_key: Option<String>,
    /// Optional key id label stored with ciphertext (default `v1`).
    pub builtin_secrets_key_id: Option<String>,
    /// Passive workspace snapshots for `share_workspace` jobs (requires presigner when enabled).
    pub workspace_snapshots: WorkspaceSnapshotConfig,
    pub workspace_snapshot_presigner: Option<Arc<dyn WorkspaceSnapshotPresigner>>,
}

impl std::fmt::Debug for EngineConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EngineConfig")
            .field("nats_url", &self.nats_url)
            .field("nats_credentials_file", &self.nats_credentials_file)
            .field("pool", &"<PgPool>")
            .field("executor", &self.executor)
            .field("scheduler", &self.scheduler)
            .field("cache_prefix", &self.cache_prefix)
            .field(
                "builtin_secrets_master_key",
                &self.builtin_secrets_master_key.is_some(),
            )
            .field("builtin_secrets_key_id", &self.builtin_secrets_key_id)
            .field("workspace_snapshots", &self.workspace_snapshots)
            .field(
                "workspace_snapshot_presigner",
                &self.workspace_snapshot_presigner.is_some(),
            )
            .finish()
    }
}

async fn connect_nats_client(
    url: &str,
    creds: Option<&Path>,
) -> std::result::Result<async_nats::Client, String> {
    match creds {
        Some(path) => {
            let opts = async_nats::ConnectOptions::with_credentials_file(path)
                .await
                .map_err(|e| e.to_string())?;
            opts.connect(url).await.map_err(|e| e.to_string())
        }
        None => async_nats::connect(url).await.map_err(|e| e.to_string()),
    }
}

/// The main pipeline execution engine.
pub struct Engine {
    executor: Arc<Executor<MemoryCache>>,
    scheduler: Arc<Scheduler>,
    events: Arc<EventBroadcaster>,
    cache: Arc<CacheManager<MemoryCache>>,
    pool: PgPool,
}

impl Engine {
    /// Create a new engine instance.
    #[instrument(skip(config))]
    pub async fn new(config: EngineConfig) -> Result<Self> {
        info!(nats_url = %config.nats_url, "initializing engine");

        let client = connect_nats_client(&config.nats_url, config.nats_credentials_file.as_deref())
            .await
            .map_err(|e| EngineError::Nats(format!("Failed to connect to NATS: {e}")))?;

        let jetstream = async_nats::jetstream::new(client);

        let events = Arc::new(EventBroadcaster::new(jetstream.clone()));
        events.ensure_stream().await?;

        let cache = Arc::new(CacheManager::new(MemoryCache::new()));

        let persistence: Arc<dyn RunPersistence> =
            Arc::new(PostgresRunPersistence::new(config.pool.clone()));

        let scheduler = Arc::new(Scheduler::with_workspace_snapshots(
            jetstream.clone(),
            config.pool.clone(),
            events.clone(),
            config.scheduler,
            persistence.clone(),
            config.workspace_snapshots.clone(),
            config.workspace_snapshot_presigner.clone(),
        ));

        let builtin_stored_crypto = Self::make_builtin_crypto(
            config.builtin_secrets_master_key.as_deref(),
            config.builtin_secrets_key_id.as_deref(),
        );

        let executor = Arc::new(Executor::new(
            scheduler.clone(),
            cache.clone(),
            events.clone(),
            config.pool.clone(),
            config.executor,
            builtin_stored_crypto,
            persistence,
            jetstream,
        ));

        info!("engine initialized successfully");

        Ok(Self {
            executor,
            scheduler,
            events,
            cache,
            pool: config.pool,
        })
    }

    fn make_builtin_crypto(
        master: Option<&str>,
        kid: Option<&str>,
    ) -> Option<Arc<BuiltinStoredCrypto>> {
        let m = master?.trim();
        if m.is_empty() {
            return None;
        }
        BuiltinStoredCrypto::from_master_key_b64(m, kid)
            .ok()
            .map(Arc::new)
    }

    /// Create an engine with a custom cache backend.
    pub async fn with_cache<C: CacheBackend + 'static>(
        nats_url: &str,
        pool: PgPool,
        cache_backend: C,
        executor_config: ExecutorConfig,
        scheduler_config: SchedulerConfig,
    ) -> Result<EngineWithCache<C>> {
        Self::with_cache_and_creds(
            nats_url,
            None,
            pool,
            cache_backend,
            executor_config,
            scheduler_config,
        )
        .await
    }

    /// Like [`Self::with_cache`], but with optional NATS `.creds` for JWT clusters.
    pub async fn with_cache_and_creds<C: CacheBackend + 'static>(
        nats_url: &str,
        nats_credentials_file: Option<&Path>,
        pool: PgPool,
        cache_backend: C,
        executor_config: ExecutorConfig,
        scheduler_config: SchedulerConfig,
    ) -> Result<EngineWithCache<C>> {
        let client = connect_nats_client(nats_url, nats_credentials_file)
            .await
            .map_err(|e| EngineError::Nats(format!("Failed to connect to NATS: {e}")))?;

        let jetstream = async_nats::jetstream::new(client);

        let events = Arc::new(EventBroadcaster::new(jetstream.clone()));
        events.ensure_stream().await?;

        let cache = Arc::new(CacheManager::new(cache_backend));

        let persistence: Arc<dyn RunPersistence> =
            Arc::new(PostgresRunPersistence::new(pool.clone()));

        let scheduler = Arc::new(Scheduler::with_workspace_snapshots(
            jetstream.clone(),
            pool.clone(),
            events.clone(),
            scheduler_config,
            persistence.clone(),
            WorkspaceSnapshotConfig::default(),
            None,
        ));

        let builtin_stored_crypto = Self::make_builtin_crypto(None, None);

        let executor = Arc::new(Executor::new(
            scheduler.clone(),
            cache.clone(),
            events.clone(),
            pool.clone(),
            executor_config,
            builtin_stored_crypto,
            persistence,
            jetstream,
        ));

        Ok(EngineWithCache {
            executor,
            scheduler,
            events,
            cache,
            pool,
        })
    }

    /// Execute a pipeline run.
    #[instrument(skip(self, pipeline), fields(pipeline = %pipeline.name))]
    pub async fn execute(
        &self,
        org_id: OrganizationId,
        pipeline: PipelineIR,
        triggered_by: &str,
    ) -> Result<ExecutionResult> {
        let run_id = RunId::new();
        let ctx = ExecutionContext::new(run_id, org_id, pipeline, triggered_by);

        if let Err(e) = self
            .events
            .run_queued(
                run_id,
                ctx.pipeline_id(),
                triggered_by,
                Some(ctx.trace_id()),
            )
            .await
        {
            tracing::warn!(error = %e, "Failed to broadcast run queued event");
        }

        self.executor.execute(ctx, RunStartKind::New).await
    }

    /// Execute using a `runs` row that already exists (for example created by the REST API).
    #[instrument(skip(self, pipeline), fields(pipeline = %pipeline.name, run_id = %run_id))]
    pub async fn execute_with_existing_run(
        &self,
        run_id: RunId,
        org_id: OrganizationId,
        pipeline: PipelineIR,
        triggered_by: &str,
    ) -> Result<ExecutionResult> {
        let ctx = ExecutionContext::new(run_id, org_id, pipeline, triggered_by);

        if let Err(e) = self
            .events
            .run_queued(
                run_id,
                ctx.pipeline_id(),
                triggered_by,
                Some(ctx.trace_id()),
            )
            .await
        {
            tracing::warn!(error = %e, "Failed to broadcast run queued event");
        }

        self.executor.execute(ctx, RunStartKind::Existing).await
    }

    /// Get a handle to cancel a running pipeline.
    pub fn cancel_handle(&self, run_state: &RunState) -> CancelHandle {
        CancelHandle {
            run_state: run_state.clone(),
        }
    }

    /// Get the event broadcaster for subscribing to events.
    pub fn events(&self) -> &Arc<EventBroadcaster> {
        &self.events
    }

    /// Get the scheduler for external job management.
    pub fn scheduler(&self) -> &Arc<Scheduler> {
        &self.scheduler
    }
}

/// Engine with custom cache backend.
pub struct EngineWithCache<C: CacheBackend> {
    executor: Arc<Executor<C>>,
    scheduler: Arc<Scheduler>,
    events: Arc<EventBroadcaster>,
    cache: Arc<CacheManager<C>>,
    pool: PgPool,
}

impl<C: CacheBackend> EngineWithCache<C> {
    /// Execute a pipeline run.
    pub async fn execute(
        &self,
        org_id: OrganizationId,
        pipeline: PipelineIR,
        triggered_by: &str,
    ) -> Result<ExecutionResult> {
        let run_id = RunId::new();
        let ctx = ExecutionContext::new(run_id, org_id, pipeline, triggered_by);
        self.executor.execute(ctx, RunStartKind::New).await
    }

    /// Execute with a pre-created `runs` row.
    pub async fn execute_with_existing_run(
        &self,
        run_id: RunId,
        org_id: OrganizationId,
        pipeline: PipelineIR,
        triggered_by: &str,
    ) -> Result<ExecutionResult> {
        let ctx = ExecutionContext::new(run_id, org_id, pipeline, triggered_by);
        self.executor.execute(ctx, RunStartKind::Existing).await
    }
}

/// Handle to cancel a running pipeline.
#[derive(Clone)]
pub struct CancelHandle {
    run_state: RunState,
}

impl CancelHandle {
    /// Request cancellation of the run.
    pub async fn cancel(&self) {
        self.run_state.request_cancellation().await;
    }
}

/// Listener for job completion messages from agents.
pub struct CompletionListener {
    jetstream: JetStreamContext,
}

impl CompletionListener {
    /// Create a new completion listener.
    pub fn new(jetstream: JetStreamContext) -> Self {
        Self { jetstream }
    }

    /// Start listening for completion messages.
    pub async fn listen(
        &self,
        org_id: OrganizationId,
        mut callback: impl FnMut(JobCompletionNotification) + Send + 'static,
    ) -> Result<()> {
        use async_nats::jetstream::consumer::pull::Config;
        use futures::StreamExt;

        let stream = self
            .jetstream
            .get_stream("COMPLETIONS")
            .await
            .map_err(|e| EngineError::Nats(e.to_string()))?;

        let consumer_name = format!("completions-{}", org_id.as_uuid());
        let filter = format!("met.completions.{}", org_id.as_uuid());

        let config = Config {
            name: Some(consumer_name.clone()),
            durable_name: Some(consumer_name),
            filter_subject: filter,
            ..Default::default()
        };

        let consumer = stream
            .create_consumer(config)
            .await
            .map_err(|e| EngineError::Nats(e.to_string()))?;

        let mut messages = consumer
            .messages()
            .await
            .map_err(|e| EngineError::Nats(e.to_string()))?;

        while let Some(msg) = messages.next().await {
            match msg {
                Ok(msg) => {
                    if let Ok(notification) = parse_completion_message(&msg.payload) {
                        callback(notification);
                    }
                    let _ = msg.ack().await;
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Error receiving completion message");
                }
            }
        }

        Ok(())
    }
}

pub(crate) fn parse_completion_message(payload: &[u8]) -> Result<JobCompletionNotification> {
    use met_proto::controller::v1::JobCompletion;
    use prost::Message;

    let proto = JobCompletion::decode(payload)
        .map_err(|e| EngineError::internal(format!("Failed to decode completion: {e}")))?;

    let job_run_id = proto
        .job_run_id
        .parse()
        .map_err(|e| EngineError::internal(format!("Invalid job_run_id: {e}")))?;
    let run_id = proto
        .run_id
        .parse()
        .map_err(|e| EngineError::internal(format!("Invalid run_id: {e}")))?;
    let agent_id = proto
        .agent_id
        .parse()
        .map_err(|e| EngineError::internal(format!("Invalid agent_id: {e}")))?;

    let success = proto.status() == met_proto::common::v1::RunStatus::Succeeded;

    Ok(JobCompletionNotification {
        job_run_id,
        run_id,
        agent_id,
        success,
        exit_code: proto.exit_code,
        error_message: if proto.error_message.is_empty() {
            None
        } else {
            Some(proto.error_message)
        },
        duration_ms: proto.duration_ms as u64,
        outputs: proto.outputs.into_iter().collect(),
        workflow_outputs: proto.workflow_outputs,
        workspace_snapshot_result: proto.workspace_snapshot_result,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_topological_order_empty() {
        let pipeline = PipelineIR {
            id: met_core::ids::PipelineId::new(),
            name: "empty".to_string(),
            source_file: None,
            project_id: None,
            triggers: Vec::new(),
            variables: Default::default(),
            secret_refs: Default::default(),
            jobs: Vec::new(),
            default_pool_selector: None,
            expose_workflow_secret_outputs: false,
        };

        let order = topological_order(&pipeline).unwrap();
        assert!(order.is_empty());
    }
}
