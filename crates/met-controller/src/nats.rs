//! NATS client for job dispatch and agent communication.

use std::path::Path;
use std::time::Duration;

use async_nats::Client;
use async_nats::jetstream::{self, consumer::PullConsumer, stream::Stream};
use futures::StreamExt;
use met_core::ids::OrganizationId;
use met_proto::controller::v1::JobCompletion;
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
    /// Pipeline job completion notifications for the engine (protobuf `JobCompletion`).
    pub const COMPLETIONS_STREAM: &str = "COMPLETIONS";

    /// Undeliverable / poison job dispatches and advisory copies (limits retention; ops triage).
    pub const JOBS_DLQ_STREAM: &str = "JOBS_DLQ";

    /// `met.completions.{org_id}` — pipeline engine completion consumers filter on this prefix.
    pub fn job_completion(org_id: OrganizationId) -> String {
        format!("met.completions.{}", org_id.as_uuid())
    }

    /// `met.dlq.jobs.{org_id}` — JSON envelope for support / replay tooling.
    ///
    /// **Must not** use `met.jobs.dlq.*`: the `JOBS` stream owns `met.jobs.>`, and JetStream
    /// rejects overlapping stream subjects (error 10065).
    pub fn jobs_dlq(org_id: OrganizationId) -> String {
        format!("met.dlq.jobs.{}", org_id.as_uuid())
    }
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
                .map_err(|e| {
                    ControllerError::Nats(format!("load NATS creds {}: {e}", path.display()))
                })?;
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

    /// Forward JetStream **max deliveries** advisories for the `JOBS` stream into `JOBS_DLQ`
    /// ([`subjects::jobs_dlq`] / `met.dlq.jobs.{org}`; nil UUID when org is unknown).
    pub fn spawn_max_deliveries_dlq_forwarder(&self) {
        let nats = self.clone();
        tokio::spawn(async move {
            if let Err(e) = run_max_deliveries_dlq_forwarder(nats).await {
                error!(error = %e, "DLQ advisory forwarder exited");
            }
        });
    }

    /// Fetch up to `limit` recent JSON payloads from `JOBS_DLQ` for [`subjects::jobs_dlq`] (admin/ops).
    pub async fn fetch_recent_jobs_dlq(
        &self,
        org_id: OrganizationId,
        limit: u32,
    ) -> Result<Vec<serde_json::Value>> {
        let limit_u = limit.clamp(1, 500) as u64;
        let mut stream = self
            .jetstream
            .get_stream(subjects::JOBS_DLQ_STREAM)
            .await
            .map_err(|e| ControllerError::Nats(e.to_string()))?;
        let info = stream
            .info()
            .await
            .map_err(|e| ControllerError::Nats(e.to_string()))?;
        let last = info.state.last_sequence;
        if last == 0 {
            return Ok(Vec::new());
        }
        let start = last.saturating_sub(limit_u - 1).max(1);
        let filter = subjects::jobs_dlq(org_id);
        let consumer_name = format!("dlq-preview-{}", uuid::Uuid::new_v4());
        let consumer = stream
            .create_consumer(jetstream::consumer::pull::Config {
                name: Some(consumer_name.clone()),
                deliver_policy: jetstream::consumer::DeliverPolicy::ByStartSequence {
                    start_sequence: start,
                },
                filter_subject: filter,
                ack_policy: jetstream::consumer::AckPolicy::Explicit,
                inactive_threshold: Duration::from_secs(30),
                max_ack_pending: 1024,
                ..Default::default()
            })
            .await
            .map_err(|e| ControllerError::Nats(e.to_string()))?;

        let mut messages = consumer
            .messages()
            .await
            .map_err(|e| ControllerError::Nats(e.to_string()))?;

        let mut out = Vec::new();
        while (out.len() as u64) < limit_u {
            match tokio::time::timeout(Duration::from_secs(3), messages.next()).await {
                Ok(Some(Ok(m))) => {
                    if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&m.payload) {
                        out.push(v);
                    }
                    let _ = m.ack().await;
                }
                Ok(Some(Err(e))) => {
                    warn!(error = %e, "DLQ preview pull error");
                    break;
                }
                Ok(None) => break,
                Err(_) => break,
            }
        }

        let _ = stream.delete_consumer(&consumer_name).await;

        Ok(out)
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

        self.ensure_stream(
            subjects::COMPLETIONS_STREAM,
            &["met.completions.>"],
            jetstream::stream::RetentionPolicy::Limits,
            Some(std::time::Duration::from_secs(24 * 60 * 60)),
        )
        .await?;

        self.ensure_stream(
            subjects::JOBS_DLQ_STREAM,
            &["met.dlq.jobs.>"],
            jetstream::stream::RetentionPolicy::Limits,
            Some(std::time::Duration::from_secs(14 * 24 * 60 * 60)),
        )
        .await?;

        Ok(())
    }

    /// Publish a JSON record to the jobs dead-letter stream (operator triage).
    pub async fn publish_jobs_dlq(
        &self,
        org_id: OrganizationId,
        event: serde_json::Value,
    ) -> Result<()> {
        let subject = subjects::jobs_dlq(org_id);
        let payload = serde_json::to_vec(&event)
            .map_err(|e| ControllerError::Internal(format!("dlq json: {e}")))?;
        self.jetstream
            .publish(subject.clone(), payload.into())
            .await
            .map_err(|e| ControllerError::Nats(format!("publish {subject}: {e}")))?
            .await
            .map_err(|e| ControllerError::Nats(format!("publish ack {subject}: {e}")))?;
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
                let stream = self
                    .jetstream
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

        debug!(subject, job_run_id = message.job_run_id, "dispatching job");

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
        let stream = self
            .jetstream
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

    /// Publish a protobuf job completion to JetStream (`COMPLETIONS` / `met.completions.{org}`).
    pub async fn publish_job_completion_proto(
        &self,
        org_id: OrganizationId,
        message: &JobCompletion,
    ) -> Result<()> {
        let subject = subjects::job_completion(org_id);
        let payload = message.encode_to_vec();
        self.jetstream
            .publish(subject.clone(), payload.into())
            .await
            .map_err(|e| ControllerError::Nats(format!("publish {subject}: {e}")))?
            .await
            .map_err(|e| ControllerError::Nats(format!("publish ack {subject}: {e}")))?;
        Ok(())
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

async fn run_max_deliveries_dlq_forwarder(nats: NatsDispatcher) -> Result<()> {
    let mut subscriber = nats
        .client()
        .subscribe("$JS.EVENT.ADVISORY.>")
        .await
        .map_err(|e| ControllerError::Nats(e.to_string()))?;

    info!("subscribed to JetStream advisory events (JOBS max deliveries → DLQ)");

    while let Some(message) = subscriber.next().await {
        let Ok(v) = serde_json::from_slice::<serde_json::Value>(&message.payload) else {
            continue;
        };
        let type_str = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
        if !type_str.contains("max_deliver") {
            continue;
        }
        let stream_name = v.get("stream").and_then(|s| s.as_str()).unwrap_or("");
        if stream_name != subjects::JOBS_STREAM {
            continue;
        }
        let org = OrganizationId::from_uuid(uuid::Uuid::nil());
        let event = serde_json::json!({
            "source": "jetstream_consumer_max_deliveries",
            "advisory": v,
        });
        if let Err(e) = nats.publish_jobs_dlq(org, event).await {
            warn!(error = %e, "failed to publish DLQ record from advisory");
        }
    }
    Ok(())
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
