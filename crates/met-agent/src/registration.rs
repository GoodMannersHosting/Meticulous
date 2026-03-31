//! Agent registration with the controller.

use met_proto::agent::v1::{
    agent_service_client::AgentServiceClient, AgentCapabilities, RegisterRequest, SecurityBundle,
};
use met_proto::AgentStatus;
use tonic::transport::Channel;
use tracing::{debug, info, warn};

use crate::config::{AgentConfig, AgentIdentity};
use crate::error::{AgentError, Result};
use crate::security::SecurityBundleCollector;

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
    pub async fn register_or_load(&mut self) -> Result<AgentIdentity> {
        let identity_path = self.config.identity_path();

        // Try to load existing identity
        if let Some(identity) = AgentIdentity::load(&identity_path)? {
            if !identity.is_jwt_expired() {
                info!(
                    agent_id = identity.agent_id,
                    "loaded existing agent identity"
                );
                return Ok(identity);
            }

            if identity.renewable {
                info!(
                    agent_id = identity.agent_id,
                    "JWT expired, will try to renew via heartbeat"
                );
                // Return the identity anyway - heartbeat will renew
                return Ok(identity);
            }

            warn!(
                agent_id = identity.agent_id,
                "JWT expired and non-renewable, re-registering"
            );
        }

        // Need to register
        self.register().await
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

        let response = self.client.register(request).await?.into_inner();

        info!(
            agent_id = response.agent_id,
            nats_subjects = ?response.nats_subjects,
            "registration successful"
        );

        // Build identity
        let identity = AgentIdentity {
            agent_id: response.agent_id,
            org_id: String::new(), // Not returned in response, derived from token
            jwt_token: response.jwt_token,
            jwt_expires_at: response
                .jwt_expires_at
                .map(|t| t.seconds)
                .unwrap_or(0),
            renewable: response.renewable,
            nats_subjects: response.nats_subjects,
            nats_url: response
                .nats_credentials
                .map(|c| c.url)
                .unwrap_or_else(|| "nats://localhost:4222".to_string()),
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
