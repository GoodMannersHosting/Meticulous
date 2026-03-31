//! Agent registration with the controller.

use met_proto::agent::v1::{
    agent_service_client::AgentServiceClient, AgentCapabilities, RegisterRequest, SecurityBundle,
};
use tonic::transport::Channel;
use tracing::{debug, info, warn};

use crate::config::{AgentConfig, AgentIdentity};
use crate::error::{AgentError, Result};
use crate::security::SecurityBundleCollector;

/// How the agent obtained its identity for this run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegistrationSource {
    /// Read from `agent-identity.json` (no `Register` RPC this run).
    LoadedFromDisk,
    /// Completed `Register` with the controller and persisted identity.
    RegisteredWithController,
}

/// Handles agent registration with the controller.
pub struct AgentRegistration {
    config: AgentConfig,
    client: AgentServiceClient<Channel>,
}

impl AgentRegistration {
    /// Create a new registration handler.
    pub async fn new(config: AgentConfig) -> Result<Self> {
        let client = AgentServiceClient::connect(config.controller_url.clone()).await?;

        Ok(Self { config, client })
    }

    /// Register the agent or load existing identity.
    ///
    /// Set `force_register` (CLI `--force-register` / `MET_FORCE_REGISTER=1`) to ignore any file
    /// under [`AgentConfig::identity_path`] and call [`register`](Self::register).
    pub async fn register_or_load(
        &mut self,
        force_register: bool,
    ) -> Result<(AgentIdentity, RegistrationSource)> {
        let identity_path = self.config.identity_path();

        if force_register {
            info!(
                "force register: removing cached identity if present and registering with controller"
            );
            if identity_path.exists() {
                if let Err(e) = std::fs::remove_file(&identity_path) {
                    warn!(error = %e, path = %identity_path.display(), "failed to remove cached identity");
                }
            }
            let identity = self.register().await?;
            return Ok((identity, RegistrationSource::RegisteredWithController));
        }

        // Try to load existing identity
        if let Some(identity) = AgentIdentity::load(&identity_path)? {
            if !identity.is_jwt_expired() {
                info!(
                    agent_id = identity.agent_id,
                    "loaded existing agent identity (join token was not used; use MET_FORCE_REGISTER=1 to enroll again)"
                );
                return Ok((identity, RegistrationSource::LoadedFromDisk));
            }

            if identity.renewable {
                info!(
                    agent_id = identity.agent_id,
                    "JWT expired, will try to renew via heartbeat"
                );
                // Return the identity anyway - heartbeat will renew
                return Ok((identity, RegistrationSource::LoadedFromDisk));
            }

            warn!(
                agent_id = identity.agent_id,
                "JWT expired and non-renewable, re-registering"
            );
        }

        // Need to register
        let identity = self.register().await?;
        Ok((identity, RegistrationSource::RegisteredWithController))
    }

    /// Register the agent with the controller.
    async fn register(&mut self) -> Result<AgentIdentity> {
        let join_token = self
            .config
            .join_token
            .clone()
            .ok_or_else(|| AgentError::Config("join_token required for registration".to_string()))?;

        info!("registering agent with controller");

        // Collect security bundle
        let collector = SecurityBundleCollector::new();
        let bundle = collector.collect().await;

        // Build capabilities
        let capabilities = AgentCapabilities {
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            labels: self.config.labels.clone(),
            pool_tags: self.config.pool_tags.clone(),
        };

        // Create security bundle proto
        let security_bundle = SecurityBundle {
            hostname: bundle.hostname,
            os: bundle.os,
            arch: bundle.arch,
            kernel_version: bundle.kernel_version,
            public_ips: bundle.public_ips,
            private_ips: bundle.private_ips,
            ntp_synchronized: bundle.ntp_synchronized,
            container_runtime: bundle.container_runtime,
            container_runtime_version: bundle.container_runtime_version,
            environment_type: bundle.environment_type as i32,
            agent_x509_public_key: bundle.x509_public_key,
        };

        // Send registration request
        let request = RegisterRequest {
            join_token,
            security_bundle: Some(security_bundle),
            capabilities: Some(capabilities),
        };

        let response = self
            .client
            .register(request)
            .await
            .map_err(|e| {
                if e.code() == tonic::Code::Unauthenticated {
                    AgentError::Registration(e.message().to_string())
                } else {
                    AgentError::Grpc(e)
                }
            })?
            .into_inner();

        info!(
            agent_id = response.agent_id,
            nats_subjects = ?response.nats_subjects,
            "registration successful"
        );

        let creds = response.nats_credentials.as_ref();
        let (nats_user_jwt, nats_user_seed) = creds
            .map(|c| {
                let jwt = c.jwt.trim();
                let seed = c.nkey_seed.trim();
                let jwt_opt = (!jwt.is_empty()).then(|| c.jwt.clone());
                let seed_opt = (!seed.is_empty()).then(|| c.nkey_seed.clone());
                (jwt_opt, seed_opt)
            })
            .unwrap_or((None, None));

        // Build identity
        let identity = AgentIdentity {
            agent_id: response.agent_id,
            org_id: response.organization_id,
            jwt_token: response.jwt_token,
            jwt_expires_at: response
                .jwt_expires_at
                .map(|t| t.seconds)
                .unwrap_or(0),
            renewable: response.renewable,
            nats_subjects: response.nats_subjects,
            nats_url: creds
                .map(|c| c.url.clone())
                .filter(|u| !u.trim().is_empty())
                .unwrap_or_else(|| "nats://localhost:4222".to_string()),
            nats_user_jwt,
            nats_user_seed,
        };

        // Persist identity
        let identity_path = self.config.identity_path();
        identity.save(&identity_path)?;
        debug!(path = %identity_path.display(), "saved agent identity");

        Ok(identity)
    }

    /// Get the gRPC client.
    pub fn client(&mut self) -> &mut AgentServiceClient<Channel> {
        &mut self.client
    }
}
