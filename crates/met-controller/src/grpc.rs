//! gRPC service implementation for agent communication.

use std::sync::Arc;
use std::time::Instant;

use chrono::Utc;
use met_core::ids::{AgentId, JoinTokenId, OrganizationId};
use met_core::models::{Agent, AgentHeartbeat, AgentStatus, EnvironmentType, JoinToken};
use met_proto::agent::v1::{
    agent_service_server::AgentService, DeregisterRequest, DeregisterResponse,
    HeartbeatAction, HeartbeatRequest, HeartbeatResponse, JobKeyExchange,
    JobSecretsPayload, JobStatusAck, JobStatusUpdate, LogAck, LogChunk, RegisterRequest,
    RegisterResponse, StepStatusAck, StepStatusUpdate,
};
use met_store::repos::{AgentHeartbeatRepo, AgentRepo, JoinTokenRepo};
use met_store::PgPool;
use tokio_stream::StreamExt;
use tonic::{Request, Response, Status, Streaming};
use tracing::{debug, error, info, instrument, warn};

use crate::config::ControllerConfig;
use crate::error::ControllerError;
use crate::jwt::JwtManager;
use crate::nats::NatsDispatcher;
use crate::registry::{AgentRegistry, AgentState, ResourceSnapshot};

/// gRPC implementation of the AgentService.
pub struct AgentServiceImpl {
    config: ControllerConfig,
    pool: Arc<PgPool>,
    registry: AgentRegistry,
    jwt: JwtManager,
    nats: NatsDispatcher,
}

impl AgentServiceImpl {
    /// Create a new AgentService implementation.
    pub fn new(
        config: ControllerConfig,
        pool: Arc<PgPool>,
        registry: AgentRegistry,
        nats: NatsDispatcher,
    ) -> Self {
        let jwt = JwtManager::new(
            &config.jwt_secret,
            config.jwt_validity,
            config.jwt_renewable,
        );

        Self {
            config,
            pool,
            registry,
            jwt,
            nats,
        }
    }

    /// Validate and parse a join token.
    async fn validate_join_token(&self, token: &str) -> Result<JoinToken, ControllerError> {
        // Hash the token for lookup
        let token_hash =
            bcrypt::hash(token, bcrypt::DEFAULT_COST).map_err(ControllerError::Bcrypt)?;

        let repo = JoinTokenRepo::new(&self.pool);

        // Find the token - since we can't reverse the bcrypt hash, we need to
        // check against known tokens. In production, use a different approach
        // like storing a searchable hash prefix.
        let token = repo
            .validate_token(&token_hash)
            .await?
            .ok_or(ControllerError::InvalidJoinToken)?;

        if token.revoked {
            return Err(ControllerError::JoinTokenRevoked);
        }

        if !token.is_valid() {
            if token.expires_at.is_some_and(|e| Utc::now() >= e) {
                return Err(ControllerError::JoinTokenExpired);
            }
            if token.max_uses.is_some_and(|m| token.current_uses >= m) {
                return Err(ControllerError::JoinTokenExhausted);
            }
        }

        Ok(token)
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

        // Validate join token
        // Note: In production, implement proper token validation
        // For now, we'll create a simplified flow
        let join_token_id = JoinTokenId::new(); // Placeholder
        let org_id = OrganizationId::new(); // Would come from token validation
        let pool_tags: Vec<String> = req
            .capabilities
            .as_ref()
            .map(|c| c.pool_tags.clone())
            .unwrap_or_default();
        let labels: Vec<String> = req
            .capabilities
            .as_ref()
            .map(|c| c.labels.clone())
            .unwrap_or_default();

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
        agent.tags = pool_tags.clone();
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
        agent.join_token_id = Some(join_token_id);
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

        // Update registry
        let updated = self
            .registry
            .heartbeat(agent_id, status, running_jobs, current_job, resources.clone())
            .await
            .ok_or_else(|| Status::not_found("agent not found"))?;

        // Update database heartbeat
        let repo = AgentRepo::new(&self.pool);
        repo.heartbeat(agent_id, running_jobs)
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

        while let Some(update) = stream.next().await {
            let update = update?;
            debug!(
                job_run_id = update.job_run_id,
                status = ?update.status,
                "job status update received"
            );

            // TODO: Update job_runs table with status
            // TODO: Trigger pipeline engine callbacks

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

        while let Some(update) = stream.next().await {
            let update = update?;
            debug!(
                step_run_id = update.step_run_id,
                job_run_id = update.job_run_id,
                status = ?update.status,
                "step status update received"
            );

            // TODO: Update step_runs table with status

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

        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            // TODO: Forward logs to log storage / WebSocket fanout
            last_sequence = chunk.sequence;
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

        // TODO: Implement actual secret encryption
        // 1. Look up secrets required for the job
        // 2. Encrypt each secret with the agent's one-time public key
        // 3. Return the encrypted secrets

        Ok(Response::new(JobSecretsPayload {
            job_id: req.job_id,
            secrets: vec![], // Would contain encrypted secrets
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
