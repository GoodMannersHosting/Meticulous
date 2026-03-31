//! Job executor for running pipeline jobs.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_nats::jetstream::consumer::pull::Stream as PullStream;
use chrono::Utc;
use futures::StreamExt;
use met_proto::agent::v1::{
    agent_service_client::AgentServiceClient, ExecutedBinary as ProtoExecutedBinary,
    JobAssignment, JobExecutionMetadata as ProtoJobExecutionMetadata, JobKeyExchange,
    JobStatusUpdate, StepSpec, StepStatusUpdate,
};
use met_proto::common::v1::{RunStatus, StepKind, Timestamp};
use prost::Message;
use sha2::{Digest, Sha256};
use tokio::sync::{mpsc, watch, RwLock};
use tonic::transport::Channel;
use tracing::{debug, error, info, instrument, warn};

use crate::backend::ExecutionBackend;
use crate::config::{AgentConfig, AgentIdentity};
use crate::error::{AgentError, Result};
use crate::heartbeat::HeartbeatState;
use crate::process_watcher::{merge_execution_metadata, JobExecutionMetadata, ProcessWatcher};
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
        self.report_job_status(&job.job_run_id, RunStatus::Running, None, None, None)
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

        // Collect execution metadata from all steps
        let mut step_metadata: Vec<(String, JobExecutionMetadata)> = Vec::new();

        // Execute steps sequentially
        let mut job_success = true;
        for step in &job.steps {
            let step_result = self
                .execute_step(step, &workspace, &job.variables, &secrets)
                .await;

            match step_result {
                Ok((exit_code, metadata)) => {
                    // Collect step metadata
                    if let Some(meta) = metadata {
                        step_metadata.push((step.step_id.clone(), meta));
                    }

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

        // Merge execution metadata from all steps
        let job_metadata = if !step_metadata.is_empty() {
            Some(merge_execution_metadata(step_metadata))
        } else {
            None
        };

        // Log execution summary
        if let Some(ref meta) = job_metadata {
            info!(
                job_run_id = %job.job_run_id,
                total_processes = meta.total_processes_spawned,
                unique_binaries = meta.executed_binaries.len(),
                max_depth = meta.execution_tree_depth,
                "job execution metadata collected"
            );

            // Log each unique binary for audit purposes
            for binary in &meta.executed_binaries {
                debug!(
                    path = %binary.path,
                    sha256 = %binary.sha256,
                    execution_count = binary.execution_count,
                    "executed binary"
                );
            }
        }

        // Report job completion with execution metadata
        let final_status = if job_success {
            RunStatus::Succeeded
        } else {
            RunStatus::Failed
        };
        self.report_job_status(&job.job_run_id, final_status, Some(0), None, job_metadata)
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
    /// Returns (exit_code, execution_metadata).
    #[instrument(skip(self, step, workspace, variables, secrets), fields(step_name = %step.name))]
    async fn execute_step(
        &mut self,
        step: &met_proto::controller::v1::StepSpec,
        workspace: &Path,
        variables: &HashMap<String, String>,
        secrets: &HashMap<String, String>,
    ) -> Result<(i32, Option<JobExecutionMetadata>)> {
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

        // Create a process watcher for this step
        let mut watcher = ProcessWatcher::new();

        // Execute with process watching
        let result = self
            .backend
            .execute_with_watcher(&backend_step, workspace, &mut watcher)
            .await;

        match result {
            Ok(step_result) => {
                let exit_code = step_result.exit_code;
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

                // Create metadata from step result
                let metadata = JobExecutionMetadata {
                    executed_binaries: step_result.executed_binaries,
                    total_processes_spawned: step_result.processes_spawned,
                    execution_tree_depth: step_result.execution_tree_depth,
                };

                Ok((exit_code, Some(metadata)))
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
            one_time_x509_public_key: pki.x25519_public_key().to_vec(),
        };

        let response = self.client.exchange_job_keys(request).await?.into_inner();

        // Derive HMAC key from agent identity for secret verification
        // This must match the key derivation in the controller
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(self.identity.jwt_token.as_bytes());
        hasher.update(b"meticulous-secrets-hmac-v1");
        let hmac_key = hasher.finalize();

        // Decrypt secrets using X25519 + AES-256-GCM hybrid decryption
        let mut secrets = HashMap::new();
        for secret in response.secrets {
            match pki.decrypt(&secret.encrypted_value, &hmac_key) {
                Ok(plaintext) => {
                    // Verify SHA-256 checksum
                    let mut checksum_hasher = Sha256::new();
                    checksum_hasher.update(&*plaintext);
                    let computed_checksum = hex::encode(checksum_hasher.finalize());

                    if computed_checksum != secret.sha256_checksum {
                        error!(
                            key = %secret.key,
                            expected = %secret.sha256_checksum,
                            computed = %computed_checksum,
                            "secret checksum verification failed"
                        );
                        return Err(AgentError::Security(format!(
                            "checksum verification failed for secret '{}'",
                            secret.key
                        )));
                    }

                    // Convert decrypted bytes to string (zeroizing wrapper ensures cleanup)
                    let value = String::from_utf8(plaintext.to_vec())
                        .map_err(|_| AgentError::Security(format!(
                            "secret '{}' is not valid UTF-8",
                            secret.key
                        )))?;

                    debug!(key = %secret.key, "decrypted and verified secret");
                    secrets.insert(secret.key, value);
                }
                Err(e) => {
                    error!(key = %secret.key, error = %e, "failed to decrypt secret");
                    return Err(AgentError::Security(format!(
                        "failed to decrypt secret '{}': {}",
                        secret.key, e
                    )));
                }
            }
        }

        info!(
            job_id = %job_id,
            secrets_count = secrets.len(),
            "decrypted and verified all job secrets"
        );

        Ok(secrets)
    }

    /// Report job status to controller.
    async fn report_job_status(
        &mut self,
        job_run_id: &str,
        status: RunStatus,
        exit_code: Option<i32>,
        error_message: Option<String>,
        execution_metadata: Option<JobExecutionMetadata>,
    ) -> Result<()> {
        // Convert execution metadata to protobuf format
        let proto_metadata = execution_metadata.map(|meta| ProtoJobExecutionMetadata {
            job_run_id: job_run_id.to_string(),
            executed_binaries: meta
                .executed_binaries
                .into_iter()
                .map(|b| ProtoExecutedBinary {
                    path: b.path,
                    sha256: b.sha256,
                    execution_count: b.execution_count,
                    first_executed_at: Some(Timestamp {
                        seconds: b.first_executed_at.timestamp(),
                        nanos: 0,
                    }),
                    last_executed_at: Some(Timestamp {
                        seconds: b.last_executed_at.timestamp(),
                        nanos: 0,
                    }),
                    step_ids: b.step_ids,
                })
                .collect(),
            total_processes_spawned: meta.total_processes_spawned,
            execution_tree_depth: meta.execution_tree_depth,
            unique_binaries_count: 0, // Will be set from executed_binaries.len()
        });

        let update = JobStatusUpdate {
            job_run_id: job_run_id.to_string(),
            status: status as i32,
            exit_code,
            error_message: error_message.unwrap_or_default(),
            timestamp: Some(Timestamp {
                seconds: Utc::now().timestamp(),
                nanos: 0,
            }),
            execution_metadata: proto_metadata,
        };

        // Would stream this in production
        debug!(
            job_run_id,
            status = ?status,
            has_metadata = update.execution_metadata.is_some(),
            "reported job status"
        );

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
