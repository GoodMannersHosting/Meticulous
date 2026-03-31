//! NATS client for job dispatch and agent communication.

use std::path::Path;

use async_nats::jetstream::{self, consumer::PullConsumer, stream::Stream};
use async_nats::Client;
use met_core::ids::OrganizationId;
use prost::Message;
use tracing::{debug, error, info, instrument, warn};

use crate::error::{ControllerError, Result};

/// NATS subject hierarchy.
pub mod subjects {
    use met_core::ids::OrganizationId;

    /// Job dispatch subject pattern (pool-wide; avoid multiple JetStream consumers on WorkQueue).
    /// Format: met.jobs.{tenant_id}.{pool_tag}
    pub fn job_dispatch(org_id: OrganizationId, pool_tag: &str) -> String {
        format!("met.jobs.{}.{}", org_id.as_uuid(), pool_tag)
    }

    /// Dispatch to a specific agent inbox (WorkQueue-safe: one consumer per unique filter).
    /// Format: met.jobs.{tenant_id}.{pool_tag}.{agent_id}
    pub fn job_dispatch_to_agent(org_id: OrganizationId, pool_tag: &str, agent_id: &str) -> String {
        format!("met.jobs.{}.{}.{}", org_id.as_uuid(), pool_tag, agent_id)
    }

    /// Pull consumer filter: all pool tags for this agent (`*` = single subject token).
    /// Format: met.jobs.{tenant_id}.*.{agent_id}
    pub fn job_inbox_filter(org_id: OrganizationId, agent_id: &str) -> String {
        format!("met.jobs.{}.*.{}", org_id.as_uuid(), agent_id)
    }

    /// Default pool job dispatch subject.
    pub fn job_dispatch_default(org_id: OrganizationId) -> String {
        job_dispatch(org_id, "_default")
    }

    /// Agent status subject.
    /// Format: met.status.{tenant_id}.{agent_id}
    pub fn agent_status(org_id: OrganizationId, agent_id: &str) -> String {
        format!("met.status.{}.{}", org_id.as_uuid(), agent_id)
    }

    /// Job cancellation subject.
    /// Format: met.cancel.{tenant_id}.{job_id}
    pub fn job_cancel(org_id: OrganizationId, job_id: &str) -> String {
        format!("met.cancel.{}.{}", org_id.as_uuid(), job_id)
    }

    /// Broadcast subject for all agents in a tenant.
    pub fn broadcast(org_id: OrganizationId) -> String {
        format!("met.broadcast.{}", org_id.as_uuid())
    }

    /// Stream names.
    pub const JOBS_STREAM: &str = "JOBS";
    pub const STATUS_STREAM: &str = "STATUS";
    pub const CANCEL_STREAM: &str = "CANCEL";
}

/// NATS dispatcher for job dispatch and agent communication.
#[derive(Clone)]
pub struct NatsDispatcher {
    client: Client,
    jetstream: jetstream::Context,
}

impl NatsDispatcher {
    /// Connect to NATS and create the dispatcher.
    ///
    /// When `creds_path` is set, authenticates with a `.creds` file (JWT + NKey).
    pub async fn connect(url: &str, creds_path: Option<&Path>) -> Result<Self> {
        info!(url, "connecting to NATS");
        let client = if let Some(path) = creds_path {
            let opts = async_nats::ConnectOptions::with_credentials_file(path)
                .await
                .map_err(|e| ControllerError::Nats(format!("load NATS creds {}: {e}", path.display())))?;
            opts.connect(url)
                .await
                .map_err(|e| ControllerError::Nats(e.to_string()))?
        } else {
            async_nats::connect(url)
                .await
                .map_err(|e| ControllerError::Nats(e.to_string()))?
        };
        let jetstream = jetstream::new(client.clone());

        let dispatcher = Self { client, jetstream };

        // Ensure streams exist
        dispatcher.ensure_streams().await?;

        Ok(dispatcher)
    }

    /// Ensure required JetStream streams exist.
    async fn ensure_streams(&self) -> Result<()> {
        // JOBS stream for job dispatch
        self.ensure_stream(
            subjects::JOBS_STREAM,
            &["met.jobs.>"],
            jetstream::stream::RetentionPolicy::WorkQueue,
            Some(std::time::Duration::from_secs(24 * 60 * 60)), // 24h
        )
        .await?;

        // STATUS stream for agent status
        self.ensure_stream(
            subjects::STATUS_STREAM,
            &["met.status.>"],
            jetstream::stream::RetentionPolicy::Limits,
            Some(std::time::Duration::from_secs(60 * 60)), // 1h
        )
        .await?;

        // CANCEL stream for job cancellation
        self.ensure_stream(
            subjects::CANCEL_STREAM,
            &["met.cancel.>"],
            jetstream::stream::RetentionPolicy::Interest,
            Some(std::time::Duration::from_secs(60 * 60)), // 1h
        )
        .await?;

        Ok(())
    }

    /// Ensure a stream exists with the given configuration.
    async fn ensure_stream(
        &self,
        name: &str,
        subjects: &[&str],
        retention: jetstream::stream::RetentionPolicy,
        max_age: Option<std::time::Duration>,
    ) -> Result<Stream> {
        let config = jetstream::stream::Config {
            name: name.to_string(),
            subjects: subjects.iter().map(|s| s.to_string()).collect(),
            retention,
            max_age: max_age.unwrap_or_default(),
            ..Default::default()
        };

        match self.jetstream.get_stream(name).await {
            Ok(stream) => {
                debug!(stream = name, "stream already exists");
                Ok(stream)
            }
            Err(_) => {
                info!(stream = name, "creating stream");
                let stream = self.jetstream
                    .create_stream(config)
                    .await
                    .map_err(|e| ControllerError::Nats(e.to_string()))?;
                Ok(stream)
            }
        }
    }

    /// Publish a job dispatch message.
    #[instrument(skip(self, message))]
    pub async fn dispatch_job(
        &self,
        org_id: OrganizationId,
        pool_tag: &str,
        agent_id: &str,
        message: &met_proto::controller::v1::JobDispatch,
    ) -> Result<()> {
        let pool = if pool_tag.is_empty() {
            "_default"
        } else {
            pool_tag
        };
        let subject = subjects::job_dispatch_to_agent(org_id, pool, agent_id);

        let payload = message.encode_to_vec();

        debug!(
            subject,
            job_run_id = message.job_run_id,
            "dispatching job"
        );

        self.jetstream
            .publish(subject, payload.into())
            .await
            .map_err(|e| ControllerError::Nats(e.to_string()))?
            .await
            .map_err(|e| ControllerError::Internal(format!("publish ack error: {e}")))?;

        Ok(())
    }

    /// Publish a job cancellation message.
    #[instrument(skip(self))]
    pub async fn cancel_job(&self, org_id: OrganizationId, job_id: &str) -> Result<()> {
        let subject = subjects::job_cancel(org_id, job_id);
        let payload = job_id.as_bytes().to_vec();

        debug!(subject, job_id, "cancelling job");

        self.jetstream
            .publish(subject, payload.into())
            .await
            .map_err(|e| ControllerError::Nats(e.to_string()))?
            .await
            .map_err(|e| ControllerError::Internal(format!("publish ack error: {e}")))?;

        Ok(())
    }

    /// Broadcast a message to all agents in an organization.
    #[instrument(skip(self, payload))]
    pub async fn broadcast(&self, org_id: OrganizationId, payload: Vec<u8>) -> Result<()> {
        let subject = subjects::broadcast(org_id);

        debug!(subject, "broadcasting to agents");

        self.client
            .publish(subject, payload.into())
            .await
            .map_err(|e| ControllerError::Nats(e.to_string()))?;

        Ok(())
    }

    /// Create a pull consumer for job dispatch in a pool.
    pub async fn create_job_consumer(
        &self,
        org_id: OrganizationId,
        pool_tag: &str,
        agent_id: &str,
    ) -> Result<PullConsumer> {
        let stream = self.jetstream
            .get_stream(subjects::JOBS_STREAM)
            .await
            .map_err(|e| ControllerError::Nats(e.to_string()))?;

        let consumer_name = format!("pool-{}-{}-{}", org_id.as_uuid(), pool_tag, agent_id);
        let filter = subjects::job_dispatch_to_agent(org_id, pool_tag, agent_id);

        let config = jetstream::consumer::pull::Config {
            name: Some(consumer_name.clone()),
            durable_name: Some(consumer_name),
            filter_subject: filter,
            ack_policy: jetstream::consumer::AckPolicy::Explicit,
            ack_wait: std::time::Duration::from_secs(30),
            max_deliver: 3,
            ..Default::default()
        };

        let consumer = stream
            .create_consumer(config)
            .await
            .map_err(|e| ControllerError::Nats(e.to_string()))?;

        Ok(consumer)
    }

    /// Get the underlying NATS client.
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// Get the JetStream context.
    pub fn jetstream(&self) -> &jetstream::Context {
        &self.jetstream
    }

    /// Delete the per-agent pull consumer on the JOBS stream (matches `met-agent` naming: `agent-{agent_id}`).
    pub async fn delete_agent_pull_consumer(&self, agent_id: &str) -> Result<()> {
        let stream = self
            .jetstream
            .get_stream(subjects::JOBS_STREAM)
            .await
            .map_err(|e| ControllerError::Nats(e.to_string()))?;
        let name = format!("agent-{agent_id}");
        stream
            .delete_consumer(&name)
            .await
            .map_err(|e| ControllerError::Nats(e.to_string()))?;
        info!(consumer = %name, stream = subjects::JOBS_STREAM, "deleted agent JetStream consumer");
        Ok(())
    }

    /// Best-effort cleanup on the JOBS WorkQueue stream so operators never need manual `nats consumer` commands:
    /// - Deletes legacy `pool-{org}-{pool_tag}-{agent_id}` pull consumers (older layout / tests).
    /// - Deletes `agent-{agent_id}` when its filter no longer matches [`subjects::job_inbox_filter`]
    ///   (e.g. stale `met.jobs.{org}._default`), so the agent can recreate with the correct filter.
    pub async fn reconcile_jobs_consumers_for_agent(
        &self,
        org_id: OrganizationId,
        agent_id: &str,
        pool_tags: &[String],
    ) -> Result<()> {
        let stream = self
            .jetstream
            .get_stream(subjects::JOBS_STREAM)
            .await
            .map_err(|e| ControllerError::Nats(e.to_string()))?;

        let mut tags: Vec<&str> = pool_tags.iter().map(|s| s.as_str()).collect();
        if tags.is_empty() {
            tags.push("_default");
        }

        for pt in tags {
            let name = format!("pool-{}-{}-{}", org_id.as_uuid(), pt, agent_id);
            match stream.delete_consumer(&name).await {
                Ok(_) => info!(
                    consumer = %name,
                    stream = subjects::JOBS_STREAM,
                    "removed legacy pool-prefixed JetStream consumer"
                ),
                Err(e) => {
                    debug!(
                        consumer = %name,
                        error = %e,
                        "pool-prefixed consumer absent or delete failed (expected if none existed)"
                    );
                }
            }
        }

        let agent_c = format!("agent-{agent_id}");
        let expected = subjects::job_inbox_filter(org_id, agent_id);
        match stream.consumer_info(&agent_c).await {
            Ok(info) => {
                let filter_ok = if !info.config.filter_subjects.is_empty() {
                    info.config.filter_subjects.iter().any(|s| s == &expected)
                } else {
                    info.config.filter_subject == expected
                };
                if !filter_ok {
                    match stream.delete_consumer(&agent_c).await {
                        Ok(_) => info!(
                            consumer = %agent_c,
                            stale_filter = %info.config.filter_subject,
                            expected_filter = %expected,
                            "removed agent JetStream consumer with stale filter (agent recreates on next pull)"
                        ),
                        Err(e) => warn!(
                            consumer = %agent_c,
                            error = %e,
                            "failed to delete stale agent JetStream consumer"
                        ),
                    }
                }
            }
            Err(_) => {
                debug!(consumer = %agent_c, "no existing agent JetStream consumer to inspect");
            }
        }

        Ok(())
    }

    /// Close the connection.
    pub async fn close(self) {
        if let Err(e) = self.client.drain().await {
            error!(error = %e, "error draining NATS connection");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subject_formatting() {
        let org_id = OrganizationId::new();

        let subject = subjects::job_dispatch(org_id, "docker");
        assert!(subject.starts_with("met.jobs."));
        assert!(subject.ends_with(".docker"));

        let default_subject = subjects::job_dispatch_default(org_id);
        assert!(default_subject.ends_with("._default"));

        let aid = met_core::ids::AgentId::new();
        let aid_s = aid.to_string();
        let to_agent = subjects::job_dispatch_to_agent(org_id, "docker", &aid_s);
        assert!(to_agent.ends_with(&format!(".docker.{}", aid_s)));

        let inbox = subjects::job_inbox_filter(org_id, &aid_s);
        assert!(inbox.contains(".*."));
    }
}
