//! Job executor for running pipeline jobs.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use async_nats::jetstream::AckKind;
use futures::StreamExt;
use met_proto::agent::v1::{
    agent_service_client::AgentServiceClient, ExecutedBinary as ProtoExecutedBinary,
    JobExecutionMetadata as ProtoJobExecutionMetadata, JobKeyExchange, JobStatusUpdate,
    SecretMaterialKind, StepStatusUpdate,
};
use met_proto::common::v1::{RunStatus, Timestamp};
use prost::Message;
use sha2::{Digest, Sha256};
use tokio::sync::{mpsc, watch, RwLock};
use tokio_stream;
use tonic::transport::Channel;
use tonic::Request;
use tracing::{debug, error, info, instrument, warn};

use crate::backend::ExecutionBackend;
use crate::config::{AgentConfig, AgentIdentity};
use crate::error::{AgentError, Result};
use crate::heartbeat::HeartbeatState;
use crate::process_watcher::{merge_execution_metadata, JobExecutionMetadata, ProcessWatcher};
use crate::security::JobPki;
use crate::step_log::{step_log_spool_path, StepLogSession};

/// In-flight job identity for operator interrupt → controller cancellation.
#[derive(Clone)]
struct ActiveJobTrace {
    job_run_id: String,
    step_run_id: Option<String>,
}

/// Job executor that pulls jobs from NATS and executes them.
pub struct JobExecutor {
    config: AgentConfig,
    identity: AgentIdentity,
    client: AgentServiceClient<Channel>,
    backend: Arc<dyn ExecutionBackend>,
    heartbeat_state: Arc<RwLock<HeartbeatState>>,
    shutdown_rx: watch::Receiver<bool>,
    /// When true, stop pulling new jobs from NATS (controller requested drain).
    job_pause_rx: watch::Receiver<bool>,
    /// Nudge heartbeat after busy/idle transitions so the controller sees `busy` without waiting for the interval.
    heartbeat_transition_wake: mpsc::UnboundedSender<()>,
    /// Updated while a job is active (after Running is reported) for SIGINT cancellation.
    active_trace: Arc<RwLock<Option<ActiveJobTrace>>>,
    /// Step log `StreamLogs` flushes; must finish before workspace cleanup deletes spool files.
    pending_log_flushes: Vec<tokio::task::JoinHandle<()>>,
    /// Footprint / blast-radius metadata from finished steps (for cancel and early-abort reporting).
    footprint_accumulator: Arc<RwLock<Vec<(String, JobExecutionMetadata)>>>,
}

impl JobExecutor {
    /// Wait until `rx` becomes `true` (process shutdown requested).
    async fn wait_shutdown(rx: &watch::Receiver<bool>) {
        let mut r = rx.clone();
        if *r.borrow() {
            return;
        }
        loop {
            if r.changed().await.is_err() {
                return;
            }
            if *r.borrow() {
                return;
            }
        }
    }

    /// Create a new job executor.
    pub fn new(
        config: AgentConfig,
        identity: AgentIdentity,
        client: AgentServiceClient<Channel>,
        backend: Arc<dyn ExecutionBackend>,
        heartbeat_state: Arc<RwLock<HeartbeatState>>,
        shutdown_rx: watch::Receiver<bool>,
        job_pause_rx: watch::Receiver<bool>,
        heartbeat_transition_wake: mpsc::UnboundedSender<()>,
    ) -> Self {
        Self {
            config,
            identity,
            client,
            backend,
            heartbeat_state,
            shutdown_rx,
            job_pause_rx,
            heartbeat_transition_wake,
            active_trace: Arc::new(RwLock::new(None)),
            pending_log_flushes: Vec::new(),
            footprint_accumulator: Arc::new(RwLock::new(Vec::new())),
        }
    }

    async fn clear_footprint_accumulator(&self) {
        self.footprint_accumulator.write().await.clear();
    }

    async fn record_footprint_step(&self, step_id: String, meta: JobExecutionMetadata) {
        self.footprint_accumulator
            .write()
            .await
            .push((step_id, meta));
    }

    async fn merged_footprint_metadata(&self) -> Option<JobExecutionMetadata> {
        let acc = self.footprint_accumulator.read().await;
        if acc.is_empty() {
            return None;
        }
        Some(merge_execution_metadata(acc.clone()))
    }

    async fn join_pending_log_flushes(&mut self) {
        for h in self.pending_log_flushes.drain(..) {
            if let Err(e) = h.await {
                warn!(error = %e, "step log flush task panicked or was cancelled");
            }
        }
    }

    /// Run the executor loop (re-enters NATS pull after drain/resume).
    pub async fn run(mut self, nats_client: async_nats::Client) -> Result<()> {
        info!("starting job executor");
        let jetstream = async_nats::jetstream::new(nats_client);

        loop {
            while *self.job_pause_rx.borrow() {
                if *self.shutdown_rx.borrow() {
                    info!("executor shutting down");
                    return Ok(());
                }
                if self.job_pause_rx.changed().await.is_err() {
                    return Ok(());
                }
            }

            if *self.shutdown_rx.borrow() {
                info!("executor shutting down");
                return Ok(());
            }

            match self.run_pull_session(&jetstream).await {
                Ok(()) => {}
                Err(e) => {
                    error!(
                        error = %e,
                        "job pull session failed; retrying after delay (controller may be reconciling JetStream consumers)"
                    );
                    tokio::select! {
                        _ = tokio::time::sleep(Duration::from_secs(3)) => {}
                        _ = Self::wait_shutdown(&self.shutdown_rx) => {
                            info!("executor shutting down during NATS retry backoff");
                            return Ok(());
                        }
                    }
                }
            }

            if *self.shutdown_rx.borrow() {
                info!("executor shutting down");
                return Ok(());
            }
        }
    }

    /// One pull-consumer session until drain, shutdown, or stream end.
    async fn run_pull_session(
        &mut self,
        jetstream: &async_nats::jetstream::Context,
    ) -> Result<()> {
        let stream = match jetstream.get_stream("JOBS").await {
            Ok(s) => s,
            Err(e) => {
                error!(error = %e, "failed to get JOBS stream");
                return Err(AgentError::Nats(e.into()));
            }
        };

        let subject = self.identity.job_pull_filter_subject();
        let stored_first = self.identity.nats_subjects.first().cloned();
        if stored_first.as_deref() != Some(subject.as_str()) {
            warn!(
                stored = ?stored_first,
                derived = %subject,
                "job inbox filter derived from org_id+agent_id (stale nats_subjects in identity file are ignored for JetStream pull)"
            );
        }

        let consumer_name = format!("agent-{}", self.identity.agent_id);
        let pull_config = async_nats::jetstream::consumer::pull::Config {
            name: Some(consumer_name.clone()),
            durable_name: Some(consumer_name.clone()),
            filter_subject: subject.clone(),
            ack_policy: async_nats::jetstream::consumer::AckPolicy::Explicit,
            ack_wait: Duration::from_secs(30),
            max_deliver: 3,
            ..Default::default()
        };

        let consumer = stream
            .get_or_create_consumer(&consumer_name, pull_config)
            .await
            .map_err(|e| AgentError::Nats(e.into()))?;

        info!(subject = %subject, "subscribed to job dispatch");

        let mut messages = consumer
            .messages()
            .await
            .map_err(|e| AgentError::Nats(e.into()))?;

        if *self.job_pause_rx.borrow() {
            info!("draining: not pulling new jobs from NATS");
            return Ok(());
        }

        loop {
            tokio::select! {
                msg = messages.next() => {
                    match msg {
                        Some(Ok(message)) => {
                            match met_proto::controller::v1::JobDispatch::decode(message.payload.as_ref())
                            {
                                Ok(job) => {
                                    let job_run_id = job.job_run_id.clone();
                                    let shutdown_watcher = self.shutdown_rx.clone();
                                    tokio::select! {
                                        exec_res = self.execute_job(job) => {
                                            match exec_res {
                                                Ok(()) => {
                                                    if let Err(e) = message.ack().await {
                                                        warn!(error = %e, "failed to ack message after job success");
                                                    }
                                                }
                                                Err(e) => {
                                                    error!(error = %e, "job execution failed; NAK for redelivery");
                                                    let _ = message
                                                        .ack_with(AckKind::Nak(None))
                                                        .await;
                                                }
                                            }
                                        }
                                        _ = Self::wait_shutdown(&shutdown_watcher) => {
                                            self.report_interrupted_job_to_controller(&job_run_id)
                                                .await;
                                            info!(
                                                job_run_id = %job_run_id,
                                                "agent shutdown: dropping in-flight job (child processes use kill_on_drop)"
                                            );
                                            let _ = message.ack_with(AckKind::Nak(None)).await;
                                            return Ok(());
                                        }
                                    }
                                }
                                Err(e) => {
                                    warn!(error = %e, "failed to decode job dispatch; ACK to drop poison payload");
                                    let _ = message.ack().await;
                                }
                            }
                        }
                        Some(Err(e)) => {
                            error!(error = %e, "error receiving message");
                        }
                        None => {
                            info!("message stream ended");
                            return Ok(());
                        }
                    }
                }
                _ = self.shutdown_rx.changed() => {
                    if *self.shutdown_rx.borrow() {
                        info!("executor shutting down");
                        return Ok(());
                    }
                }
                _ = self.job_pause_rx.changed() => {
                    if *self.job_pause_rx.borrow() {
                        info!("draining: stopped pulling new jobs from NATS");
                        return Ok(());
                    }
                }
            }
        }
    }

    /// Report job (and current step, if any) cancelled when the operator stops the agent.
    async fn report_interrupted_job_to_controller(&mut self, job_run_id: &str) {
        const MSG: &str = "Agent shut down by operator (SIGINT)";
        let step_run_id = {
            let trace = self.active_trace.read().await;
            trace.as_ref().and_then(|t| {
                if t.job_run_id == job_run_id {
                    t.step_run_id.clone()
                } else {
                    None
                }
            })
        };

        if let Some(ref sid) = step_run_id {
            let _ = self
                .report_step_status(
                    sid,
                    job_run_id,
                    RunStatus::Cancelled,
                    None,
                    Some(MSG.to_string()),
                )
                .await;
        }

        self.join_pending_log_flushes().await;

        let merged = self.merged_footprint_metadata().await;
        let ws = self.config.workspace_dir.join(job_run_id);
        let sbom = maybe_read_workspace_sbom_cyclonedx(&ws).await;

        let _ = self
            .report_job_status(
                job_run_id,
                RunStatus::Cancelled,
                None,
                Some(MSG.to_string()),
                merged,
                sbom,
            )
            .await;

        self.release_job_heartbeat_slot().await;

        *self.active_trace.write().await = None;
    }

    async fn clear_step_trace_slot(&self, job_run_id: &str) {
        let mut g = self.active_trace.write().await;
        if let Some(t) = g.as_mut() {
            if t.job_run_id == job_run_id {
                t.step_run_id = None;
            }
        }
    }

    /// Clear busy heartbeat after a job slot was claimed (matches increment in `run_job_dispatch`).
    async fn release_job_heartbeat_slot(&self) {
        let mut state = self.heartbeat_state.write().await;
        state.running_jobs = state.running_jobs.saturating_sub(1);
        state.current_job_id = None;
        if state.running_jobs == 0 {
            state.status = met_proto::AgentStatus::Online;
        }
        drop(state);
        let _ = self.heartbeat_transition_wake.send(());
    }

    /// Execute a single job.
    #[instrument(skip(self, job), fields(job_run_id = %job.job_run_id))]
    async fn execute_job(&mut self, job: met_proto::controller::v1::JobDispatch) -> Result<()> {
        let out = self.run_job_dispatch(job).await;
        *self.active_trace.write().await = None;
        out
    }

    async fn run_job_dispatch(&mut self, job: met_proto::controller::v1::JobDispatch) -> Result<()> {
        info!(
            job_name = %job.job_name,
            pipeline = %job.pipeline_name,
            steps = job.steps.len(),
            "executing job"
        );

        if crate::job_claim::job_successfully_completed(&self.config, &job.job_run_id).await {
            info!(
                job_run_id = %job.job_run_id,
                "skipping job: already completed successfully on this agent (idempotent)"
            );
            return Ok(());
        }

        // Update heartbeat state — any early error must still clear this (see `release_job_heartbeat_slot` below).
        {
            let mut state = self.heartbeat_state.write().await;
            state.running_jobs += 1;
            state.current_job_id = Some(job.job_run_id.clone());
            state.status = met_proto::AgentStatus::Busy;
        }
        let _ = self.heartbeat_transition_wake.send(());

        let job_run_id = job.job_run_id.clone();
        let res = self.do_run_job_dispatch(job).await;
        self.release_job_heartbeat_slot().await;
        match res {
            Ok(()) => Ok(()),
            Err(e) => {
                let msg = format!("{e}");
                error!(
                    job_run_id = %job_run_id,
                    error = %msg,
                    "job aborted before completion; reporting failed to controller"
                );
                let merged = self.merged_footprint_metadata().await;
                let ws = self.config.workspace_dir.join(&job_run_id);
                let sbom = maybe_read_workspace_sbom_cyclonedx(&ws).await;
                let _ = self
                    .report_job_status(
                        &job_run_id,
                        RunStatus::Failed,
                        Some(1),
                        Some(msg),
                        merged,
                        sbom,
                    )
                    .await;
                Ok(())
            }
        }
    }

    async fn do_run_job_dispatch(&mut self, job: met_proto::controller::v1::JobDispatch) -> Result<()> {
        if !self.pending_log_flushes.is_empty() {
            warn!(
                count = self.pending_log_flushes.len(),
                "draining leftover step log flush handle(s) before starting job"
            );
            self.join_pending_log_flushes().await;
        }

        self.clear_footprint_accumulator().await;

        // Report job accepted
        self.report_job_status(
            &job.job_run_id,
            RunStatus::Running,
            None,
            None,
            None,
            None,
        )
            .await?;

        *self.active_trace.write().await = Some(ActiveJobTrace {
            job_run_id: job.job_run_id.clone(),
            step_run_id: None,
        });

        // Create workspace
        let workspace = self.create_workspace(&job.job_run_id).await?;

        let job_result = self.run_job_in_workspace(&job, &workspace).await;

        self.join_pending_log_flushes().await;

        if let Err(e) = self.cleanup_workspace(&workspace).await {
            warn!(
                error = %e,
                job_run_id = %job.job_run_id,
                "workspace cleanup failed (one-time job directory may remain on disk)"
            );
        }

        job_result
    }

    /// Job lifecycle after the per-run workspace directory exists; workspace is removed by the caller.
    async fn run_job_in_workspace(
        &mut self,
        job: &met_proto::controller::v1::JobDispatch,
        workspace: &std::path::Path,
    ) -> Result<()> {
        // Generate per-job PKI and exchange keys
        let pki = JobPki::generate().map_err(AgentError::Certificate)?;

        let secrets = if job.requires_secret_exchange {
            self.exchange_keys(job, &pki, workspace).await?
        } else {
            HashMap::new()
        };

        // Collect execution metadata from all steps
        let mut step_metadata: Vec<(String, JobExecutionMetadata)> = Vec::new();

        // Execute steps sequentially
        let mut job_success = true;
        let mut last_exit_code: Option<i32> = None;
        for step in &job.steps {
            match self
                .execute_step(&job.job_run_id, step, workspace, &job.variables, &secrets)
                .await
            {
                Ok((exit_code, metadata)) => {
                    last_exit_code = Some(exit_code);
                    if let Some(meta) = metadata {
                        step_metadata.push((step.step_id.clone(), meta.clone()));
                        self.record_footprint_step(step.step_id.clone(), meta)
                            .await;
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

        let sbom_json = maybe_read_workspace_sbom_cyclonedx(workspace).await;

        // Report job completion with execution metadata
        let final_status = if job_success {
            RunStatus::Succeeded
        } else {
            RunStatus::Failed
        };
        let job_exit = if job_success {
            Some(0)
        } else {
            last_exit_code.or(Some(1))
        };
        self.report_job_status(
            &job.job_run_id,
            final_status,
            job_exit,
            None,
            job_metadata,
            sbom_json,
        )
        .await?;

        if job_success {
            if let Err(e) =
                crate::job_claim::record_job_successful_completion(&self.config, &job.job_run_id)
                    .await
            {
                warn!(error = %e, "failed to persist local job completion idempotency marker");
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
        job_run_id: &str,
        step: &met_proto::controller::v1::StepSpec,
        workspace: &Path,
        variables: &HashMap<String, String>,
        secrets: &HashMap<String, String>,
    ) -> Result<(i32, Option<JobExecutionMetadata>)> {
        info!(step = %step.name, "executing step");

        let step_run_id = step.step_run_id.as_str();

        let spool_path = step_log_spool_path(workspace, &step.step_run_id);
        let log_session = StepLogSession::spawn(
            self.client.clone(),
            job_run_id.to_string(),
            step.step_run_id.clone(),
            spool_path,
            128,
            128,
        )?;
        let log_pipe = log_session
            .pipe()
            .ok_or_else(|| AgentError::Internal("step log pipe not initialized".to_string()))?;
        info!(step = %step.name, "step log stream pipeline started");

        // Report step started
        self.report_step_status(step_run_id, job_run_id, RunStatus::Running, None, None)
            .await?;
        info!(step = %step.name, "reported step Running to controller");

        {
            let mut g = self.active_trace.write().await;
            if let Some(t) = g.as_mut() {
                if t.job_run_id == job_run_id {
                    t.step_run_id = Some(step.step_run_id.clone());
                }
            }
        }

        // Build environment
        let mut env = step.environment.clone();
        env.extend(variables.clone());
        env.extend(secrets.clone());
        if !env.contains_key("METICULOUS_WORKSPACE") {
            let ws = if self.backend.name() == "container" {
                "/workspace".to_string()
            } else {
                tokio::fs::canonicalize(workspace)
                    .await
                    .unwrap_or_else(|_| workspace.to_path_buf())
                    .to_string_lossy()
                    .into_owned()
            };
            env.insert("METICULOUS_WORKSPACE".into(), ws);
        }

        // Convert to backend step spec
        let backend_step = crate::backend::StepSpec {
            step_id: step.step_id.clone(),
            step_run_id: step.step_run_id.clone(),
            step_sequence: step.sequence,
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

        info!(
            step = %step.name,
            image = %backend_step.image,
            "invoking execution backend (container/native)"
        );

        // Execute with process watching and live log shipping
        let result = self
            .backend
            .execute_with_watcher(
                &backend_step,
                workspace,
                &mut watcher,
                Some(&log_pipe),
            )
            .await;

        // Report terminal step status before awaiting log drain: `finish()` waits on `stream_logs`
        // and must not delay controller updates or heartbeat release.
        match &result {
            Ok(step_result) => {
                let exit_code = step_result.exit_code;
                let status = if exit_code == 0 {
                    RunStatus::Succeeded
                } else {
                    RunStatus::Failed
                };
                self.report_step_status(
                    step_run_id,
                    job_run_id,
                    status,
                    Some(exit_code),
                    None,
                )
                .await?;
            }
            Err(e) => {
                self.report_step_status(
                    step_run_id,
                    job_run_id,
                    RunStatus::Failed,
                    None,
                    Some(e.to_string()),
                )
                .await?;
            }
        }
        self.clear_step_trace_slot(job_run_id).await;

        // Flush logs in the background; `join_pending_log_flushes` runs before workspace deletion.
        let step_run_for_log = step.step_run_id.clone();
        let flush = tokio::spawn(async move {
            if let Err(log_err) = log_session.finish().await {
                warn!(
                    error = %log_err,
                    step_run_id = %step_run_for_log,
                    "step log pipeline flush failed after status was reported to controller"
                );
            }
        });
        self.pending_log_flushes.push(flush);

        match result {
            Ok(step_result) => {
                let mut metadata = JobExecutionMetadata {
                    executed_binaries: step_result.executed_binaries,
                    total_processes_spawned: step_result.processes_spawned,
                    execution_tree_depth: step_result.execution_tree_depth,
                };
                crate::script_exec_hints::merge_command_hints_into_metadata(
                    &step.command,
                    &step.step_id,
                    &step.step_run_id,
                    &mut metadata,
                );
                Ok((step_result.exit_code, Some(metadata)))
            }
            Err(e) => {
                let meta = watcher
                    .aggregate_metadata(&step.step_id, &step.step_run_id)
                    .await;
                error!(step = %step.name, error = %e, "step backend error; still shipping footprint metadata");
                Ok((1, Some(meta)))
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
        job: &met_proto::controller::v1::JobDispatch,
        pki: &JobPki,
        workspace: &Path,
    ) -> Result<HashMap<String, String>> {
        let request = JobKeyExchange {
            agent_id: self.identity.agent_id.clone(),
            job_id: job.job_run_id.clone(),
            one_time_x509_public_key: pki.x25519_public_key().to_vec(),
            org_id: job.org_id.clone(),
            project_id: job.project_id.clone(),
            pipeline_id: job.pipeline_id.clone(),
            secret_resolution_hints_json: job.secret_resolution_hints_json.clone(),
        };

        let response = self.client.exchange_job_keys(request).await?.into_inner();

        // Decrypt secrets using X25519 + AES-256-GCM; plaintext HMAC key is ECDH-derived (see met_secrets).
        let mut secrets = HashMap::new();
        for secret in response.secrets {
            match pki.decrypt(&secret.encrypted_value) {
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

                    let is_file = secret.material_kind == SecretMaterialKind::WorkspaceFilePath as i32;

                    if is_file {
                        let secrets_dir = workspace.join(".meticulous").join("secrets");
                        tokio::fs::create_dir_all(&secrets_dir)
                            .await
                            .map_err(|e| AgentError::Workspace(format!("secrets dir: {e}")))?;
                        let safe_name: String = secret
                            .key
                            .chars()
                            .map(|c| if c.is_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
                            .collect();
                        let path = secrets_dir.join(safe_name);
                        tokio::fs::write(&path, value.as_bytes())
                            .await
                            .map_err(|e| AgentError::Workspace(format!("write secret file: {e}")))?;
                        #[cfg(unix)]
                        {
                            use std::os::unix::fs::PermissionsExt;
                            let _ = tokio::fs::set_permissions(
                                &path,
                                std::fs::Permissions::from_mode(0o600),
                            )
                            .await;
                        }
                        let abs = path
                            .canonicalize()
                            .unwrap_or(path)
                            .to_string_lossy()
                            .into_owned();
                        debug!(key = %secret.key, path = %abs, "decrypted secret materialized as file");
                        secrets.insert(secret.key, abs);
                    } else {
                        debug!(key = %secret.key, "decrypted and verified secret");
                        secrets.insert(secret.key, value);
                    }
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
            job_id = %job.job_run_id,
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
        sbom_cyclonedx_json: Option<String>,
    ) -> Result<()> {
        // Convert execution metadata to protobuf format
        let proto_metadata = execution_metadata.map(|meta| {
            let unique_binaries_count = meta.executed_binaries.len() as u32;
            ProtoJobExecutionMetadata {
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
                        step_run_ids: b.step_run_ids,
                    })
                    .collect(),
                total_processes_spawned: meta.total_processes_spawned,
                execution_tree_depth: meta.execution_tree_depth,
                unique_binaries_count,
            }
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
            agent_id: Some(self.identity.agent_id.clone()),
            sbom_cyclonedx_json,
        };

        self.client
            .report_job_status(Request::new(tokio_stream::iter(vec![update])))
            .await?;

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

        self.client
            .report_step_status(Request::new(tokio_stream::iter(vec![update])))
            .await?;

        Ok(())
    }
}

/// CycloneDX JSON at the job workspace root (written by workflows such as `git-clone-snippet`).
const WORKSPACE_SBOM_CYCLONEDX_FILENAME: &str = "sbom.cdx.json";
const MAX_SBOM_INLINE_BYTES: u64 = 6 * 1024 * 1024;

async fn maybe_read_workspace_sbom_cyclonedx(workspace: &Path) -> Option<String> {
    let path = workspace.join(WORKSPACE_SBOM_CYCLONEDX_FILENAME);
    let meta = tokio::fs::metadata(&path).await.ok()?;
    if !meta.is_file() || meta.len() > MAX_SBOM_INLINE_BYTES {
        if meta.is_file() && meta.len() > MAX_SBOM_INLINE_BYTES {
            warn!(
                path = %path.display(),
                len = meta.len(),
                max = MAX_SBOM_INLINE_BYTES,
                "sbom file too large to inline on job status; skipping controller ingest"
            );
        }
        return None;
    }
    let raw = tokio::fs::read_to_string(&path).await.ok()?;
    if raw.trim().is_empty() {
        return None;
    }
    if serde_json::from_str::<serde_json::Value>(&raw)
        .ok()
        .filter(|v| v.is_object())
        .is_none()
    {
        warn!(
            path = %path.display(),
            "sbom file is not valid JSON object; skipping controller ingest"
        );
        return None;
    }
    Some(raw)
}
