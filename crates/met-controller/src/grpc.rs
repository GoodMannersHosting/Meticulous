//! gRPC service implementation for agent communication.

use std::sync::{Arc, OnceLock};
use std::time::Instant;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::{TimeZone, Utc};
use met_core::hash_join_token;
use met_core::ids::{AgentId, JobRunId, OrganizationId, ProjectId, RunId, StepId, StepRunId};
use met_core::models::{
    Agent, AgentHeartbeat, AgentStatus, EnvironmentType, JobStatus, JoinTokenScope,
};
use met_logging::{Redactor, RedactorConfig};
use met_objstore::ObjectStore;
use met_proto::agent::v1::{
    DeregisterRequest, DeregisterResponse, EncryptedSecretValue, HeartbeatAction, HeartbeatRequest,
    HeartbeatResponse, JobExecutionMetadata as ProtoJobExecMeta, JobKeyExchange, JobSecretsPayload,
    JobStatusAck, JobStatusUpdate, LogAck, LogChunk, LogStream, RegisterRequest, RegisterResponse,
    SecretMaterialKind, SecurityBundle, StepStatusAck, StepStatusUpdate,
    agent_service_server::AgentService,
};
use met_proto::common::v1::RunStatus as ProtoRunStatus;
use met_proto::common::v1::Timestamp as ProtoTimestamp;
use met_proto::controller::v1::JobCompletion;
use met_secrets::BuiltinStoredCrypto;
use met_secrets::pki::encryption::HybridEncryption;
use met_store::PgPool;
use met_store::StoreError;
use met_store::repos::{
    AgentHeartbeatRepo, AgentRepo, JobRunRepo, JoinTokenRepo, LogCacheRepo,
    PipelineRunWorkflowOutputsRepo, ProjectRepo, StepRunRepo,
    reenroll_agent_with_exhausted_join_token, register_agent_with_join_token,
};
use sha2::{Digest, Sha256};
use tokio_stream::StreamExt;
use tonic::{Request, Response, Status, Streaming};
use tracing::{debug, error, info, instrument, warn};

use crate::config::ControllerConfig;
use crate::error::ControllerError;
use crate::jwt::JwtManager;
use crate::log_archive::finalize_job_logs;
use crate::nats::NatsDispatcher;
use crate::nats_jwt::issue_agent_nats_credentials;
use crate::registry::{AgentRegistry, AgentState, ResourceSnapshot, agent_state_from_db_row};

/// After this many heartbeats without the agent reporting `draining` while drain is requested, delete its NATS consumer.
const DRAIN_FORCE_EJECT_MISSED_HEARTBEATS: i32 = 3;

const MAX_SECURITY_MACHINE_ID_LEN: usize = 256;
const MAX_SECURITY_EGRESS_IP_LEN: usize = 45;
const MAX_SECURITY_LOGICAL_CPUS: u32 = 65_536;
const MAX_SECURITY_MEMORY_BYTES: u64 = 1 << 50;
const MAX_K8S_METADATA_FIELD_LEN: usize = 512;

fn security_bundle_to_json(bundle: &SecurityBundle) -> serde_json::Value {
    serde_json::json!({
        "hostname": bundle.hostname,
        "os": bundle.os,
        "arch": bundle.arch,
        "kernel_version": bundle.kernel_version,
        "public_ips": bundle.public_ips,
        "private_ips": bundle.private_ips,
        "ntp_synchronized": bundle.ntp_synchronized,
        "container_runtime": bundle.container_runtime,
        "container_runtime_version": bundle.container_runtime_version,
        "environment_type": bundle.environment_type,
        "agent_x509_public_key_hex": hex::encode(&bundle.agent_x509_public_key),
        "machine_id": bundle.machine_id,
        "logical_cpus": bundle.logical_cpus,
        "memory_total_bytes": bundle.memory_total_bytes,
        "egress_public_ip": bundle.egress_public_ip,
        "kubernetes_pod_uid": bundle.kubernetes_pod_uid,
        "kubernetes_namespace": bundle.kubernetes_namespace,
        "kubernetes_node_name": bundle.kubernetes_node_name,
    })
}

/// gRPC implementation of the AgentService.
pub struct AgentServiceImpl {
    config: ControllerConfig,
    pool: Arc<PgPool>,
    registry: AgentRegistry,
    jwt: JwtManager,
    nats: NatsDispatcher,
    /// Optional object store for log archival (SeaweedFS / S3).
    object_store: Option<Arc<dyn ObjectStore + Send + Sync>>,
    /// Decrypts `builtin_secrets` ciphertext for job key exchange.
    stored_secret_crypto: Option<Arc<BuiltinStoredCrypto>>,
}

impl AgentServiceImpl {
    /// Create a new AgentService implementation.
    pub fn new(
        config: ControllerConfig,
        pool: Arc<PgPool>,
        registry: AgentRegistry,
        nats: NatsDispatcher,
        object_store: Option<Arc<dyn ObjectStore + Send + Sync>>,
        stored_secret_crypto: Option<Arc<BuiltinStoredCrypto>>,
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
            object_store,
            stored_secret_crypto,
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

    fn job_status_to_proto(status: JobStatus) -> ProtoRunStatus {
        match status {
            JobStatus::Pending => ProtoRunStatus::Pending,
            JobStatus::Queued => ProtoRunStatus::Queued,
            JobStatus::Running => ProtoRunStatus::Running,
            JobStatus::Succeeded => ProtoRunStatus::Succeeded,
            JobStatus::Failed => ProtoRunStatus::Failed,
            JobStatus::Cancelled => ProtoRunStatus::Cancelled,
            JobStatus::TimedOut => ProtoRunStatus::TimedOut,
            JobStatus::Skipped => ProtoRunStatus::Skipped,
        }
    }

    /// Validate the security bundle from the agent.
    fn validate_security_bundle(&self, bundle: &SecurityBundle) -> Result<(), ControllerError> {
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

        if bundle.machine_id.len() > MAX_SECURITY_MACHINE_ID_LEN {
            return Err(ControllerError::ValidationFailed(format!(
                "machine_id exceeds max length {}",
                MAX_SECURITY_MACHINE_ID_LEN
            )));
        }
        if bundle.egress_public_ip.len() > MAX_SECURITY_EGRESS_IP_LEN {
            return Err(ControllerError::ValidationFailed(
                "egress_public_ip too long".to_string(),
            ));
        }
        if bundle.logical_cpus > MAX_SECURITY_LOGICAL_CPUS {
            return Err(ControllerError::ValidationFailed(
                "logical_cpus out of range".to_string(),
            ));
        }
        if bundle.memory_total_bytes > MAX_SECURITY_MEMORY_BYTES {
            return Err(ControllerError::ValidationFailed(
                "memory_total_bytes out of range".to_string(),
            ));
        }
        for (label, s) in [
            ("kubernetes_pod_uid", bundle.kubernetes_pod_uid.as_str()),
            ("kubernetes_namespace", bundle.kubernetes_namespace.as_str()),
            ("kubernetes_node_name", bundle.kubernetes_node_name.as_str()),
        ] {
            if s.len() > MAX_K8S_METADATA_FIELD_LEN {
                return Err(ControllerError::ValidationFailed(format!(
                    "{label} exceeds max length {MAX_K8S_METADATA_FIELD_LEN}"
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

    /// Resolve plaintext secrets for hybrid encryption to the agent.
    async fn fetch_job_secrets(
        &self,
        job_id: &str,
        org_id: &str,
        project_id: &str,
        pipeline_id: &str,
        hints_json: &str,
    ) -> Result<Vec<(String, String, i32)>, ControllerError> {
        let Some(crypto) = self.stored_secret_crypto.as_ref() else {
            return Ok(Vec::new());
        };
        met_secret_resolve::resolve_job_secrets_for_exchange(
            &self.pool,
            crypto.as_ref(),
            job_id,
            org_id,
            project_id,
            pipeline_id,
            hints_json,
        )
        .await
        .map_err(|e| ControllerError::Internal(e.to_string()))
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
        let token_row = join_repo
            .get_by_token_hash(&token_hash)
            .await
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::unauthenticated("invalid or unknown join token"))?;

        if token_row.revoked {
            return Err(Status::unauthenticated(
                "join token is expired, revoked, or has reached max uses",
            ));
        }
        if let Some(exp) = token_row.expires_at
            && exp < Utc::now()
        {
            return Err(Status::unauthenticated(
                "join token is expired, revoked, or has reached max uses",
            ));
        }

        let allow_fresh_registration = token_row.current_uses < token_row.max_uses;
        let reenroll_with_exhausted_token = !allow_fresh_registration
            && token_row.max_uses > 0
            && token_row.consumed_by_agent_id.is_some();

        if !allow_fresh_registration && !reenroll_with_exhausted_token {
            return Err(Status::unauthenticated(
                "join token is expired, revoked, or has reached max uses",
            ));
        }

        let join_record = token_row;

        let org_id = match join_record.scope {
            JoinTokenScope::Tenant => {
                let Some(scope_uuid) = join_record.scope_id else {
                    return Err(Status::failed_precondition(
                        "tenant join token is missing organization scope",
                    ));
                };
                OrganizationId::from_uuid(scope_uuid)
            }
            JoinTokenScope::Project => {
                let Some(proj_uuid) = join_record.scope_id else {
                    return Err(Status::failed_precondition(
                        "project join token is missing project scope",
                    ));
                };
                let project = ProjectRepo::new(&self.pool)
                    .get(ProjectId::from_uuid(proj_uuid))
                    .await
                    .map_err(|e| match e {
                        StoreError::NotFound { .. } => Status::failed_precondition(
                            "join token references a project that does not exist",
                        ),
                        _ => Status::internal(e.to_string()),
                    })?;
                project.org_id
            }
            JoinTokenScope::Platform | JoinTokenScope::Pipeline => {
                return Err(Status::invalid_argument(
                    "only tenant-scoped or project-scoped join tokens are supported for agent registration",
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

        if reenroll_with_exhausted_token && bundle.machine_id.trim().is_empty() {
            return Err(Status::invalid_argument(
                "machine_id is required to re-register with an exhausted join token",
            ));
        }

        let caps = req.capabilities.as_ref();

        let mut agent = if reenroll_with_exhausted_token {
            let consumed_id = join_record
                .consumed_by_agent_id
                .ok_or_else(|| Status::failed_precondition("join token missing consuming agent"))?;
            AgentRepo::new(&self.pool)
                .get(consumed_id)
                .await
                .map_err(|_| Status::unauthenticated("invalid or unknown join token"))?
        } else {
            let agent_id = AgentId::new();
            let mut a = Agent::new(
                org_id,
                &bundle.hostname,
                caps.map(|c| c.os.as_str()).unwrap_or(&bundle.os),
                caps.map(|c| c.arch.as_str()).unwrap_or(&bundle.arch),
                env!("CARGO_PKG_VERSION"),
            );
            a.id = agent_id;
            a
        };

        let agent_id = agent.id;
        agent.org_id = org_id;
        agent.name = bundle.hostname.clone();
        agent.os = caps
            .map(|c| c.os.clone())
            .unwrap_or_else(|| bundle.os.clone());
        agent.arch = caps
            .map(|c| c.arch.clone())
            .unwrap_or_else(|| bundle.arch.clone());
        agent.version = env!("CARGO_PKG_VERSION").to_string();
        agent.status = AgentStatus::Online;
        agent.tags = labels.clone();
        // Match `runs-on.tags` in pipelines: scheduler requires `key=value` strings on this column
        // (see met-engine scheduler). Mirror reported OS/arch from capabilities or the security bundle.
        let os_for_tags = caps.map(|c| c.os.as_str()).unwrap_or(bundle.os.as_str());
        let arch_for_tags = caps
            .map(|c| c.arch.as_str())
            .unwrap_or(bundle.arch.as_str());
        for tag in [format!("os={os_for_tags}"), format!("arch={arch_for_tags}")] {
            if !agent.tags.contains(&tag) {
                agent.tags.push(tag);
            }
        }
        agent.environment_type = self.convert_environment_type(
            met_proto::agent::v1::EnvironmentType::try_from(bundle.environment_type)
                .unwrap_or(met_proto::agent::v1::EnvironmentType::Virtual),
        );
        agent.kernel_version = Some(bundle.kernel_version.clone()).filter(|s| !s.is_empty());
        agent.public_ips = bundle.public_ips.clone();
        agent.private_ips = bundle.private_ips.clone();
        agent.ntp_synchronized = bundle.ntp_synchronized;
        agent.container_runtime = Some(bundle.container_runtime.clone()).filter(|s| !s.is_empty());
        agent.container_runtime_version =
            Some(bundle.container_runtime_version.clone()).filter(|s| !s.is_empty());
        agent.x509_public_key =
            Some(bundle.agent_x509_public_key.clone()).filter(|b| !b.is_empty());
        agent.last_security_bundle =
            met_core::models::pack_last_security_bundle(security_bundle_to_json(bundle));
        agent.join_token_id = Some(join_record.id);
        agent.last_heartbeat_at = Some(Utc::now());
        agent.pool_tags = pool_tags.clone();
        agent.pool = pool_tags.first().cloned();

        // Issue JWT
        let (jwt_token, jwt_expires_at) = self
            .jwt
            .issue(agent_id, org_id, pool_tags.clone())
            .map_err(|e| Status::internal(e.to_string()))?;

        agent.jwt_expires_at = Some(jwt_expires_at);

        let (nats_jwt, nats_seed) = if let Some(ref seed) = self.config.nats_account_signing_seed {
            issue_agent_nats_credentials(
                org_id,
                &pool_tags,
                agent_id,
                seed,
                self.config
                    .nats_account_issuer_pubkey
                    .as_deref()
                    .filter(|s| !s.is_empty()),
                self.config.nats_agent_jwt_ttl,
            )
            .map_err(|e| Status::internal(e.to_string()))?
        } else {
            (String::new(), String::new())
        };

        let agent = if reenroll_with_exhausted_token {
            let (updated, _) = reenroll_agent_with_exhausted_join_token(
                &self.pool,
                &token_hash,
                &bundle.machine_id,
                &agent,
            )
            .await
            .map_err(|e| match e {
                StoreError::NotFound { .. } => {
                    Status::unauthenticated("invalid or unknown join token")
                }
                StoreError::Constraint(_) => {
                    Status::unauthenticated("invalid or unknown join token")
                }
                _ => Status::internal(e.to_string()),
            })?;
            updated
        } else {
            let (registered, _) = register_agent_with_join_token(&self.pool, &token_hash, &agent)
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
            registered
        };

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
            running_jobs: agent.running_jobs,
            current_job: None,
            jwt_expires_at,
            resources: None,
        };
        self.registry.register(state).await;

        if let Err(e) = self
            .nats
            .reconcile_jobs_consumers_for_agent(org_id, &agent_id.to_string(), &pool_tags)
            .await
        {
            warn!(
                error = %e,
                agent_id = %agent_id,
                "NATS JOBS consumer reconcile after register failed (non-fatal)"
            );
        }

        // JetStream pull: one non-overlapping inbox per agent (pool is `*` in the subject).
        let job_inbox = crate::nats::subjects::job_inbox_filter(org_id, &agent_id.to_string());
        let nats_subjects: Vec<String> = std::iter::once(job_inbox)
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
                jwt: nats_jwt,
                nkey_seed: nats_seed,
            }),
            heartbeat_interval_secs: self.config.heartbeat_interval.as_secs() as i32,
            organization_id: org_id.as_uuid().to_string(),
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
            .map(
                |s| match met_proto::common::v1::AgentStatus::try_from(s.status) {
                    Ok(met_proto::common::v1::AgentStatus::Online) => AgentStatus::Online,
                    Ok(met_proto::common::v1::AgentStatus::Busy) => AgentStatus::Busy,
                    Ok(met_proto::common::v1::AgentStatus::Draining) => AgentStatus::Draining,
                    Ok(met_proto::common::v1::AgentStatus::Offline) => AgentStatus::Offline,
                    _ => AgentStatus::Online,
                },
            )
            .unwrap_or(AgentStatus::Online);

        let running_jobs = req.status.as_ref().map(|s| s.running_jobs).unwrap_or(0);

        let current_job = req.current_job_id.and_then(|id| id.parse().ok());

        let resources = req.resources.map(|r| ResourceSnapshot {
            cpu_percent: r.cpu_percent,
            memory_percent: r.memory_percent,
            disk_percent: r.disk_percent,
        });

        // Rehydrate in-memory registry after controller restart (before DB merge).
        if self.registry.get(agent_id).await.is_none() {
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
                }
                Err(_) => return Err(Status::not_found("agent not found")),
            }
        }

        let repo = AgentRepo::new(&self.pool);
        let mut db_row = repo
            .heartbeat_from_controller(agent_id, status, running_jobs)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let mut force_ejected = false;
        if db_row.drain_missed_heartbeats >= DRAIN_FORCE_EJECT_MISSED_HEARTBEATS {
            force_ejected = true;
            if let Err(e) = self
                .nats
                .delete_agent_pull_consumer(&agent_id.to_string())
                .await
            {
                warn!(
                    error = %e,
                    agent_id = %agent_id,
                    "failed to delete agent NATS consumer after drain was not acknowledged"
                );
            }
            repo.update_status(agent_id, AgentStatus::Offline)
                .await
                .map_err(|e| Status::internal(e.to_string()))?;
            db_row = repo
                .get(agent_id)
                .await
                .map_err(|e| Status::internal(e.to_string()))?;
        }

        self.registry
            .heartbeat(
                agent_id,
                db_row.status,
                db_row.running_jobs,
                current_job,
                resources.clone(),
            )
            .await
            .ok_or_else(|| Status::not_found("agent not found"))?;

        // Record heartbeat for diagnostics
        let heartbeat_record = AgentHeartbeat::new(agent_id, db_row.status);
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

        if let Err(e) = self
            .nats
            .reconcile_jobs_consumers_for_agent(
                db_row.org_id,
                &agent_id.to_string(),
                &db_row.pool_tags,
            )
            .await
        {
            warn!(
                error = %e,
                agent_id = %agent_id,
                "NATS JOBS consumer reconcile on heartbeat failed (non-fatal)"
            );
        }

        // Check for JWT renewal
        // We'd need to validate the JWT from request metadata in production
        let mut response = HeartbeatResponse {
            action: HeartbeatAction::Continue.into(),
            config_patch: None,
            new_jwt_token: None,
            new_jwt_expires_at: None,
        };

        if force_ejected {
            response.action = HeartbeatAction::Terminate.into();
        } else if db_row.status == AgentStatus::Revoked {
            response.action = HeartbeatAction::Terminate.into();
        } else if db_row.status == AgentStatus::Draining && status != AgentStatus::Draining {
            // API requested drain; agent has not yet reported draining — tell it to stop accepting work.
            response.action = HeartbeatAction::Drain.into();
        } else if matches!(db_row.status, AgentStatus::Online | AgentStatus::Busy)
            && status == AgentStatus::Draining
        {
            // API resumed (or never was draining) but agent still reports draining — clear local drain.
            response.action = HeartbeatAction::Resume.into();
        }

        debug!(
            agent_id = %agent_id,
            reported = ?status,
            db_status = ?db_row.status,
            drain_missed = db_row.drain_missed_heartbeats,
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
            let proto_status =
                ProtoRunStatus::try_from(update.status).unwrap_or(ProtoRunStatus::Unspecified);
            let status = Self::convert_run_status(proto_status);

            // Update job_runs table based on status
            let error_msg = if update.error_message.is_empty() {
                None
            } else {
                Some(update.error_message.as_str())
            };

            let result = match status {
                JobStatus::Queued => job_run_repo.mark_queued(job_run_id).await,
                JobStatus::Running => {
                    let Some(raw) = update.agent_id.as_deref().filter(|s| !s.is_empty()) else {
                        return Err(Status::invalid_argument(
                            "agent_id is required when reporting job running status",
                        ));
                    };
                    let aid: AgentId = raw
                        .parse()
                        .map_err(|_| Status::invalid_argument("invalid agent_id"))?;
                    let agent_snapshot = match AgentRepo::new(self.pool.as_ref())
                        .get_for_audit_snapshot(aid)
                        .await
                    {
                        Ok(agent) => Some(agent.job_audit_snapshot_json()),
                        Err(e) => {
                            warn!(
                                job_run_id = %job_run_id,
                                agent_id = %aid,
                                error = %e,
                                "could not load agent row for job-run audit snapshot; storing running state without snapshot"
                            );
                            None
                        }
                    };
                    job_run_repo
                        .mark_running(job_run_id, aid, agent_snapshot)
                        .await
                }
                JobStatus::Succeeded => {
                    job_run_repo
                        .mark_completed(job_run_id, true, update.exit_code, None, None)
                        .await
                }
                JobStatus::Failed => {
                    job_run_repo
                        .mark_completed(job_run_id, false, update.exit_code, error_msg, None)
                        .await
                }
                JobStatus::Cancelled => {
                    job_run_repo
                        .mark_cancelled(job_run_id, error_msg.or(Some("Cancelled")))
                        .await
                }
                JobStatus::TimedOut => job_run_repo.mark_timed_out(job_run_id).await,
                JobStatus::Skipped => job_run_repo.mark_skipped(job_run_id, error_msg).await,
                _ => job_run_repo.get(job_run_id).await,
            };

            match result {
                Ok(ref job_row) if status.is_terminal() => {
                    if let Some(ref meta) = update.execution_metadata {
                        let aid = job_row
                            .agent_id
                            .or_else(|| update.agent_id.as_deref().and_then(|s| s.parse().ok()));
                        if let Err(e) = persist_run_binaries_for_job(
                            self.pool.as_ref(),
                            job_row.run_id,
                            job_run_id,
                            aid,
                            meta,
                        )
                        .await
                        {
                            warn!(
                                error = %e,
                                job_run_id = %job_run_id,
                                "failed to persist executed binaries from job metadata"
                            );
                        }
                    }

                    if let Some(raw) = update
                        .sbom_cyclonedx_json
                        .as_deref()
                        .filter(|s| !s.is_empty())
                    {
                        if let Err(e) = persist_job_sbom_cyclonedx(
                            self.pool.as_ref(),
                            job_row.run_id,
                            job_run_id,
                            raw,
                        )
                        .await
                        {
                            warn!(
                                error = %e,
                                job_run_id = %job_run_id,
                                "failed to persist CycloneDX SBOM from job status"
                            );
                        }
                    }

                    if let Some(ref wo) = update.workflow_invocation_outputs {
                        if !wo.workflow_invocation_id.is_empty() {
                            let mut public_map = serde_json::Map::new();
                            for (k, v) in &wo.public {
                                public_map.insert(k.clone(), serde_json::Value::String(v.clone()));
                            }
                            let mut secret_map = serde_json::Map::new();
                            for s in &wo.secrets {
                                let mut packed = Vec::new();
                                packed.extend_from_slice(&s.ephemeral_x25519_public);
                                packed.extend_from_slice(&s.nonce);
                                packed.extend_from_slice(&s.ciphertext);
                                secret_map.insert(
                                    s.name.clone(),
                                    serde_json::Value::String(STANDARD.encode(&packed)),
                                );
                            }
                            if let Err(e) = PipelineRunWorkflowOutputsRepo::new(self.pool.as_ref())
                                .upsert_merge(
                                    job_row.run_id.as_uuid(),
                                    &wo.workflow_invocation_id,
                                    job_row.id.as_uuid(),
                                    serde_json::Value::Object(public_map),
                                    serde_json::Value::Object(secret_map),
                                )
                                .await
                            {
                                warn!(
                                    error = %e,
                                    job_run_id = %job_run_id,
                                    "failed to persist workflow invocation outputs"
                                );
                            }
                        }
                    }

                    let workflow_completion_proto = update
                        .workflow_invocation_outputs
                        .as_ref()
                        .map(|w| vec![w.clone()])
                        .unwrap_or_default();

                    match job_run_repo.get_pipeline_context(job_run_id).await {
                        Ok(Some(ctx)) => {
                            let org_id = OrganizationId::from_uuid(ctx.org_id);
                            let duration_ms = match (job_row.started_at, job_row.finished_at) {
                                (Some(s), Some(f)) => (f - s).num_milliseconds().max(0) as i64,
                                _ => 0,
                            };
                            let ts = Utc::now();
                            let completion = JobCompletion {
                                job_run_id: job_row.id.to_string(),
                                run_id: job_row.run_id.to_string(),
                                agent_id: job_row
                                    .agent_id
                                    .map(|a| a.to_string())
                                    .unwrap_or_default(),
                                status: Self::job_status_to_proto(job_row.status) as i32,
                                exit_code: job_row.exit_code,
                                error_message: job_row.error_message.clone().unwrap_or_default(),
                                duration_ms,
                                timestamp: Some(ProtoTimestamp {
                                    seconds: ts.timestamp(),
                                    nanos: ts.timestamp_subsec_nanos() as i32,
                                }),
                                workflow_outputs: workflow_completion_proto,
                                ..Default::default()
                            };
                            if let Err(e) = self
                                .nats
                                .publish_job_completion_proto(org_id, &completion)
                                .await
                            {
                                warn!(
                                    error = %e,
                                    job_run_id = %job_run_id,
                                    "failed to publish JobCompletion to JetStream"
                                );
                            }
                        }
                        Ok(None) => {
                            warn!(
                                job_run_id = %job_run_id,
                                "skipping JobCompletion publish: no pipeline context for job run"
                            );
                        }
                        Err(e) => {
                            warn!(
                                error = %e,
                                job_run_id = %job_run_id,
                                "get_pipeline_context failed for JobCompletion"
                            );
                        }
                    }

                    let pool = Arc::clone(&self.pool);
                    let store = self.object_store.clone();
                    tokio::spawn(async move {
                        finalize_job_logs(pool.as_ref(), store, job_run_id).await;
                    });
                }
                Err(ref e) => {
                    error!(error = %e, job_run_id = %job_run_id, "failed to update job_run status");
                }
                Ok(_) => {}
            }

            count += 1;
        }

        Ok(Response::new(JobStatusAck {
            received_count: count,
        }))
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
            let proto_status =
                ProtoRunStatus::try_from(update.status).unwrap_or(ProtoRunStatus::Unspecified);
            let status = Self::convert_run_status(proto_status);

            // Update step_runs table based on status
            let error_msg = if update.error_message.is_empty() {
                None
            } else {
                Some(update.error_message.as_str())
            };

            let result = match status {
                JobStatus::Running => step_run_repo.mark_running(step_run_id).await,
                JobStatus::Succeeded | JobStatus::Failed => {
                    step_run_repo
                        .mark_completed(
                            step_run_id,
                            update
                                .exit_code
                                .unwrap_or(if status == JobStatus::Succeeded { 0 } else { 1 }),
                            error_msg,
                            None,
                            None,
                        )
                        .await
                }
                JobStatus::Skipped => step_run_repo.mark_skipped(step_run_id, error_msg).await,
                JobStatus::Cancelled => {
                    step_run_repo
                        .mark_cancelled(step_run_id, error_msg.or(Some("Cancelled")))
                        .await
                }
                _ => step_run_repo.get(step_run_id).await,
            };

            if let Err(e) = result {
                error!(error = %e, step_run_id = %step_run_id, "failed to update step_run status");
            }

            count += 1;
        }

        Ok(Response::new(StepStatusAck {
            received_count: count,
        }))
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

            let log_stream = LogStream::try_from(chunk.stream).unwrap_or(LogStream::Unspecified);

            // Convert content bytes to string (telemetry JSON or UTF-8 log text)
            let content_raw = String::from_utf8_lossy(&chunk.content);

            let line_ts = chunk
                .timestamp
                .as_ref()
                .and_then(|t| Utc.timestamp_opt(t.seconds, t.nanos as u32).single())
                .unwrap_or_else(Utc::now);

            match log_stream {
                LogStream::ExecBinary
                | LogStream::Syscall
                | LogStream::RuntimeScript
                | LogStream::NetworkFlow => {
                    if let Err(e) = ingest_telemetry_log_chunk(
                        self.pool.as_ref(),
                        run_id,
                        job_run_id,
                        step_run_id,
                        log_stream,
                        content_raw.as_ref(),
                    )
                    .await
                    {
                        warn!(error = %e, job_run_id = %job_run_id, "telemetry chunk ingest failed");
                    }
                    let evt_type = match log_stream {
                        LogStream::ExecBinary => "exec.binary",
                        LogStream::Syscall => "exec.syscall",
                        LogStream::RuntimeScript => "exec.runtime_script",
                        LogStream::NetworkFlow => "net.flow",
                        _ => "exec.chunk",
                    };
                    let payload_parse =
                        serde_json::from_str::<serde_json::Value>(content_raw.as_ref()).ok();
                    let log_event = serde_json::json!({
                        "type": evt_type,
                        "job_run_id": job_run_id.to_string(),
                        "step_run_id": step_run_id.map(|s| s.to_string()),
                        "sequence": chunk.sequence,
                        "payload": payload_parse,
                        "timestamp": Utc::now().to_rfc3339(),
                    });
                    let subject = format!("met.logs.{}", job_run_id.as_uuid());
                    if let Err(e) = self
                        .nats
                        .client()
                        .publish(
                            subject,
                            serde_json::to_vec(&log_event).unwrap_or_default().into(),
                        )
                        .await
                    {
                        warn!(error = %e, "failed to publish telemetry chunk to NATS");
                    }
                }
                _ => {
                    let stream_type = if log_stream == LogStream::Stderr {
                        "stderr"
                    } else {
                        "stdout"
                    };
                    let content = telemetry_log_redactor().redact(content_raw.as_ref());

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
                            serde_json::to_vec(&log_event).unwrap_or_default().into(),
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
            }
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
            .map_err(|_| {
                Status::invalid_argument("invalid public key length, expected 32 bytes")
            })?;

        let job_secrets = self
            .fetch_job_secrets(
                &req.job_id,
                &req.org_id,
                &req.project_id,
                &req.pipeline_id,
                &req.secret_resolution_hints_json,
            )
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        // Encrypt each secret with the agent's one-time public key
        let mut encrypted_secrets = Vec::new();
        for (name, value, material) in job_secrets {
            let material_kind = match material {
                2 => SecretMaterialKind::WorkspaceFilePath as i32,
                1 => SecretMaterialKind::EnvInline as i32,
                _ => SecretMaterialKind::Unspecified as i32,
            };
            match HybridEncryption::encrypt(&agent_public_key, value.as_bytes()) {
                Ok(envelope) => {
                    // Compute SHA-256 checksum of plaintext for verification
                    let mut hasher = Sha256::new();
                    hasher.update(value.as_bytes());
                    let checksum = hex::encode(hasher.finalize());

                    encrypted_secrets.push(EncryptedSecretValue {
                        key: name,
                        encrypted_value: envelope.to_bytes(),
                        sha256_checksum: checksum,
                        material_kind,
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

fn telemetry_log_redactor() -> &'static Redactor {
    static R: OnceLock<Redactor> = OnceLock::new();
    R.get_or_init(|| Redactor::new(RedactorConfig::default()))
}

/// ADR-006-style path redaction before persistence and fanout (home directory prefix).
fn redact_exec_path_for_storage(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("/home/") {
        if let Some(idx) = rest.find('/') {
            return format!("/home/<redacted>{}", &rest[idx..]);
        }
        return "/home/<redacted>".to_string();
    }
    path.to_string()
}

/// Inline CycloneDX cap on the job status path (aligned with the agent read cap).
const MAX_JOB_STATUS_SBOM_BYTES: usize = 6 * 1024 * 1024;

async fn persist_job_sbom_cyclonedx(
    pool: &PgPool,
    run_id: RunId,
    job_run_id: JobRunId,
    raw: &str,
) -> sqlx::Result<()> {
    if raw.len() > MAX_JOB_STATUS_SBOM_BYTES {
        warn!(
            len = raw.len(),
            max = MAX_JOB_STATUS_SBOM_BYTES,
            "SBOM JSON on job status exceeds cap; not persisting"
        );
        return Ok(());
    }
    let doc: serde_json::Value = match serde_json::from_str::<serde_json::Value>(raw) {
        Ok(v) if v.is_object() => v,
        _ => {
            warn!("SBOM payload on job status is not a JSON object; skipping");
            return Ok(());
        }
    };
    let sha256 = hex::encode(Sha256::digest(raw.as_bytes()));
    let size_bytes = raw.len() as i64;
    let metadata = serde_json::json!({ "sbom_json": doc });
    let id = uuid::Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO artifacts (id, run_id, job_run_id, name, content_type, size_bytes, storage_path, sha256, metadata)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        "#,
    )
    .bind(id)
    .bind(run_id.as_uuid())
    .bind(job_run_id.as_uuid())
    .bind("sbom.cdx.json")
    .bind("application/vnd.cyclonedx+json")
    .bind(size_bytes)
    .bind("agent-job-status-inline")
    .bind(&sha256)
    .bind(metadata)
    .execute(pool)
    .await?;
    Ok(())
}

/// Prefer explicit `step_run_ids` from the agent; otherwise correlate IR `step_ids` with `step_runs`.
async fn resolve_step_run_uuid_for_binary_row(
    pool: &PgPool,
    job_run_id: JobRunId,
    step_run_ids: &[String],
    step_ids: &[String],
) -> sqlx::Result<Option<uuid::Uuid>> {
    use std::str::FromStr as _;

    if let Some(id) = step_run_ids
        .iter()
        .find_map(|s| StepRunId::from_str(s).ok())
    {
        return Ok(Some(id.as_uuid()));
    }

    for sid in step_ids {
        let Ok(step_id) = StepId::from_str(sid) else {
            continue;
        };
        let row: Option<uuid::Uuid> = sqlx::query_scalar(
            r#"
            SELECT id
            FROM step_runs
            WHERE job_run_id = $1 AND step_id = $2
            ORDER BY created_at ASC
            LIMIT 1
            "#,
        )
        .bind(job_run_id.as_uuid())
        .bind(step_id.as_uuid())
        .fetch_optional(pool)
        .await?;
        if let Some(id) = row {
            return Ok(Some(id));
        }
    }

    Ok(None)
}

async fn persist_run_binaries_for_job(
    pool: &PgPool,
    run_id: RunId,
    job_run_id: JobRunId,
    agent_id: Option<AgentId>,
    meta: &ProtoJobExecMeta,
) -> sqlx::Result<()> {
    for b in &meta.executed_binaries {
        let path = redact_exec_path_for_storage(&b.path);
        let step_run_uuid =
            resolve_step_run_uuid_for_binary_row(pool, job_run_id, &b.step_run_ids, &b.step_ids)
                .await?;

        sqlx::query(
            r#"
            INSERT INTO run_binary_executions
                (run_id, job_run_id, agent_id, binary_path, binary_sha256, pid, ppid, step_run_id)
            VALUES ($1, $2, $3, $4, $5, NULL, NULL, $6)
            "#,
        )
        .bind(run_id.as_uuid())
        .bind(job_run_id.as_uuid())
        .bind(agent_id.map(|a| a.as_uuid()))
        .bind(&path)
        .bind(&b.sha256)
        .bind(step_run_uuid)
        .execute(pool)
        .await?;
    }
    Ok(())
}

async fn ingest_telemetry_log_chunk(
    pool: &PgPool,
    run_id: RunId,
    job_run_id: JobRunId,
    step_run_id: Option<StepRunId>,
    stream: LogStream,
    text: &str,
) -> sqlx::Result<()> {
    match stream {
        LogStream::ExecBinary => {
            let v: serde_json::Value =
                serde_json::from_str(text).unwrap_or_else(|_| serde_json::json!({}));
            let path = redact_exec_path_for_storage(
                v.get("path").and_then(|x| x.as_str()).unwrap_or_default(),
            );
            let sha = v.get("sha256").and_then(|x| x.as_str()).unwrap_or_default();
            let pid = v
                .get("pid")
                .and_then(|x| x.as_i64())
                .and_then(|i| i32::try_from(i).ok());
            let ppid = v
                .get("ppid")
                .and_then(|x| x.as_i64())
                .and_then(|i| i32::try_from(i).ok());
            let step_seq = v
                .get("step_sequence")
                .and_then(|x| x.as_i64())
                .map(|i| i as i32);
            sqlx::query(
                r#"
                INSERT INTO run_binary_executions
                    (run_id, job_run_id, step_run_id, binary_path, binary_sha256,
                     pid, ppid, step_sequence)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                "#,
            )
            .bind(run_id.as_uuid())
            .bind(job_run_id.as_uuid())
            .bind(step_run_id.map(|s| s.as_uuid()))
            .bind(&path)
            .bind(sha)
            .bind(pid)
            .bind(ppid)
            .bind(step_seq)
            .execute(pool)
            .await?;
        }
        LogStream::Syscall => {
            let v: serde_json::Value =
                serde_json::from_str(text).unwrap_or_else(|_| serde_json::json!({}));
            let nr = v
                .get("nr")
                .or_else(|| v.get("syscall_nr"))
                .and_then(|x| x.as_i64())
                .unwrap_or(-1) as i32;
            let name = v
                .get("name")
                .or_else(|| v.get("syscall_name"))
                .and_then(|x| x.as_str())
                .unwrap_or("unknown")
                .to_string();
            let outcome = v
                .get("outcome")
                .and_then(|x| x.as_str())
                .unwrap_or("unknown")
                .to_string();
            let rc = v.get("return_code").and_then(|x| x.as_i64());
            let pid = v
                .get("pid")
                .and_then(|x| x.as_i64())
                .and_then(|i| i32::try_from(i).ok());
            let tid = v
                .get("tid")
                .and_then(|x| x.as_i64())
                .and_then(|i| i32::try_from(i).ok());
            sqlx::query(
                r#"
                INSERT INTO run_syscall_events
                    (run_id, job_run_id, step_run_id, syscall_nr, syscall_name,
                     outcome, return_code, pid, tid, metadata)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                "#,
            )
            .bind(run_id.as_uuid())
            .bind(job_run_id.as_uuid())
            .bind(step_run_id.map(|s| s.as_uuid()))
            .bind(nr)
            .bind(&name)
            .bind(&outcome)
            .bind(rc)
            .bind(pid)
            .bind(tid)
            .bind(v)
            .execute(pool)
            .await?;
        }
        LogStream::RuntimeScript => {
            let v: serde_json::Value =
                serde_json::from_str(text).unwrap_or_else(|_| serde_json::json!({}));
            let sha = v
                .get("sha256_hex")
                .and_then(|x| x.as_str())
                .unwrap_or_default()
                .to_string();
            let byte_length = v.get("byte_length").and_then(|x| x.as_i64()).unwrap_or(0);
            let truncated = v
                .get("truncated")
                .and_then(|x| x.as_bool())
                .unwrap_or(false);
            let object_key = v
                .get("object_key")
                .and_then(|x| x.as_str())
                .map(|s| s.to_string());
            sqlx::query(
                r#"
                INSERT INTO run_runtime_script_artifacts
                    (run_id, job_run_id, step_run_id, sha256_hex, byte_length, truncated, object_key)
                VALUES ($1, $2, $3, $4, $5, $6, $7)
                "#,
            )
            .bind(run_id.as_uuid())
            .bind(job_run_id.as_uuid())
            .bind(step_run_id.map(|s| s.as_uuid()))
            .bind(&sha)
            .bind(byte_length)
            .bind(truncated)
            .bind(object_key.as_deref())
            .execute(pool)
            .await?;
        }
        LogStream::NetworkFlow => {
            let v: serde_json::Value =
                serde_json::from_str(text).unwrap_or_else(|_| serde_json::json!({}));
            let src_ip = v
                .get("src_ip")
                .and_then(|x| x.as_str())
                .unwrap_or("0.0.0.0");
            let src_port = v
                .get("src_port")
                .and_then(|x| x.as_i64())
                .and_then(|i| i32::try_from(i).ok())
                .unwrap_or(0_i32);
            let dst_ip = v
                .get("dst_ip")
                .and_then(|x| x.as_str())
                .unwrap_or("0.0.0.0");
            let dst_port = v
                .get("dst_port")
                .and_then(|x| x.as_i64())
                .and_then(|i| i32::try_from(i).ok())
                .unwrap_or(0_i32);
            let protocol = v.get("protocol").and_then(|x| x.as_str()).unwrap_or("tcp");
            let direction = v
                .get("direction")
                .and_then(|x| x.as_str())
                .unwrap_or("observed");
            let pid = v
                .get("pid")
                .and_then(|x| x.as_i64())
                .and_then(|i| i32::try_from(i).ok());
            let binary_path = v
                .get("binary_path")
                .and_then(|x| x.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| redact_exec_path_for_storage(s));
            let binary_sha256 = v
                .get("binary_sha256")
                .and_then(|x| x.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string());
            sqlx::query(
                r#"
                INSERT INTO run_network_connections
                    (run_id, job_run_id, src_ip, src_port, dst_ip, dst_port,
                     protocol, direction, pid, binary_path, binary_sha256)
                VALUES ($1, $2, $3::inet, $4, $5::inet, $6, $7, $8, $9, $10, $11)
                "#,
            )
            .bind(run_id.as_uuid())
            .bind(job_run_id.as_uuid())
            .bind(src_ip)
            .bind(src_port)
            .bind(dst_ip)
            .bind(dst_port)
            .bind(protocol)
            .bind(direction)
            .bind(pid)
            .bind(binary_path.as_deref())
            .bind(binary_sha256.as_deref())
            .execute(pool)
            .await?;
        }
        _ => {}
    }
    Ok(())
}
