//! Job executor for running pipeline jobs.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_nats::jetstream::consumer::pull::Stream as PullStream;
use chrono::Utc;
use futures::StreamExt;
use met_proto::agent::v1::{
    agent_service_client::AgentServiceClient, JobAssignment, JobKeyExchange, JobStatusUpdate,
    StepSpec, StepStatusUpdate,
};
use met_proto::common::v1::{RunStatus, StepKind, Timestamp};
use prost::Message;
use tokio::sync::{mpsc, watch, RwLock};
use tonic::transport::Channel;
use tracing::{debug, error, info, instrument, warn};

use crate::backend::ExecutionBackend;
use crate::config::{AgentConfig, AgentIdentity};
use crate::error::{AgentError, Result};
use crate::heartbeat::HeartbeatState;
use crate::security::JobPki;

/// Job executor that pulls jobs from NATS and executes them.
pub struct JobExecutor {
    config: AgentConfig,
    identity: AgentIdentity,
    client: AgentServiceClient<Channel>,
    backend: Arc<dyn ExecutionBackend>,
    heartbeat_state: Arc<RwLock<HeartbeatState>>,
    shutdown_rx: watch::Receiver<bool>,
}

impl JobExecutor {
    /// Create a new job executor.
    pub fn new(
        config: AgentConfig,
        identity: AgentIdentity,
        client: AgentServiceClient<Channel>,
        backend: Arc<dyn ExecutionBackend>,
        heartbeat_state: Arc<RwLock<HeartbeatState>>,
        shutdown_rx: watch::Receiver<bool>,
    ) -> Self {
        Self {
            config,
            identity,
            client,
            backend,
            heartbeat_state,
            shutdown_rx,
        }
    }

    /// Run the executor loop.
    pub async fn run(mut self, nats_client: async_nats::Client) -> Result<()> {
        info!("starting job executor");

        // Create JetStream context
        let jetstream = async_nats::jetstream::new(nats_client);

        // Get the JOBS stream
        let stream = match jetstream.get_stream("JOBS").await {
            Ok(s) => s,
            Err(e) => {
                error!(error = %e, "failed to get JOBS stream");
                return Err(AgentError::Nats(e.into()));
            }
        };

        // Create a consumer for our subjects
        // For simplicity, consume from the first subject
        let subject = self
            .identity
            .nats_subjects
            .first()
            .cloned()
            .unwrap_or_else(|| "met.jobs.*._default".to_string());

        let consumer_name = format!("agent-{}", self.identity.agent_id);

        let consumer = stream
            .create_consumer(async_nats::jetstream::consumer::pull::Config {
                name: Some(consumer_name.clone()),
                durable_name: Some(consumer_name),
                filter_subject: subject.clone(),
                ack_policy: async_nats::jetstream::consumer::AckPolicy::Explicit,
                ack_wait: Duration::from_secs(30),
                max_deliver: 3,
                ..Default::default()
            })
            .await
            .map_err(|e| AgentError::Nats(e.into()))?;

        info!(subject = %subject, "subscribed to job dispatch");

        // Pull messages
        let mut messages = consumer
            .messages()
            .await
            .map_err(|e| AgentError::Nats(e.into()))?;

        loop {
            tokio::select! {
                msg = messages.next() => {
                    match msg {
                        Some(Ok(message)) => {
                            // Parse job dispatch
                            match met_proto::controller::v1::JobDispatch::decode(message.payload.as_ref()) {
                                Ok(job) => {
                                    // Ack receipt
                                    if let Err(e) = message.ack().await {
                                        warn!(error = %e, "failed to ack message");
                                    }

                                    // Execute job
                                    if let Err(e) = self.execute_job(job).await {
                                        error!(error = %e, "job execution failed");
                                    }
                                }
                                Err(e) => {
                                    warn!(error = %e, "failed to decode job dispatch");
                                    // Ack anyway to prevent redelivery of bad messages
                                    let _ = message.ack().await;
                                }
                            }
                        }
                        Some(Err(e)) => {
                            error!(error = %e, "error receiving message");
                        }
                        None => {
                            info!("message stream ended");
                            break;
                        }
                    }
                }
                _ = self.shutdown_rx.changed() => {
                    if *self.shutdown_rx.borrow() {
                        info!("executor shutting down");
                        break;
                    }
                }
            }
        }

        Ok(())
    }

    /// Execute a single job.
    #[instrument(skip(self, job), fields(job_run_id = %job.job_run_id))]
    async fn execute_job(&mut self, job: met_proto::controller::v1::JobDispatch) -> Result<()> {
        info!(
            job_name = %job.job_name,
            pipeline = %job.pipeline_name,
            steps = job.steps.len(),
            "executing job"
        );

        // Update heartbeat state
        {
            let mut state = self.heartbeat_state.write().await;
            state.running_jobs += 1;
            state.current_job_id = Some(job.job_run_id.clone());
            state.status = met_proto::AgentStatus::Busy;
        }

        // Report job accepted
        self.report_job_status(&job.job_run_id, RunStatus::Running, None, None)
            .await?;

        // Create workspace
        let workspace = self.create_workspace(&job.job_run_id).await?;

        // Generate per-job PKI and exchange keys
        let pki = JobPki::generate().map_err(|e| AgentError::Certificate(e))?;

        let secrets = if !job.secrets.is_empty() {
            self.exchange_keys(&job.job_run_id, &pki).await?
        } else {
            HashMap::new()
        };

        // Execute steps sequentially
        let mut job_success = true;
        for step in &job.steps {
            let step_result = self
                .execute_step(step, &workspace, &job.variables, &secrets)
                .await;

            match step_result {
                Ok(exit_code) => {
                    if exit_code != 0 && !step.continue_on_error {
                        job_success = false;
                        break;
                    }
                }
                Err(e) => {
                    error!(step = %step.name, error = %e, "step failed");
                    job_success = false;
                    if !step.continue_on_error {
                        break;
                    }
                }
            }
        }

        // Report job completion
        let final_status = if job_success {
            RunStatus::Succeeded
        } else {
            RunStatus::Failed
        };
        self.report_job_status(&job.job_run_id, final_status, Some(0), None)
            .await?;

        // Cleanup workspace
        self.cleanup_workspace(&workspace).await?;

        // Update heartbeat state
        {
            let mut state = self.heartbeat_state.write().await;
            state.running_jobs -= 1;
            state.current_job_id = None;
            if state.running_jobs == 0 {
                state.status = met_proto::AgentStatus::Online;
            }
        }

        info!(
            job_run_id = %job.job_run_id,
            success = job_success,
            "job execution complete"
        );

        Ok(())
    }

    /// Execute a single step.
    #[instrument(skip(self, step, workspace, variables, secrets), fields(step_name = %step.name))]
    async fn execute_step(
        &mut self,
        step: &met_proto::controller::v1::StepSpec,
        workspace: &Path,
        variables: &HashMap<String, String>,
        secrets: &HashMap<String, String>,
    ) -> Result<i32> {
        info!(step = %step.name, "executing step");

        // Report step started
        self.report_step_status(&step.step_run_id, &step.step_id, RunStatus::Running, None, None)
            .await?;

        // Build environment
        let mut env = step.environment.clone();
        env.extend(variables.clone());
        env.extend(secrets.clone());

        // Convert to backend step spec
        let backend_step = crate::backend::StepSpec {
            step_id: step.step_id.clone(),
            name: step.name.clone(),
            command: step.command.clone(),
            image: step.image.clone(),
            working_dir: step.working_dir.clone(),
            shell: step.shell.clone(),
            environment: env,
            timeout: Duration::from_secs(step.timeout_secs as u64),
        };

        // Execute
        let result = self.backend.execute(&backend_step, workspace).await;

        match result {
            Ok(exit_code) => {
                let status = if exit_code == 0 {
                    RunStatus::Succeeded
                } else {
                    RunStatus::Failed
                };
                self.report_step_status(
                    &step.step_run_id,
                    &step.step_id,
                    status,
                    Some(exit_code),
                    None,
                )
                .await?;
                Ok(exit_code)
            }
            Err(e) => {
                self.report_step_status(
                    &step.step_run_id,
                    &step.step_id,
                    RunStatus::Failed,
                    None,
                    Some(e.to_string()),
                )
                .await?;
                Err(e)
            }
        }
    }

    /// Create a workspace directory for a job.
    async fn create_workspace(&self, job_run_id: &str) -> Result<PathBuf> {
        let workspace = self
            .config
            .workspace_dir
            .join(job_run_id);

        tokio::fs::create_dir_all(&workspace)
            .await
            .map_err(|e| AgentError::Workspace(format!("failed to create workspace: {e}")))?;

        debug!(workspace = %workspace.display(), "created workspace");

        Ok(workspace)
    }

    /// Cleanup workspace after job completion.
    async fn cleanup_workspace(&self, workspace: &Path) -> Result<()> {
        if workspace.exists() {
            tokio::fs::remove_dir_all(workspace)
                .await
                .map_err(|e| AgentError::Workspace(format!("failed to cleanup workspace: {e}")))?;
        }
        Ok(())
    }

    /// Exchange keys with controller for job secrets.
    async fn exchange_keys(
        &mut self,
        job_id: &str,
        pki: &JobPki,
    ) -> Result<HashMap<String, String>> {
        let request = JobKeyExchange {
            agent_id: self.identity.agent_id.clone(),
            job_id: job_id.to_string(),
            one_time_x509_public_key: pki.public_key_der(),
        };

        let response = self.client.exchange_job_keys(request).await?.into_inner();

        // Decrypt secrets
        let mut secrets = HashMap::new();
        for secret in response.secrets {
            // TODO: Implement actual decryption
            // For now, skip decryption
            warn!(key = %secret.key, "secret decryption not implemented");
        }

        Ok(secrets)
    }

    /// Report job status to controller.
    async fn report_job_status(
        &mut self,
        job_run_id: &str,
        status: RunStatus,
        exit_code: Option<i32>,
        error_message: Option<String>,
    ) -> Result<()> {
        let update = JobStatusUpdate {
            job_run_id: job_run_id.to_string(),
            status: status as i32,
            exit_code,
            error_message: error_message.unwrap_or_default(),
            timestamp: Some(Timestamp {
                seconds: Utc::now().timestamp(),
                nanos: 0,
            }),
        };

        // Would stream this in production
        debug!(job_run_id, status = ?status, "reported job status");

        Ok(())
    }

    /// Report step status to controller.
    async fn report_step_status(
        &mut self,
        step_run_id: &str,
        job_run_id: &str,
        status: RunStatus,
        exit_code: Option<i32>,
        error_message: Option<String>,
    ) -> Result<()> {
        let update = StepStatusUpdate {
            step_run_id: step_run_id.to_string(),
            job_run_id: job_run_id.to_string(),
            status: status as i32,
            exit_code,
            error_message: error_message.unwrap_or_default(),
            timestamp: Some(Timestamp {
                seconds: Utc::now().timestamp(),
                nanos: 0,
            }),
        };

        // Would stream this in production
        debug!(step_run_id, status = ?status, "reported step status");

        Ok(())
    }
}
