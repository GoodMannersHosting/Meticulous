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

pub mod artifacts;
pub mod cache;
pub mod cel;
pub mod context;
pub mod error;
pub mod events;
pub mod executor;
pub mod scheduler;
pub mod state;

pub use artifacts::{ArtifactBackend, ArtifactManager, ArtifactMetadata, MemoryArtifactStore};
pub use cache::{CacheBackend, CacheKey, CacheLookupResult, CacheManager, MemoryCache, ObjectStoreCache};
pub use context::{ArtifactRef, CacheHit, ExecutionContext, ResolvedSecret};
pub use error::{EngineError, Result};
pub use events::{subjects as event_subjects, EventBroadcaster};
pub use executor::{topological_order, ExecutionResult, Executor, ExecutorConfig};
pub use scheduler::{JobCompletionNotification, JobDispatchMessage, Scheduler, SchedulerConfig, StepDispatch};
pub use state::{JobState, RunState, StepState};

use async_nats::jetstream::Context as JetStreamContext;
use met_core::ids::{OrganizationId, RunId};
use met_parser::PipelineIR;
use sqlx::PgPool;
use std::sync::Arc;
use tracing::{info, instrument};

/// Engine configuration.
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// NATS connection URL.
    pub nats_url: String,
    /// Database connection pool.
    pub pool: PgPool,
    /// Executor configuration.
    pub executor: ExecutorConfig,
    /// Scheduler configuration.
    pub scheduler: SchedulerConfig,
    /// Object storage prefix for caching.
    pub cache_prefix: String,
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

        let client = async_nats::connect(&config.nats_url)
            .await
            .map_err(|e| EngineError::Nats(format!("Failed to connect to NATS: {e}")))?;

        let jetstream = async_nats::jetstream::new(client);

        let events = Arc::new(EventBroadcaster::new(jetstream.clone()));
        events.ensure_stream().await?;

        let cache = Arc::new(CacheManager::new(MemoryCache::new()));

        let scheduler = Arc::new(Scheduler::new(
            jetstream,
            config.pool.clone(),
            events.clone(),
            config.scheduler,
        ));

        let executor = Arc::new(Executor::new(
            scheduler.clone(),
            cache.clone(),
            events.clone(),
            config.pool.clone(),
            config.executor,
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

    /// Create an engine with a custom cache backend.
    pub async fn with_cache<C: CacheBackend + 'static>(
        nats_url: &str,
        pool: PgPool,
        cache_backend: C,
        executor_config: ExecutorConfig,
        scheduler_config: SchedulerConfig,
    ) -> Result<EngineWithCache<C>> {
        let client = async_nats::connect(nats_url)
            .await
            .map_err(|e| EngineError::Nats(format!("Failed to connect to NATS: {e}")))?;

        let jetstream = async_nats::jetstream::new(client);

        let events = Arc::new(EventBroadcaster::new(jetstream.clone()));
        events.ensure_stream().await?;

        let cache = Arc::new(CacheManager::new(cache_backend));

        let scheduler = Arc::new(Scheduler::new(
            jetstream,
            pool.clone(),
            events.clone(),
            scheduler_config,
        ));

        let executor = Arc::new(Executor::new(
            scheduler.clone(),
            cache.clone(),
            events.clone(),
            pool.clone(),
            executor_config,
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
                ctx.trace_id(),
            )
            .await
        {
            tracing::warn!(error = %e, "Failed to broadcast run queued event");
        }

        self.executor.execute(ctx).await
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
        self.executor.execute(ctx).await
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

fn parse_completion_message(payload: &[u8]) -> Result<JobCompletionNotification> {
    use met_proto::controller::v1::JobCompletion;
    use prost::Message;

    let proto = JobCompletion::decode(payload)
        .map_err(|e| EngineError::internal(format!("Failed to decode completion: {e}")))?;

    let job_run_id = proto.job_run_id.parse()
        .map_err(|e| EngineError::internal(format!("Invalid job_run_id: {e}")))?;
    let run_id = proto.run_id.parse()
        .map_err(|e| EngineError::internal(format!("Invalid run_id: {e}")))?;
    let agent_id = proto.agent_id.parse()
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
        outputs: indexmap::IndexMap::new(),
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
        };

        let order = topological_order(&pipeline).unwrap();
        assert!(order.is_empty());
    }
}
