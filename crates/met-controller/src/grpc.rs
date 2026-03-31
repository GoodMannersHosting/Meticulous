//! gRPC service implementation for agent communication.

use std::sync::Arc;
use std::time::Instant;

use chrono::{TimeZone, Utc};
use met_core::hash_join_token;
use met_core::ids::{AgentId, JobRunId, OrganizationId, RunId, StepRunId};
use met_core::models::{
    Agent, AgentHeartbeat, AgentStatus, EnvironmentType, JobStatus, JoinTokenScope,
};
use met_store::StoreError;
use met_proto::agent::v1::{
    agent_service_server::AgentService, DeregisterRequest, DeregisterResponse,
    EncryptedSecretValue, HeartbeatAction, HeartbeatRequest, HeartbeatResponse, JobKeyExchange,
    JobSecretsPayload, JobStatusAck, JobStatusUpdate, LogAck, LogChunk, RegisterRequest,
    RegisterResponse, StepStatusAck, StepStatusUpdate,
};
use met_proto::common::v1::RunStatus as ProtoRunStatus;
use met_secrets::pki::encryption::HybridEncryption;
use met_objstore::ObjectStore;
use met_store::repos::{
    AgentHeartbeatRepo, AgentRepo, JobRunRepo, JoinTokenRepo, LogCacheRepo, StepRunRepo,
};
use met_store::PgPool;
use sha2::{Digest, Sha256};
use tokio_stream::StreamExt;
use tonic::{Request, Response, Status, Streaming};
use tracing::{debug, error, info, instrument, warn};

use crate::config::ControllerConfig;
use crate::log_archive::finalize_job_logs;
use crate::error::ControllerError;
use crate::jwt::JwtManager;
use crate::nats::NatsDispatcher;
use crate::registry::{agent_state_from_db_row, AgentRegistry, AgentState, ResourceSnapshot};

/// gRPC implementation of the AgentService.
pub struct AgentServiceImpl {
    config: ControllerConfig,
    pool: Arc<PgPool>,
    registry: AgentRegistry,
    jwt: JwtManager,
    nats: NatsDispatcher,
    /// HMAC key for secret envelope verification
    secrets_hmac_key: Vec<u8>,
    /// Optional object store for log archival (SeaweedFS / S3).
    object_store: Option<Arc<dyn ObjectStore + Send + Sync>>,
}

impl AgentServiceImpl {
    /// Create a new AgentService implementation.
    pub fn new(
        config: ControllerConfig,
        pool: Arc<PgPool>,
        registry: AgentRegistry,
        nats: NatsDispatcher,
        object_store: Option<Arc<dyn ObjectStore + Send + Sync>>,
    ) -> Self {
        let jwt = JwtManager::new(
            &config.jwt_secret,
            config.jwt_validity,
            config.jwt_renewable,
        );

        // Derive HMAC key from JWT secret for secret envelope verification
        let mut hasher = Sha256::new();
        hasher.update(config.jwt_secret.as_bytes());
        hasher.update(b"meticulous-secrets-hmac-v1");
        let secrets_hmac_key = hasher.finalize().to_vec();

        Self {
            config,
            pool,
            registry,
            jwt,
            nats,
            secrets_hmac_key,
            object_store,
        }
    }

    /// Convert proto RunStatus to model JobStatus.
    fn convert_run_status(proto: ProtoRunStatus) -> JobStatus {
        match proto {
            ProtoRunStatus::Pending => JobStatus::Pending,
            ProtoRunStatus::Queued => JobStatus::Queued,
            ProtoRunStatus::Running => JobStatus::Running,
            ProtoRunStatus::Succeeded => JobStatus::Succeeded,
            ProtoRunStatus::Failed => JobStatus::Failed,
            ProtoRunStatus::Cancelled => JobStatus::Cancelled,
            ProtoRunStatus::TimedOut => JobStatus::TimedOut,
            ProtoRunStatus::Skipped => JobStatus::Skipped,
            ProtoRunStatus::Unspecified => JobStatus::Pending,
        }
    }

    /// Validate the security bundle from the agent.
    fn validate_security_bundle(
        &self,
        bundle: &met_proto::agent::v1::SecurityBundle,
    ) -> Result<(), ControllerError> {
        // Check NTP synchronization
        if self.config.require_ntp_sync && !bundle.ntp_synchronized {
            return Err(ControllerError::NtpNotSynchronized);
        }

        // Check OS/arch if restrictions are configured
        if !self.config.allowed_platforms.is_empty() {
            let platform = format!("{}/{}", bundle.os, bundle.arch);
            if !self.config.allowed_platforms.contains(&platform) {
                return Err(ControllerError::ValidationFailed(format!(
                    "platform {platform} not allowed"
                )));
            }
        }

        Ok(())
    }

    /// Convert proto environment type to model.
    fn convert_environment_type(
        &self,
        proto: met_proto::agent::v1::EnvironmentType,
    ) -> EnvironmentType {
        match proto {
            met_proto::agent::v1::EnvironmentType::Physical => EnvironmentType::Physical,
            met_proto::agent::v1::EnvironmentType::Virtual => EnvironmentType::Virtual,
            met_proto::agent::v1::EnvironmentType::Container => EnvironmentType::Container,
            met_proto::agent::v1::EnvironmentType::Unspecified => EnvironmentType::Virtual,
        }
    }

    /// Fetch secrets required for a job from the secrets provider.
    /// In production, this would query the job configuration and resolve
    /// secret references from vault, AWS Secrets Manager, etc.
    async fn fetch_job_secrets(&self, _job_id: &str) -> Vec<(String, String)> {
        // Placeholder - in production this would:
        // 1. Look up job configuration from database
        // 2. Extract secret references from the job spec
        // 3. Resolve each secret from the appropriate provider
        Vec::new()
    }
}

#[tonic::async_trait]
impl AgentService for AgentServiceImpl {
    #[instrument(skip(self, request), fields(join_token = "***"))]
    async fn register(
        &self,
        request: Request<RegisterRequest>,
    ) -> Result<Response<RegisterResponse>, Status> {
        let req = request.into_inner();

        info!("agent registration request received");

        if req.join_token.is_empty() {
            return Err(Status::invalid_argument("join_token required"));
        }

        let join_repo = JoinTokenRepo::new(&self.pool);
        let token_hash = hash_join_token(&req.join_token);
        let join_record = join_repo
            .validate_and_consume(&token_hash)
            .await
            .map_err(|e| match e {
                StoreError::NotFound { .. } => {
                    Status::unauthenticated("invalid or unknown join token")
                }
                StoreError::Constraint(_) => Status::unauthenticated(
                    "join token is expired, revoked, or has reached max uses",
                ),
                _ => Status::internal(e.to_string()),
            })?;

        let org_id = match join_record.scope {
            JoinTokenScope::Tenant => {
                let Some(scope_uuid) = join_record.scope_id else {
                    return Err(Status::failed_precondition(
                        "tenant join token is missing organization scope",
                    ));
                };
                OrganizationId::from_uuid(scope_uuid)
            }
            JoinTokenScope::Platform | JoinTokenScope::Project | JoinTokenScope::Pipeline => {
                return Err(Status::invalid_argument(
                    "only tenant-scoped join tokens are supported for agent registration",
                ));
            }
        };

        let caps_pool = req
            .capabilities
            .as_ref()
            .map(|c| c.pool_tags.clone())
            .unwrap_or_default();
        let caps_labels = req
            .capabilities
            .as_ref()
            .map(|c| c.labels.clone())
            .unwrap_or_default();

        let mut pool_tags = join_record.pool_tags.clone();
        for t in caps_pool {
            if !pool_tags.contains(&t) {
                pool_tags.push(t);
            }
        }
        if pool_tags.is_empty() {
            pool_tags.push("_default".to_string());
        }

        let mut labels = join_record.labels.clone();
        for t in caps_labels {
            if !labels.contains(&t) {
                labels.push(t);
            }
        }

        // Validate security bundle
        let bundle = req
            .security_bundle
            .as_ref()
            .ok_or_else(|| Status::invalid_argument("security_bundle required"))?;

        self.validate_security_bundle(bundle)
            .map_err(|e| Status::from(e))?;

        // Create agent record
        let agent_id = AgentId::new();
        let caps = req.capabilities.as_ref();

        let mut agent = Agent::new(
            org_id,
            &bundle.hostname,
            caps.map(|c| c.os.as_str()).unwrap_or(&bundle.os),
            caps.map(|c| c.arch.as_str()).unwrap_or(&bundle.arch),
            env!("CARGO_PKG_VERSION"),
        );

        agent.id = agent_id;
        agent.status = AgentStatus::Online;
        agent.tags = labels.clone();
        agent.environment_type = self.convert_environment_type(
            met_proto::agent::v1::EnvironmentType::try_from(bundle.environment_type)
                .unwrap_or(met_proto::agent::v1::EnvironmentType::Virtual),
        );
        agent.kernel_version = Some(bundle.kernel_version.clone()).filter(|s| !s.is_empty());
        agent.public_ips = bundle.public_ips.clone();
        agent.private_ips = bundle.private_ips.clone();
        agent.ntp_synchronized = bundle.ntp_synchronized;
        agent.container_runtime =
            Some(bundle.container_runtime.clone()).filter(|s| !s.is_empty());
        agent.container_runtime_version =
            Some(bundle.container_runtime_version.clone()).filter(|s| !s.is_empty());
        agent.x509_public_key = Some(bundle.agent_x509_public_key.clone())
            .filter(|b| !b.is_empty());
        agent.join_token_id = Some(join_record.id);
        agent.last_heartbeat_at = Some(Utc::now());

        // Issue JWT
        let (jwt_token, jwt_expires_at) = self
            .jwt
            .issue(agent_id, org_id, pool_tags.clone())
            .map_err(|e| Status::internal(e.to_string()))?;

        agent.jwt_expires_at = Some(jwt_expires_at);

        // Save to database
        let repo = AgentRepo::new(&self.pool);
        let agent = repo
            .register(&agent)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        // Add to registry
        let state = AgentState {
            agent_id,
            org_id,
            status: AgentStatus::Online,
            last_heartbeat: Instant::now(),
            last_heartbeat_at: Utc::now(),
            os: agent.os.clone(),
            arch: agent.arch.clone(),
            pool_tags: pool_tags.clone(),
            labels,
            max_jobs: agent.max_jobs,
            running_jobs: 0,
            current_job: None,
            jwt_expires_at,
            resources: None,
        };
        self.registry.register(state).await;

        // Build NATS subjects for agent to subscribe
        let nats_subjects: Vec<String> = pool_tags
            .iter()
            .map(|tag| crate::nats::subjects::job_dispatch(org_id, tag))
            .chain(std::iter::once(crate::nats::subjects::broadcast(org_id)))
            .collect();

        info!(
            agent_id = %agent_id,
            org_id = %org_id,
            hostname = %bundle.hostname,
            "agent registered successfully"
        );

        Ok(Response::new(RegisterResponse {
            agent_id: agent_id.to_string(),
            jwt_token,
            jwt_expires_at: Some(met_proto::common::v1::Timestamp {
                seconds: jwt_expires_at.timestamp(),
                nanos: 0,
            }),
            renewable: self.config.jwt_renewable,
            nats_subjects,
            nats_credentials: Some(met_proto::agent::v1::NatsCredentials {
                url: self.config.nats_url.clone(),
                jwt: String::new(), // Would be populated with NATS-specific JWT
                nkey_seed: String::new(),
            }),
            heartbeat_interval_secs: self.config.heartbeat_interval.as_secs() as i32,
        }))
    }

    #[instrument(skip(self, request), fields(agent_id))]
    async fn heartbeat(
        &self,
        request: Request<HeartbeatRequest>,
    ) -> Result<Response<HeartbeatResponse>, Status> {
        let req = request.into_inner();

        let agent_id: AgentId = req
            .agent_id
            .parse()
            .map_err(|_| Status::invalid_argument("invalid agent_id"))?;

        tracing::Span::current().record("agent_id", &agent_id.to_string());

        // Parse status
        let status = req
            .status
            .as_ref()
            .map(|s| {
                match met_proto::common::v1::AgentStatus::try_from(s.status) {
                    Ok(met_proto::common::v1::AgentStatus::Online) => AgentStatus::Online,
                    Ok(met_proto::common::v1::AgentStatus::Busy) => AgentStatus::Busy,
                    Ok(met_proto::common::v1::AgentStatus::Draining) => AgentStatus::Draining,
                    Ok(met_proto::common::v1::AgentStatus::Offline) => AgentStatus::Offline,
                    _ => AgentStatus::Online,
                }
            })
            .unwrap_or(AgentStatus::Online);

        let running_jobs = req.status.as_ref().map(|s| s.running_jobs).unwrap_or(0);

        let current_job = req.current_job_id.and_then(|id| id.parse().ok());

        let resources = req.resources.map(|r| ResourceSnapshot {
            cpu_percent: r.cpu_percent,
            memory_percent: r.memory_percent,
            disk_percent: r.disk_percent,
        });

        // Update registry (may be empty after controller restart — rehydrate from DB)
        let mut updated = self
            .registry
            .heartbeat(agent_id, status, running_jobs, current_job, resources.clone())
            .await;

        if updated.is_none() {
            let repo = AgentRepo::new(&self.pool);
            match repo.get(agent_id).await {
                Ok(agent) => {
                    let state = agent_state_from_db_row(&agent);
                    if self.registry.register_if_missing(state).await {
                        info!(
                            agent_id = %agent_id,
                            "rehydrated agent into registry after heartbeat miss (e.g. controller restarted)"
                        );
                    }
                    updated = self
                        .registry
                        .heartbeat(agent_id, status, running_jobs, current_job, resources.clone())
                        .await;
                }
                Err(_) => return Err(Status::not_found("agent not found")),
            }
        }

        let updated = updated.ok_or_else(|| Status::not_found("agent not found"))?;

        // Update database heartbeat
        let repo = AgentRepo::new(&self.pool);
        repo.heartbeat(agent_id, status, running_jobs)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        // Record heartbeat for diagnostics
        let heartbeat_record = AgentHeartbeat::new(agent_id, status);
        let heartbeat_record = if let Some(r) = &resources {
            heartbeat_record.with_resources(r.cpu_percent, r.memory_percent, r.disk_percent)
        } else {
            heartbeat_record
        };
        let heartbeat_record = if let Some(job_id) = current_job {
            heartbeat_record.with_current_job(job_id.as_uuid())
        } else {
            heartbeat_record
        };

        let heartbeat_repo = AgentHeartbeatRepo::new(&self.pool);
        if let Err(e) = heartbeat_repo.record(&heartbeat_record).await {
            warn!(error = %e, "failed to record heartbeat");
        }

        // Check for JWT renewal
        // We'd need to validate the JWT from request metadata in production
        let mut response = HeartbeatResponse {
            action: HeartbeatAction::Continue.into(),
            config_patch: None,
            new_jwt_token: None,
            new_jwt_expires_at: None,
        };

        // Check if agent should be drained (e.g., if revoked)
        if updated.status == AgentStatus::Revoked {
            response.action = HeartbeatAction::Terminate.into();
        }

        debug!(
            agent_id = %agent_id,
            status = ?status,
            running_jobs,
            "heartbeat processed"
        );

        Ok(Response::new(response))
    }

    async fn report_job_status(
        &self,
        request: Request<Streaming<JobStatusUpdate>>,
    ) -> Result<Response<JobStatusAck>, Status> {
        let mut stream = request.into_inner();
        let mut count = 0i64;
        let job_run_repo = JobRunRepo::new(&self.pool);

        while let Some(update) = stream.next().await {
            let update = update?;
            debug!(
                job_run_id = update.job_run_id,
                status = ?update.status,
                "job status update received"
            );

            // Parse job run ID
            let job_run_id: JobRunId = update
                .job_run_id
                .parse()
                .map_err(|_| Status::invalid_argument("invalid job_run_id"))?;

            // Convert proto status to model status
            let proto_status = ProtoRunStatus::try_from(update.status)
                .unwrap_or(ProtoRunStatus::Unspecified);
            let status = Self::convert_run_status(proto_status);

            // Update job_runs table based on status
            let error_msg = if update.error_message.is_empty() {
                None
            } else {
                Some(update.error_message.as_str())
            };

            let result = match status {
                JobStatus::Running => job_run_repo
                    .mark_running(job_run_id, AgentId::new()) // Agent ID should come from context
                    .await,
                JobStatus::Succeeded => job_run_repo
                    .mark_completed(
                        job_run_id,
                        true,
                        update.exit_code,
                        None,
                        None,
                    )
                    .await,
                JobStatus::Failed => job_run_repo
                    .mark_completed(
                        job_run_id,
                        false,
                        update.exit_code,
                        error_msg,
                        None,
                    )
                    .await,
                JobStatus::Cancelled => job_run_repo.mark_cancelled(job_run_id).await,
                JobStatus::TimedOut => job_run_repo.mark_timed_out(job_run_id).await,
                JobStatus::Skipped => job_run_repo
                    .mark_skipped(job_run_id, error_msg)
                    .await,
                _ => job_run_repo.get(job_run_id).await,
            };

            if let Err(e) = result {
                error!(error = %e, job_run_id = %job_run_id, "failed to update job_run status");
            } else if status.is_terminal() {
                // Trigger engine callbacks for terminal statuses via NATS event
                let job_completed_event = serde_json::json!({
                    "type": "job.completed",
                    "job_run_id": job_run_id.to_string(),
                    "success": status.is_success(),
                    "exit_code": update.exit_code,
                    "timestamp": Utc::now().to_rfc3339(),
                });

                if let Err(e) = self
                    .nats
                    .client()
                    .publish(
                        format!("met.engine.callbacks.{}", job_run_id.as_uuid()),
                        serde_json::to_vec(&job_completed_event)
                            .unwrap_or_default()
                            .into(),
                    )
                    .await
                {
                    warn!(error = %e, "failed to publish job completion callback");
                }

                let pool = Arc::clone(&self.pool);
                let store = self.object_store.clone();
                tokio::spawn(async move {
                    finalize_job_logs(pool.as_ref(), store, job_run_id).await;
                });
            }

            count += 1;
        }

        Ok(Response::new(JobStatusAck { received_count: count }))
    }

    async fn report_step_status(
        &self,
        request: Request<Streaming<StepStatusUpdate>>,
    ) -> Result<Response<StepStatusAck>, Status> {
        let mut stream = request.into_inner();
        let mut count = 0i64;
        let step_run_repo = StepRunRepo::new(&self.pool);

        while let Some(update) = stream.next().await {
            let update = update?;
            debug!(
                step_run_id = update.step_run_id,
                job_run_id = update.job_run_id,
                status = ?update.status,
                "step status update received"
            );

            // Parse step run ID
            let step_run_id: StepRunId = update
                .step_run_id
                .parse()
                .map_err(|_| Status::invalid_argument("invalid step_run_id"))?;

            // Convert proto status to model status
            let proto_status = ProtoRunStatus::try_from(update.status)
                .unwrap_or(ProtoRunStatus::Unspecified);
            let status = Self::convert_run_status(proto_status);

            // Update step_runs table based on status
            let error_msg = if update.error_message.is_empty() {
                None
            } else {
                Some(update.error_message.as_str())
            };

            let result = match status {
                JobStatus::Running => step_run_repo.mark_running(step_run_id).await,
                JobStatus::Succeeded | JobStatus::Failed => step_run_repo
                    .mark_completed(
                        step_run_id,
                        update.exit_code.unwrap_or(if status == JobStatus::Succeeded { 0 } else { 1 }),
                        error_msg,
                        None,
                        None,
                    )
                    .await,
                JobStatus::Skipped => step_run_repo
                    .mark_skipped(step_run_id, error_msg)
                    .await,
                _ => step_run_repo.get(step_run_id).await,
            };

            if let Err(e) = result {
                error!(error = %e, step_run_id = %step_run_id, "failed to update step_run status");
            }

            count += 1;
        }

        Ok(Response::new(StepStatusAck { received_count: count }))
    }

    async fn stream_logs(
        &self,
        request: Request<Streaming<LogChunk>>,
    ) -> Result<Response<LogAck>, Status> {
        let mut stream = request.into_inner();
        let mut last_sequence = 0i64;
        let log_repo = LogCacheRepo::new(&self.pool);
        let job_run_repo = JobRunRepo::new(&self.pool);
        let mut resolved_run_id: Option<RunId> = None;

        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            last_sequence = chunk.sequence;

            // Parse job_run_id
            let job_run_id: JobRunId = chunk
                .job_run_id
                .parse()
                .map_err(|_| Status::invalid_argument("invalid job_run_id"))?;

            let run_id = if let Some(r) = resolved_run_id {
                r
            } else {
                let jr = job_run_repo
                    .get(job_run_id)
                    .await
                    .map_err(|e| Status::internal(e.to_string()))?;
                resolved_run_id = Some(jr.run_id);
                jr.run_id
            };

            // Parse optional step_run_id
            let step_run_id: Option<StepRunId> = if chunk.step_run_id.is_empty() {
                None
            } else {
                Some(
                    chunk
                        .step_run_id
                        .parse()
                        .map_err(|_| Status::invalid_argument("invalid step_run_id"))?,
                )
            };

            // Determine stream type from proto enum
            let stream_type = match met_proto::agent::v1::LogStream::try_from(chunk.stream) {
                Ok(met_proto::agent::v1::LogStream::Stderr) => "stderr",
                _ => "stdout",
            };

            // Convert content bytes to string (logs are UTF-8 text)
            let content = String::from_utf8_lossy(&chunk.content);

            let line_ts = chunk
                .timestamp
                .as_ref()
                .and_then(|t| Utc.timestamp_opt(t.seconds, t.nanos as u32).single())
                .unwrap_or_else(Utc::now);

            if let Err(e) = log_repo
                .append_streaming(
                    job_run_id,
                    run_id,
                    step_run_id,
                    chunk.sequence,
                    stream_type,
                    &content,
                    line_ts,
                )
                .await
            {
                warn!(error = %e, job_run_id = %job_run_id, "failed to store log chunk");
            }

            // Publish to NATS for WebSocket streaming
            let log_event = serde_json::json!({
                "type": "log.chunk",
                "job_run_id": job_run_id.to_string(),
                "step_run_id": step_run_id.map(|s| s.to_string()),
                "sequence": chunk.sequence,
                "stream": stream_type,
                "content": content,
                "timestamp": Utc::now().to_rfc3339(),
            });

            let subject = format!("met.logs.{}", job_run_id.as_uuid());
            if let Err(e) = self
                .nats
                .client()
                .publish(
                    subject,
                    serde_json::to_vec(&log_event)
                        .unwrap_or_default()
                        .into(),
                )
                .await
            {
                warn!(error = %e, "failed to publish log chunk to NATS");
            }

            debug!(
                job_run_id = %job_run_id,
                sequence = chunk.sequence,
                stream = stream_type,
                "log chunk processed"
            );
        }

        Ok(Response::new(LogAck { last_sequence }))
    }

    #[instrument(skip(self, request))]
    async fn exchange_job_keys(
        &self,
        request: Request<JobKeyExchange>,
    ) -> Result<Response<JobSecretsPayload>, Status> {
        let req = request.into_inner();

        let agent_id: AgentId = req
            .agent_id
            .parse()
            .map_err(|_| Status::invalid_argument("invalid agent_id"))?;

        info!(
            agent_id = %agent_id,
            job_id = req.job_id,
            "job key exchange requested"
        );

        // Verify agent exists and is online
        let agent = self
            .registry
            .get(agent_id)
            .await
            .ok_or_else(|| Status::not_found("agent not found"))?;

        if agent.status == AgentStatus::Revoked || agent.status == AgentStatus::Dead {
            return Err(Status::permission_denied("agent is revoked or dead"));
        }

        // Parse the agent's one-time X25519 public key (32 bytes)
        let agent_public_key: [u8; 32] = req
            .one_time_x509_public_key
            .as_slice()
            .try_into()
            .map_err(|_| Status::invalid_argument("invalid public key length, expected 32 bytes"))?;

        // Look up secrets required for this job from job assignment or cache
        // For now, we'll encrypt a placeholder - in production this would
        // fetch from the secrets provider based on job configuration
        let job_secrets = self.fetch_job_secrets(&req.job_id).await;

        // Encrypt each secret with the agent's one-time public key
        let mut encrypted_secrets = Vec::new();
        for (name, value) in job_secrets {
            match HybridEncryption::encrypt(&agent_public_key, value.as_bytes(), &self.secrets_hmac_key) {
                Ok(envelope) => {
                    // Compute SHA-256 checksum of plaintext for verification
                    let mut hasher = Sha256::new();
                    hasher.update(value.as_bytes());
                    let checksum = hex::encode(hasher.finalize());

                    encrypted_secrets.push(EncryptedSecretValue {
                        key: name,
                        encrypted_value: envelope.to_bytes(),
                        sha256_checksum: checksum,
                    });
                }
                Err(e) => {
                    error!(error = %e, secret = %name, "failed to encrypt secret");
                    return Err(Status::internal(format!("failed to encrypt secret: {e}")));
                }
            }
        }

        info!(
            agent_id = %agent_id,
            job_id = %req.job_id,
            secrets_count = encrypted_secrets.len(),
            "secrets encrypted for job"
        );

        Ok(Response::new(JobSecretsPayload {
            job_id: req.job_id,
            secrets: encrypted_secrets,
        }))
    }

    #[instrument(skip(self, request))]
    async fn deregister(
        &self,
        request: Request<DeregisterRequest>,
    ) -> Result<Response<DeregisterResponse>, Status> {
        let req = request.into_inner();

        let agent_id: AgentId = req
            .agent_id
            .parse()
            .map_err(|_| Status::invalid_argument("invalid agent_id"))?;

        info!(
            agent_id = %agent_id,
            reason = req.reason,
            "agent deregistration requested"
        );

        // Remove from registry
        self.registry.remove(agent_id).await;

        // Update database
        let repo = AgentRepo::new(&self.pool);
        if let Err(e) = repo.update_status(agent_id, AgentStatus::Offline).await {
            error!(error = %e, "failed to update agent status");
        }

        Ok(Response::new(DeregisterResponse { success: true }))
    }
}
