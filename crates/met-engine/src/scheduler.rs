//! Job scheduler for dispatching jobs to agents via NATS.

use async_nats::jetstream::Context as JetStreamContext;
use indexmap::IndexMap;
use met_core::ids::{AgentId, JobId, JobRunId, OrganizationId, PipelineId, ProjectId, RunId, StepRunId};
use met_store::repos::AgentRepo;
use met_parser::{JobIR, Shell, StepCommand, TagValue};
use prost::Message;
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, info, instrument, warn};

use crate::context::ExecutionContext;
use crate::error::{EngineError, Result};
use crate::events::EventBroadcaster;
use crate::persistence::RunPersistence;
use crate::state::RunState;

/// Job dispatch message for NATS.
#[derive(Debug, Clone)]
pub struct JobDispatchMessage {
    pub job_run_id: JobRunId,
    pub run_id: RunId,
    pub org_id: OrganizationId,
    pub project_id: Option<ProjectId>,
    pub pipeline_id: PipelineId,
    pub pipeline_name: String,
    pub job_name: String,
    pub steps: Vec<StepDispatch>,
    pub variables: IndexMap<String, String>,
    pub secrets: Vec<EncryptedSecretRef>,
    /// JSON secret hints for controller-side resolution (non-sensitive).
    pub secret_resolution_hints_json: String,
    pub requires_secret_exchange: bool,
    pub timeout_secs: u64,
    pub required_tags: Vec<String>,
    pub priority: i32,
}

/// Step specification for dispatch.
#[derive(Debug, Clone)]
pub struct StepDispatch {
    pub step_run_id: StepRunId,
    pub step_id: met_core::ids::StepId,
    pub name: String,
    pub command: String,
    pub shell: String,
    pub working_dir: Option<String>,
    pub environment: IndexMap<String, String>,
    pub sequence: i32,
    pub continue_on_error: bool,
    pub timeout_secs: u64,
}

/// Encrypted secret reference.
#[derive(Debug, Clone)]
pub struct EncryptedSecretRef {
    pub name: String,
    pub encrypted_value: Vec<u8>,
    pub sha256: String,
}

/// Job completion notification from agent.
#[derive(Debug, Clone)]
pub struct JobCompletionNotification {
    pub job_run_id: JobRunId,
    pub run_id: RunId,
    pub agent_id: AgentId,
    pub success: bool,
    pub exit_code: Option<i32>,
    pub error_message: Option<String>,
    pub duration_ms: u64,
    pub outputs: IndexMap<String, String>,
}

/// Scheduler configuration.
#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    /// Maximum time to wait for agent assignment.
    pub assignment_timeout: Duration,
    /// Maximum concurrent jobs per run.
    pub max_concurrent_jobs: usize,
    /// Base priority for jobs.
    pub base_priority: i32,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            assignment_timeout: Duration::from_secs(300),
            max_concurrent_jobs: 10,
            base_priority: 100,
        }
    }
}

/// Job scheduler for dispatching jobs to agents.
pub struct Scheduler {
    jetstream: JetStreamContext,
    pool: PgPool,
    config: SchedulerConfig,
    events: Arc<EventBroadcaster>,
    persistence: Arc<dyn RunPersistence>,
    active_jobs: RwLock<std::collections::HashMap<JobRunId, ActiveJob>>,
}

/// Active job tracking.
struct ActiveJob {
    job_id: JobId,
    run_id: RunId,
    dispatched_at: chrono::DateTime<chrono::Utc>,
    timeout: Duration,
}

impl Scheduler {
    /// Create a new scheduler.
    pub fn new(
        jetstream: JetStreamContext,
        pool: PgPool,
        events: Arc<EventBroadcaster>,
        config: SchedulerConfig,
        persistence: Arc<dyn RunPersistence>,
    ) -> Self {
        Self {
            jetstream,
            pool,
            config,
            events,
            persistence,
            active_jobs: RwLock::new(std::collections::HashMap::new()),
        }
    }

    /// Dispatch a job to an available agent.
    #[instrument(skip(self, ctx, run_state, job), fields(job_name = %job.name))]
    pub async fn dispatch_job(
        &self,
        ctx: &ExecutionContext,
        run_state: &RunState,
        job: &JobIR,
        job_run_id: JobRunId,
    ) -> Result<()> {
        info!(job = %job.name, "dispatching job");

        let variables = ctx.variables().await;
        let steps = self.prepare_steps(ctx, job).await?;

        let pool_tag = job
            .pool_selector
            .pool_name
            .clone()
            .unwrap_or_else(|| "_default".to_string());

        let required_tags: Vec<String> = job
            .pool_selector
            .required_tags
            .iter()
            .map(|(key, value)| match value {
                TagValue::String(s) => format!("{key}={s}"),
                TagValue::Bool(b) => format!("{key}={b}"),
                TagValue::Present => key.clone(),
            })
            .collect();

        let repo = AgentRepo::new(&self.pool);
        let candidates = repo
            .list_available_for_dispatch(ctx.org_id(), &pool_tag, &required_tags)
            .await?;

        let Some(chosen) = candidates.first() else {
            return Err(EngineError::NoAvailableAgents {
                job: job.name.clone(),
                tags: required_tags.clone(),
            });
        };

        for s in &steps {
            self.persistence
                .create_step_run(s.step_run_id, job_run_id, s.step_id, &s.name)
                .await?;
        }

        let message = self.build_dispatch_message(
            ctx,
            job,
            job_run_id,
            steps,
            variables,
            required_tags.clone(),
        )?;

        run_state.mark_job_queued(&job.id).await;

        let proto_message = self.to_proto_message(&message)?;
        let subject = format!(
            "met.jobs.{}.{}.{}",
            ctx.org_id().as_uuid(),
            pool_tag,
            chosen.id
        );
        let payload = proto_message.encode_to_vec();

        debug!(subject = %subject, job = %job.name, "publishing job dispatch");

        self.jetstream
            .publish(subject, payload.into())
            .await
            .map_err(|e| EngineError::Nats(format!("Failed to publish job dispatch: {e}")))?
            .await
            .map_err(|e| EngineError::Nats(format!("Failed to ack job dispatch: {e}")))?;

        self.active_jobs.write().await.insert(
            job_run_id,
            ActiveJob {
                job_id: job.id,
                run_id: ctx.run_id(),
                dispatched_at: chrono::Utc::now(),
                timeout: job.timeout,
            },
        );

        info!(job = %job.name, "job dispatched successfully");
        Ok(())
    }

    async fn prepare_steps(&self, ctx: &ExecutionContext, job: &JobIR) -> Result<Vec<StepDispatch>> {
        let mut steps = Vec::new();

        for (seq, step) in job.steps.iter().enumerate() {
            let (command, shell) = match &step.command {
                StepCommand::Run { shell, script } => {
                    let shell_str = match shell {
                        Shell::Bash => "bash",
                        Shell::Sh => "sh",
                        Shell::Powershell => "powershell",
                        Shell::Pwsh => "pwsh",
                        Shell::Cmd => "cmd",
                        Shell::Python => "python",
                    };
                    (script.clone(), shell_str.to_string())
                }
                StepCommand::Action { name, version, inputs } => {
                    let action_cmd = format!("met-action run {}@{} --inputs '{}'", 
                        name, version, serde_json::to_string(inputs).unwrap_or_default());
                    (action_cmd, "bash".to_string())
                }
            };

            let mut environment = IndexMap::new();
            for (key, value) in &step.env {
                if let Some(resolved) = ctx.resolve_env_value(value).await {
                    environment.insert(key.clone(), resolved);
                }
            }

            for (key, value) in &job.env {
                if !environment.contains_key(key) {
                    if let Some(resolved) = ctx.resolve_env_value(value).await {
                        environment.insert(key.clone(), resolved);
                    }
                }
            }

            steps.push(StepDispatch {
                step_run_id: StepRunId::new(),
                step_id: step.id,
                name: step.name.clone(),
                command,
                shell,
                working_dir: step.working_directory.clone(),
                environment,
                sequence: seq as i32,
                continue_on_error: step.continue_on_error,
                timeout_secs: step.timeout.as_secs(),
            });
        }

        Ok(steps)
    }

    fn build_dispatch_message(
        &self,
        ctx: &ExecutionContext,
        job: &JobIR,
        job_run_id: JobRunId,
        steps: Vec<StepDispatch>,
        variables: IndexMap<String, String>,
        required_tags: Vec<String>,
    ) -> Result<JobDispatchMessage> {
        let (requires_secret_exchange, secret_resolution_hints_json) =
            met_secret_resolve::hints_json_from_secret_refs(&ctx.pipeline().secret_refs);

        Ok(JobDispatchMessage {
            job_run_id,
            run_id: ctx.run_id(),
            org_id: ctx.org_id(),
            project_id: ctx.project_id(),
            pipeline_id: ctx.pipeline_id(),
            pipeline_name: ctx.pipeline().name.clone(),
            job_name: job.name.clone(),
            steps,
            variables,
            secrets: Vec::new(),
            secret_resolution_hints_json,
            requires_secret_exchange,
            timeout_secs: job.timeout.as_secs(),
            required_tags,
            priority: self.config.base_priority,
        })
    }

    fn to_proto_message(
        &self,
        msg: &JobDispatchMessage,
    ) -> Result<met_proto::controller::v1::JobDispatch> {
        use met_proto::common::v1::StepKind;
        use met_proto::controller::v1::{JobDispatch, StepSpec};

        let steps = msg
            .steps
            .iter()
            .map(|s| StepSpec {
                step_run_id: s.step_run_id.to_string(),
                step_id: s.step_id.to_string(),
                name: s.name.clone(),
                kind: StepKind::Command.into(),
                command: s.command.clone(),
                image: String::new(),
                working_dir: s.working_dir.clone().unwrap_or_default(),
                shell: s.shell.clone(),
                environment: s.environment.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
                sequence: s.sequence,
                continue_on_error: s.continue_on_error,
                timeout_secs: s.timeout_secs as i32,
            })
            .collect();

        let project_id = msg
            .project_id
            .map(|p| p.to_string())
            .unwrap_or_default();
        Ok(JobDispatch {
            job_run_id: msg.job_run_id.to_string(),
            run_id: msg.run_id.to_string(),
            org_id: msg.org_id.to_string(),
            pipeline_name: msg.pipeline_name.clone(),
            job_name: msg.job_name.clone(),
            steps,
            variables: msg.variables.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
            secrets: Vec::new(),
            timeout_secs: msg.timeout_secs as i32,
            required_tags: msg.required_tags.clone(),
            priority: msg.priority,
            cache_restore: None,
            input_artifacts: Vec::new(),
            services: Vec::new(),
            retry_policy: None,
            trace_id: String::new(),
            attempt: 1,
            requires_secret_exchange: msg.requires_secret_exchange,
            project_id,
            pipeline_id: msg.pipeline_id.to_string(),
            secret_resolution_hints_json: msg.secret_resolution_hints_json.clone(),
        })
    }

    /// Handle a job completion notification.
    #[instrument(skip(self, run_state, ctx))]
    pub async fn handle_completion(
        &self,
        notification: JobCompletionNotification,
        run_state: &RunState,
        ctx: &ExecutionContext,
    ) -> Result<()> {
        let job_run_id = notification.job_run_id;
        
        self.active_jobs.write().await.remove(&job_run_id);

        if let Some(job_state) = run_state.get_job_by_run_id(notification.job_run_id).await {
            run_state
                .mark_job_completed(
                    &job_state.job_id,
                    notification.success,
                    notification.exit_code,
                    notification.error_message.clone(),
                )
                .await;

            if !notification.outputs.is_empty() {
                ctx.set_job_outputs(job_state.job_id, notification.outputs).await;
            }

            let duration_ms = notification.duration_ms;
            if let Err(e) = self
                .events
                .job_completed(
                    job_run_id,
                    notification.run_id,
                    ctx.pipeline_id(),
                    notification.agent_id,
                    notification.success,
                    notification.exit_code,
                    duration_ms,
                    Some(ctx.trace_id()),
                )
                .await
            {
                warn!(error = %e, "Failed to broadcast job completion event");
            }
        }

        Ok(())
    }

    /// Cancel a running job.
    #[instrument(skip(self))]
    pub async fn cancel_job(&self, org_id: OrganizationId, job_run_id: JobRunId) -> Result<()> {
        let subject = format!("met.cancel.{}.{}", org_id.as_uuid(), job_run_id);
        let payload = job_run_id.to_string().into_bytes();

        self.jetstream
            .publish(subject, payload.into())
            .await
            .map_err(|e| EngineError::Nats(format!("Failed to publish cancellation: {e}")))?;

        self.active_jobs.write().await.remove(&job_run_id);

        info!(%job_run_id, "job cancellation requested");
        Ok(())
    }

    /// Check for timed-out jobs.
    pub async fn check_timeouts(&self, run_state: &RunState) -> Vec<JobRunId> {
        let now = chrono::Utc::now();
        let mut timed_out = Vec::new();

        let active = self.active_jobs.read().await;
        for (job_run_id, active_job) in active.iter() {
            let elapsed = now - active_job.dispatched_at;
            if elapsed > chrono::Duration::from_std(active_job.timeout).unwrap_or(chrono::TimeDelta::MAX) {
                timed_out.push(*job_run_id);
            }
        }
        drop(active);

        for job_run_id in &timed_out {
            if let Some(active_job) = self.active_jobs.write().await.remove(job_run_id) {
                run_state.mark_job_timed_out(&active_job.job_id).await;
            }
        }

        timed_out
    }

    /// Get count of active jobs for a run.
    pub async fn active_job_count(&self, run_id: RunId) -> usize {
        self.active_jobs
            .read()
            .await
            .values()
            .filter(|j| j.run_id == run_id)
            .count()
    }

    /// Drop scheduler bookkeeping for a job run (e.g. DB reconciliation already shows terminal).
    pub async fn forget_active_job(&self, job_run_id: JobRunId) {
        self.active_jobs.write().await.remove(&job_run_id);
    }
}

