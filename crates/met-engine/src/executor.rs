//! DAG executor for pipeline execution.
//!
//! The executor walks the job DAG in topological order, respecting dependencies,
//! evaluating conditions, and dispatching jobs to agents via the scheduler.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use futures::StreamExt;
use met_core::ids::{JobId, JobRunId, OrganizationId, RunId};
use met_core::models::{JobStatus, RunStatus};
use met_parser::{JobIR, PipelineIR, WorkflowScope as IrWorkflowScope};
use met_store::repos::{
    DefinitionSnapshotRepo, PipelineRepo, WorkflowRepo, WorkflowScope as DbWorkflowScope,
};
use met_secrets::BuiltinStoredCrypto;
use met_store::repos::JobRunRepo;
use secrecy::SecretString;
use sqlx::PgPool;
use tokio::sync::mpsc;
use tokio::time::interval;
use tracing::{debug, error, info, instrument, warn};

use crate::cache::{CacheBackend, CacheLookupResult, CacheManager};
use crate::cel::{evaluate_condition, CelContext};
use crate::context::ExecutionContext;
use crate::error::{EngineError, Result};
use crate::events::EventBroadcaster;
use crate::parse_completion_message;
use crate::persistence::{JobRunSourceRefs, RunPersistence};
use crate::scheduler::{JobCompletionNotification, Scheduler};
use crate::state::{JobState, RunState};

/// How the `runs` table row was created before [`Executor::execute`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunStartKind {
    /// Create a new `runs` row from the engine (standalone execution).
    New,
    /// Row already exists (for example API trigger); only backfill org/trace when null.
    Existing,
}

/// Executor configuration.
#[derive(Debug, Clone)]
pub struct ExecutorConfig {
    /// Interval for checking job status.
    pub poll_interval: Duration,
    /// Maximum concurrent jobs.
    pub max_concurrent: usize,
    /// Timeout for the entire run.
    pub run_timeout: Duration,
    /// Whether to fail fast on first job failure.
    pub fail_fast: bool,
}

impl Default for ExecutorConfig {
    fn default() -> Self {
        Self {
            poll_interval: Duration::from_secs(1),
            max_concurrent: 10,
            run_timeout: Duration::from_secs(3600 * 6),
            fail_fast: false,
        }
    }
}

/// Result of a pipeline execution.
#[derive(Debug)]
pub struct ExecutionResult {
    pub run_id: RunId,
    pub status: RunStatus,
    pub duration_ms: u64,
    pub jobs_succeeded: usize,
    pub jobs_failed: usize,
    pub jobs_skipped: usize,
}

/// DAG executor for pipeline runs.
pub struct Executor<C: CacheBackend> {
    scheduler: Arc<Scheduler>,
    cache: Arc<CacheManager<C>>,
    events: Arc<EventBroadcaster>,
    pool: PgPool,
    config: ExecutorConfig,
    /// When set, stored/builtin pipeline secrets are validated and resolved into the run context.
    builtin_stored_crypto: Option<Arc<BuiltinStoredCrypto>>,
    persistence: Arc<dyn RunPersistence>,
    jetstream: async_nats::jetstream::Context,
}

impl<C: CacheBackend> Executor<C> {
    /// Create a new executor.
    pub fn new(
        scheduler: Arc<Scheduler>,
        cache: Arc<CacheManager<C>>,
        events: Arc<EventBroadcaster>,
        pool: PgPool,
        config: ExecutorConfig,
        builtin_stored_crypto: Option<Arc<BuiltinStoredCrypto>>,
        persistence: Arc<dyn RunPersistence>,
        jetstream: async_nats::jetstream::Context,
    ) -> Self {
        Self {
            scheduler,
            cache,
            events,
            pool,
            config,
            builtin_stored_crypto,
            persistence,
            jetstream,
        }
    }

    /// Execute a pipeline run.
    #[instrument(skip(self, ctx), fields(run_id = %ctx.run_id(), pipeline = %ctx.pipeline().name))]
    pub async fn execute(&self, ctx: ExecutionContext, start: RunStartKind) -> Result<ExecutionResult> {
        let run_id = ctx.run_id();
        let pipeline_id = ctx.pipeline_id();
        let start_time = Utc::now();

        info!(pipeline = %ctx.pipeline().name, "starting pipeline execution");

        met_secret_resolve::validate_secret_refs(
            &self.pool,
            ctx.org_id(),
            ctx.project_id(),
            ctx.pipeline_id(),
            &ctx.pipeline().secret_refs,
        )
        .await?;

        if !ctx.pipeline().secret_refs.is_empty() {
            let Some(crypto) = self.builtin_stored_crypto.as_ref() else {
                return Err(EngineError::SecretResolution(
                    "MET_BUILTIN_SECRETS_MASTER_KEY is not set but the pipeline declares secrets"
                        .into(),
                ));
            };
            let resolved = met_secret_resolve::resolve_stored_secret_map(
                &self.pool,
                crypto.as_ref(),
                ctx.org_id(),
                ctx.project_id(),
                ctx.pipeline_id(),
                &ctx.pipeline().secret_refs,
            )
            .await?;
            for (name, (value, _, _)) in resolved {
                ctx.register_secret(name, SecretString::new(value.into_boxed_str())).await;
            }
        }

        match start {
            RunStartKind::New => {
                self.persistence
                    .create_run(
                        ctx.run_id(),
                        ctx.pipeline_id(),
                        ctx.org_id(),
                        ctx.triggered_by(),
                        ctx.trace_uuid(),
                    )
                    .await?;
            }
            RunStartKind::Existing => {
                self.persistence
                    .prepare_existing_run(ctx.run_id(), ctx.org_id(), Some(ctx.trace_uuid()))
                    .await?;
            }
        }

        if ctx.pipeline().jobs.is_empty() {
            return Err(EngineError::EmptyPipeline);
        }

        let run_state = RunState::new(run_id);
        run_state.set_status(RunStatus::Running).await;

        if let Err(e) = self
            .events
            .run_started(run_id, pipeline_id, Some(ctx.trace_id()))
            .await
        {
            warn!(error = %e, "Failed to broadcast run started event");
        }

        self.initialize_job_states(&ctx, &run_state).await?;

        self.persistence
            .update_run_status(run_id, RunStatus::Running)
            .await?;

        let completion_horizon_ts = (Utc::now() - chrono::Duration::seconds(120)).timestamp();
        let completion_horizon = time::OffsetDateTime::from_unix_timestamp(completion_horizon_ts)
            .unwrap_or(time::OffsetDateTime::UNIX_EPOCH);

        let (completion_tx, completion_rx) = mpsc::channel::<JobCompletionNotification>(64);
        let comp_task = {
            let js = self.jetstream.clone();
            let org_id = ctx.org_id();
            let run_id = ctx.run_id();
            tokio::spawn(run_completion_pull_task(
                js,
                org_id,
                run_id,
                completion_tx,
                completion_horizon,
            ))
        };

        let exec_result = self
            .run_execution_loop(&ctx, &run_state, completion_rx)
            .await;
        comp_task.abort();

        let final_status = match &exec_result {
            Ok(()) => run_state.compute_final_status().await,
            Err(EngineError::RunCancelled { .. }) => RunStatus::Cancelled,
            Err(_) => RunStatus::Failed,
        };

        let persist_err_msg: Option<String> = match &exec_result {
            Ok(()) | Err(EngineError::RunCancelled { .. }) => None,
            Err(e) => Some(e.to_string()),
        };

        run_state.set_status(final_status).await;

        if let Err(e) = self
            .persistence
            .complete_run(run_id, final_status, persist_err_msg.as_deref())
            .await
        {
            warn!(error = %e, %run_id, "failed to persist run completion");
        }

        let end_time = Utc::now();
        let duration_ms = (end_time - start_time).num_milliseconds() as u64;

        let jobs = run_state.all_jobs().await;
        let jobs_succeeded = jobs.values().filter(|j| j.status == JobStatus::Succeeded).count();
        let jobs_failed = jobs.values().filter(|j| matches!(j.status, JobStatus::Failed | JobStatus::TimedOut | JobStatus::Cancelled)).count();
        let jobs_skipped = jobs.values().filter(|j| j.status == JobStatus::Skipped).count();

        if let Err(e) = self
            .events
            .run_completed(
                run_id,
                pipeline_id,
                final_status.is_success(),
                duration_ms,
                Some(ctx.trace_id()),
            )
            .await
        {
            warn!(error = %e, "Failed to broadcast run completed event");
        }

        info!(
            status = ?final_status,
            duration_ms,
            succeeded = jobs_succeeded,
            failed = jobs_failed,
            skipped = jobs_skipped,
            "pipeline execution completed"
        );

        match exec_result {
            Ok(()) => Ok(ExecutionResult {
                run_id,
                status: final_status,
                duration_ms,
                jobs_succeeded,
                jobs_failed,
                jobs_skipped,
            }),
            Err(EngineError::RunCancelled { .. }) => Ok(ExecutionResult {
                run_id,
                status: final_status,
                duration_ms,
                jobs_succeeded,
                jobs_failed,
                jobs_skipped,
            }),
            Err(e) => Err(e),
        }
    }

    async fn initialize_job_states(&self, ctx: &ExecutionContext, run_state: &RunState) -> Result<()> {
        let pipeline_row = PipelineRepo::new(&self.pool).get(ctx.pipeline_id()).await?;
        let pipeline_digest =
            DefinitionSnapshotRepo::ensure_json(&self.pool, &pipeline_row.definition).await?;

        let wf_repo = WorkflowRepo::new(&self.pool);

        for job in &ctx.pipeline().jobs {
            let source_meta = job.source_workflow.as_ref().map(|wf| {
                serde_json::json!({
                    "scope": match wf.scope {
                        IrWorkflowScope::Global => "global",
                        IrWorkflowScope::Project => "project",
                    },
                    "name": wf.name,
                    "version": wf.version,
                })
            });

            let workflow_digest = if let Some(wf) = &job.source_workflow {
                let scope = match wf.scope {
                    IrWorkflowScope::Global => DbWorkflowScope::Global,
                    IrWorkflowScope::Project => DbWorkflowScope::Project,
                };
                match wf_repo
                    .get(ctx.org_id(), ctx.project_id(), scope, &wf.name, &wf.version)
                    .await
                {
                    Ok(row) => Some(DefinitionSnapshotRepo::ensure_json(&self.pool, &row.definition).await?),
                    Err(e) => {
                        warn!(
                            error = %e,
                            job = %job.name,
                            "could not load reusable workflow for definition snapshot; workflow digest omitted"
                        );
                        None
                    }
                }
            } else {
                None
            };

            let job_run_id = JobRunId::new();
            self.persistence
                .create_job_run(
                    job_run_id,
                    ctx.run_id(),
                    job.id,
                    &job.name,
                    JobRunSourceRefs {
                        pipeline_definition_sha256: pipeline_digest,
                        workflow_definition_sha256: workflow_digest,
                        source_workflow: source_meta,
                    },
                )
                .await?;
            let job_state = JobState::new(job.id, job_run_id, &job.name);
            run_state.register_job(job_state).await;
        }
        Ok(())
    }

    async fn run_execution_loop(
        &self,
        ctx: &ExecutionContext,
        run_state: &RunState,
        mut completion_rx: mpsc::Receiver<JobCompletionNotification>,
    ) -> Result<()> {
        let mut poll_interval = interval(self.config.poll_interval);
        let run_start = Utc::now();

        loop {
            tokio::select! {
                _ = poll_interval.tick() => {
                    if run_state.is_cancellation_requested().await {
                        return Err(EngineError::RunCancelled { run_id: ctx.run_id() });
                    }

                    if let Err(e) = self.reconcile_terminal_jobs_from_db(ctx, run_state).await {
                        warn!(error = %e, "failed to reconcile job runs from database");
                    }

                    let elapsed = Utc::now() - run_start;
                    if elapsed > chrono::Duration::from_std(self.config.run_timeout).unwrap_or(chrono::TimeDelta::MAX) {
                        error!("Run timed out");
                        return Err(EngineError::Internal("Run execution timed out".to_string()));
                    }

                    self.scheduler.check_timeouts(run_state).await;

                    let ready_jobs = self.find_ready_jobs(ctx, run_state).await?;

                    let active_count = self.scheduler.active_job_count(ctx.run_id()).await;
                    let can_dispatch = self.config.max_concurrent.saturating_sub(active_count);

                    for job in ready_jobs.into_iter().take(can_dispatch) {
                        self.dispatch_job_if_ready(ctx, run_state, job).await?;
                    }

                    if self.config.fail_fast && run_state.has_failures().await {
                        info!("Fail-fast triggered, cancelling remaining jobs");
                        self.cancel_pending_jobs(ctx, run_state).await?;
                        break;
                    }

                    if run_state.is_complete().await {
                        break;
                    }
                }
                note = completion_rx.recv() => {
                    let Some(note) = note else {
                        return Err(EngineError::Internal(
                            "job completion subscriber disconnected".to_string(),
                        ));
                    };
                    if note.run_id != ctx.run_id() {
                        continue;
                    }
                    if let Err(e) = self.handle_job_completion(note, ctx, run_state).await {
                        warn!(error = %e, "failed to apply job completion notification");
                    }
                }
            }
        }

        Ok(())
    }

    async fn find_ready_jobs<'a>(
        &self,
        ctx: &'a ExecutionContext,
        run_state: &RunState,
    ) -> Result<Vec<&'a JobIR>> {
        let mut ready = Vec::new();
        let pending = run_state.pending_jobs().await;

        for job in &ctx.pipeline().jobs {
            if !pending.contains(&job.id) {
                continue;
            }

            let deps_satisfied = self
                .check_dependencies_satisfied(job, run_state)
                .await;

            if deps_satisfied {
                ready.push(job);
            }
        }

        Ok(ready)
    }

    async fn check_dependencies_satisfied(&self, job: &JobIR, run_state: &RunState) -> bool {
        for dep_id in &job.depends_on {
            if !run_state.is_job_complete(dep_id).await {
                return false;
            }
        }
        true
    }

    async fn dispatch_job_if_ready(
        &self,
        ctx: &ExecutionContext,
        run_state: &RunState,
        job: &JobIR,
    ) -> Result<()> {
        if let Some(condition) = &job.condition {
            let cel_ctx = CelContext::from_state(ctx, run_state, &job.depends_on).await;

            match evaluate_condition(condition, &cel_ctx) {
                Ok(true) => {
                    debug!(job = %job.name, condition, "condition evaluated to true");
                }
                Ok(false) => {
                    info!(job = %job.name, condition, "job skipped due to condition");
                    run_state
                        .mark_job_skipped(&job.id, Some(format!("Condition '{condition}' evaluated to false")))
                        .await;
                    return Ok(());
                }
                Err(e) => {
                    if condition != "success()" {
                        warn!(job = %job.name, condition, error = %e, "condition evaluation failed, skipping job");
                        run_state
                            .mark_job_skipped(&job.id, Some(format!("Condition evaluation failed: {e}")))
                            .await;
                        return Ok(());
                    }
                }
            }
        } else {
            let any_dep_failed = job.depends_on.iter().any(|dep_id| {
                futures::executor::block_on(run_state.failed_jobs()).contains(dep_id)
            });

            if any_dep_failed {
                info!(job = %job.name, "job skipped due to dependency failure");
                run_state
                    .mark_job_skipped(&job.id, Some("Dependency failed".to_string()))
                    .await;
                return Ok(());
            }
        }

        if let Some(cache_config) = &job.cache_config {
            let cache_key = self
                .cache
                .compute_key(
                    &cache_config.key,
                    &cache_config.paths,
                    &cache_config.restore_keys,
                    ctx,
                )
                .await?;

            match self.cache.lookup(&cache_key).await? {
                CacheLookupResult::Hit { key, storage_path: _, .. } => {
                    info!(job = %job.name, cache_key = %key, "cache hit, skipping job");
                    run_state
                        .mark_job_completed(&job.id, true, Some(0), None)
                        .await;
                    return Ok(());
                }
                CacheLookupResult::PartialHit { matched_key, .. } => {
                    debug!(job = %job.name, matched_key, "partial cache hit, will restore");
                }
                CacheLookupResult::Miss { .. } => {
                    debug!(job = %job.name, "cache miss");
                }
            }
        }

        let job_state = run_state.get_job(&job.id).await
            .ok_or_else(|| EngineError::JobNotFound(job.id))?;

        match self
            .scheduler
            .dispatch_job(ctx, run_state, job, job_state.job_run_id)
            .await
        {
            Ok(()) => Ok(()),
            Err(EngineError::NoAvailableAgents { job: j, tags }) => {
                info!(
                    job = %j,
                    ?tags,
                    "no eligible agent yet; job stays pending until one is available (retrying on next poll)"
                );
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    /// When the controller updates `job_runs` but a JetStream completion is missed (`DeliverPolicy::New`,
    /// publish failure, etc.), engine `RunState` would never reach `is_complete`. Sync terminal rows from PG.
    async fn reconcile_terminal_jobs_from_db(
        &self,
        ctx: &ExecutionContext,
        run_state: &RunState,
    ) -> Result<()> {
        let repo = JobRunRepo::new(&self.pool);
        let rows = repo.list_by_run(ctx.run_id()).await?;

        for jr in rows {
            if !jr.status.is_terminal() {
                continue;
            }
            let Some(js) = run_state.get_job_by_run_id(jr.id).await else {
                continue;
            };
            if run_state.is_job_complete(&js.job_id).await {
                continue;
            }

            self.scheduler.forget_active_job(jr.id).await;

            match jr.status {
                JobStatus::Succeeded => {
                    run_state
                        .mark_job_completed(
                            &js.job_id,
                            true,
                            jr.exit_code,
                            jr.error_message.clone(),
                        )
                        .await;
                }
                JobStatus::Failed => {
                    run_state
                        .mark_job_completed(
                            &js.job_id,
                            false,
                            jr.exit_code,
                            jr.error_message.clone(),
                        )
                        .await;
                }
                JobStatus::Cancelled => {
                    run_state.mark_job_cancelled(&js.job_id).await;
                }
                JobStatus::TimedOut => {
                    run_state.mark_job_timed_out(&js.job_id).await;
                }
                JobStatus::Skipped => {
                    run_state
                        .mark_job_skipped(&js.job_id, jr.error_message.clone())
                        .await;
                }
                JobStatus::Pending | JobStatus::Queued | JobStatus::Running => {}
            }
        }

        Ok(())
    }

    async fn cancel_pending_jobs(
        &self,
        _ctx: &ExecutionContext,
        run_state: &RunState,
    ) -> Result<()> {
        let pending = run_state.pending_jobs().await;
        for job_id in pending {
            run_state
                .mark_job_cancelled(&job_id)
                .await;
        }
        Ok(())
    }

    /// Handle a job completion notification (called from completion listener).
    pub async fn handle_job_completion(
        &self,
        notification: JobCompletionNotification,
        ctx: &ExecutionContext,
        run_state: &RunState,
    ) -> Result<()> {
        self.scheduler
            .handle_completion(notification, run_state, ctx)
            .await
    }

    /// Request cancellation of a run.
    pub async fn cancel(&self, run_state: &RunState) {
        run_state.request_cancellation().await;
    }
}

async fn run_completion_pull_task(
    jetstream: async_nats::jetstream::Context,
    org_id: OrganizationId,
    run_id_filter: RunId,
    tx: mpsc::Sender<JobCompletionNotification>,
    completion_horizon: time::OffsetDateTime,
) {
    use async_nats::jetstream::consumer::pull::Config;

    loop {
        let stream = match jetstream.get_stream("COMPLETIONS").await {
            Ok(s) => s,
            Err(e) => {
                warn!(error = %e, "get COMPLETIONS stream; retrying");
                tokio::time::sleep(Duration::from_secs(2)).await;
                continue;
            }
        };
        let filter_subject = format!("met.completions.{}", org_id.as_uuid());
        let config = Config {
            name: Some(format!("engine-{}", uuid::Uuid::new_v4())),
            deliver_policy: async_nats::jetstream::consumer::DeliverPolicy::ByStartTime {
                start_time: completion_horizon,
            },
            filter_subject,
            ack_policy: async_nats::jetstream::consumer::AckPolicy::Explicit,
            ..Default::default()
        };
        let consumer = match stream.create_consumer(config).await {
            Ok(c) => c,
            Err(e) => {
                warn!(error = %e, "create ephemeral completion consumer; retrying");
                tokio::time::sleep(Duration::from_secs(2)).await;
                continue;
            }
        };
        let mut messages = match consumer.messages().await {
            Ok(m) => m,
            Err(e) => {
                warn!(error = %e, "completion consumer messages(); retrying");
                tokio::time::sleep(Duration::from_secs(2)).await;
                continue;
            }
        };
        while let Some(msg) = messages.next().await {
            let msg = match msg {
                Ok(m) => m,
                Err(e) => {
                    warn!(error = %e, "completion pull error");
                    break;
                }
            };
            if let Ok(note) = parse_completion_message(msg.payload.as_ref()) {
                if note.run_id == run_id_filter && tx.send(note).await.is_err() {
                    return;
                }
            }
            let _ = msg.ack().await;
        }
        warn!("completion message stream ended; reconnecting");
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

/// Build a topological order of jobs from the pipeline IR.
pub fn topological_order(pipeline: &PipelineIR) -> Result<Vec<JobId>> {
    let mut in_degree: HashMap<JobId, usize> = HashMap::new();
    let mut adjacency: HashMap<JobId, Vec<JobId>> = HashMap::new();

    for job in &pipeline.jobs {
        in_degree.entry(job.id).or_insert(0);
        adjacency.entry(job.id).or_default();

        for dep_id in &job.depends_on {
            adjacency.entry(*dep_id).or_default().push(job.id);
            *in_degree.entry(job.id).or_insert(0) += 1;
        }
    }

    let mut queue: Vec<JobId> = in_degree
        .iter()
        .filter(|&(_, degree)| *degree == 0)
        .map(|(id, _)| *id)
        .collect();

    let mut order = Vec::new();

    while let Some(job_id) = queue.pop() {
        order.push(job_id);

        if let Some(dependents) = adjacency.get(&job_id) {
            for dependent in dependents {
                if let Some(degree) = in_degree.get_mut(dependent) {
                    *degree -= 1;
                    if *degree == 0 {
                        queue.push(*dependent);
                    }
                }
            }
        }
    }

    if order.len() != pipeline.jobs.len() {
        return Err(EngineError::CycleDetected);
    }

    Ok(order)
}

#[cfg(test)]
mod tests {
    use super::*;
    use met_core::ids::PipelineId;
    use met_parser::PoolSelector;
    use std::time::Duration;

    fn test_job(id: JobId, name: &str, depends_on: Vec<JobId>) -> JobIR {
        JobIR {
            id,
            name: name.to_string(),
            depends_on,
            pool_selector: PoolSelector::default(),
            steps: Vec::new(),
            services: Vec::new(),
            timeout: Duration::from_secs(300),
            retry_policy: None,
            cache_config: None,
            condition: None,
            source_workflow: None,
            env: Default::default(),
        }
    }

    fn test_pipeline(jobs: Vec<JobIR>) -> PipelineIR {
        PipelineIR {
            id: PipelineId::new(),
            name: "test".to_string(),
            source_file: None,
            project_id: None,
            triggers: Vec::new(),
            variables: Default::default(),
            secret_refs: Default::default(),
            jobs,
            default_pool_selector: None,
        }
    }

    #[test]
    fn test_topological_order_linear() {
        let a = JobId::new();
        let b = JobId::new();
        let c = JobId::new();

        let pipeline = test_pipeline(vec![
            test_job(a, "a", vec![]),
            test_job(b, "b", vec![a]),
            test_job(c, "c", vec![b]),
        ]);

        let order = topological_order(&pipeline).unwrap();

        let a_pos = order.iter().position(|&id| id == a).unwrap();
        let b_pos = order.iter().position(|&id| id == b).unwrap();
        let c_pos = order.iter().position(|&id| id == c).unwrap();

        assert!(a_pos < b_pos);
        assert!(b_pos < c_pos);
    }

    #[test]
    fn test_topological_order_diamond() {
        let a = JobId::new();
        let b = JobId::new();
        let c = JobId::new();
        let d = JobId::new();

        let pipeline = test_pipeline(vec![
            test_job(a, "a", vec![]),
            test_job(b, "b", vec![a]),
            test_job(c, "c", vec![a]),
            test_job(d, "d", vec![b, c]),
        ]);

        let order = topological_order(&pipeline).unwrap();

        let a_pos = order.iter().position(|&id| id == a).unwrap();
        let b_pos = order.iter().position(|&id| id == b).unwrap();
        let c_pos = order.iter().position(|&id| id == c).unwrap();
        let d_pos = order.iter().position(|&id| id == d).unwrap();

        assert!(a_pos < b_pos);
        assert!(a_pos < c_pos);
        assert!(b_pos < d_pos);
        assert!(c_pos < d_pos);
    }

    #[test]
    fn test_topological_order_cycle() {
        let a = JobId::new();
        let b = JobId::new();

        let pipeline = test_pipeline(vec![
            test_job(a, "a", vec![b]),
            test_job(b, "b", vec![a]),
        ]);

        assert!(matches!(topological_order(&pipeline), Err(EngineError::CycleDetected)));
    }
}
