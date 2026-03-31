//! Health monitor for tracking agent liveness.

use std::sync::Arc;

use met_core::ids::JobRunId;
use met_core::models::AgentStatus;
use met_store::repos::{AgentRepo, JobRunRepo};
use met_store::PgPool;
use tokio::sync::watch;
use tracing::{debug, error, info, warn};

use crate::config::ControllerConfig;
use crate::nats::NatsDispatcher;
use crate::registry::AgentRegistry;

/// Health monitor that detects stale agents and requeues their jobs.
pub struct HealthMonitor {
    config: ControllerConfig,
    registry: AgentRegistry,
    pool: Arc<PgPool>,
    nats: NatsDispatcher,
    shutdown_rx: watch::Receiver<bool>,
}

impl HealthMonitor {
    /// Create a new health monitor.
    pub fn new(
        config: ControllerConfig,
        registry: AgentRegistry,
        pool: Arc<PgPool>,
        nats: NatsDispatcher,
        shutdown_rx: watch::Receiver<bool>,
    ) -> Self {
        Self {
            config,
            registry,
            pool,
            nats,
            shutdown_rx,
        }
    }

    /// Start the health monitor loop.
    pub async fn run(mut self) {
        info!(
            interval = ?self.config.health_check_interval,
            stale_threshold = ?self.config.stale_threshold,
            dead_threshold = ?self.config.dead_threshold,
            "starting health monitor"
        );

        let mut interval = tokio::time::interval(self.config.health_check_interval);

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if let Err(e) = self.check_health().await {
                        error!(error = %e, "health check failed");
                    }
                }
                _ = self.shutdown_rx.changed() => {
                    if *self.shutdown_rx.borrow() {
                        info!("health monitor shutting down");
                        break;
                    }
                }
            }
        }
    }

    /// Perform a health check cycle.
    async fn check_health(&self) -> crate::Result<()> {
        // Find stale agents (missed heartbeat threshold)
        let stale_agents = self.registry.find_stale(self.config.stale_threshold).await;

        for agent in stale_agents {
            if agent.last_heartbeat.elapsed() > self.config.dead_threshold {
                // Agent is dead - mark and requeue jobs
                warn!(
                    agent_id = %agent.agent_id,
                    last_heartbeat = ?agent.last_heartbeat.elapsed(),
                    "marking agent as dead"
                );

                self.mark_dead(agent.agent_id).await?;

                if let Some(job_run_id) = agent.current_job {
                    warn!(
                        agent_id = %agent.agent_id,
                        job_run_id = %job_run_id,
                        "requeuing job from dead agent"
                    );
                    
                    // Requeue the job via NATS
                    if let Err(e) = self.requeue_job(agent.org_id, job_run_id).await {
                        error!(
                            error = %e,
                            job_run_id = %job_run_id,
                            "failed to requeue job from dead agent"
                        );
                    }
                }
            } else {
                // Agent is stale but not dead - mark offline
                debug!(
                    agent_id = %agent.agent_id,
                    last_heartbeat = ?agent.last_heartbeat.elapsed(),
                    "marking agent as offline"
                );

                self.mark_offline(agent.agent_id).await?;
            }
        }

        // Log statistics
        let counts = self.registry.count_by_status().await;
        debug!(
            online = counts.get(&AgentStatus::Online).unwrap_or(&0),
            busy = counts.get(&AgentStatus::Busy).unwrap_or(&0),
            offline = counts.get(&AgentStatus::Offline).unwrap_or(&0),
            dead = counts.get(&AgentStatus::Dead).unwrap_or(&0),
            "agent health check complete"
        );

        Ok(())
    }

    /// Requeue a job from a dead agent via NATS.
    async fn requeue_job(
        &self,
        org_id: met_core::ids::OrganizationId,
        job_run_id: JobRunId,
    ) -> crate::Result<()> {
        let job_run_repo = JobRunRepo::new(&self.pool);

        // Get the job run to check retry eligibility
        let job_run = job_run_repo.get(job_run_id).await?;

        // Check if we've exceeded max retries (default max_deliver is 3)
        const MAX_RETRIES: i32 = 3;
        if job_run.attempt >= MAX_RETRIES {
            warn!(
                job_run_id = %job_run_id,
                attempt = job_run.attempt,
                max_retries = MAX_RETRIES,
                "job exceeded max retries, marking as failed"
            );

            job_run_repo
                .mark_completed(
                    job_run_id,
                    false,
                    None,
                    Some("Exceeded maximum retry attempts due to agent failures"),
                    None,
                )
                .await?;

            return Ok(());
        }

        // Increment attempt counter and reset status to pending
        let updated_job_run = job_run_repo.increment_attempt(job_run_id).await?;

        info!(
            job_run_id = %job_run_id,
            attempt = updated_job_run.attempt,
            "requeuing job for retry"
        );

        // Publish requeue event to NATS for the engine to pick up
        let requeue_event = serde_json::json!({
            "type": "job.requeue",
            "job_run_id": job_run_id.to_string(),
            "run_id": job_run.run_id.to_string(),
            "attempt": updated_job_run.attempt,
            "reason": "agent_failure",
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });

        self.nats
            .client()
            .publish(
                format!("met.engine.requeue.{}", org_id.as_uuid()),
                serde_json::to_vec(&requeue_event)
                    .unwrap_or_default()
                    .into(),
            )
            .await
            .map_err(|e| crate::error::ControllerError::Nats(e.to_string()))?;

        Ok(())
    }

    /// Mark an agent as offline.
    async fn mark_offline(&self, agent_id: met_core::ids::AgentId) -> crate::Result<()> {
        // Update registry
        self.registry
            .update_status(agent_id, AgentStatus::Offline)
            .await;

        // Update database
        let repo = AgentRepo::new(&self.pool);
        repo.update_status(agent_id, AgentStatus::Offline).await?;

        Ok(())
    }

    /// Mark an agent as dead.
    async fn mark_dead(&self, agent_id: met_core::ids::AgentId) -> crate::Result<()> {
        // Update registry
        self.registry
            .update_status(agent_id, AgentStatus::Dead)
            .await;

        // Update database
        let repo = AgentRepo::new(&self.pool);
        repo.update_status(agent_id, AgentStatus::Dead).await?;

        Ok(())
    }
}

/// Start the health monitor in a background task.
pub fn spawn_health_monitor(
    config: ControllerConfig,
    registry: AgentRegistry,
    pool: Arc<PgPool>,
    nats: NatsDispatcher,
) -> (tokio::task::JoinHandle<()>, watch::Sender<bool>) {
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    let monitor = HealthMonitor::new(config, registry, pool, nats, shutdown_rx);
    let handle = tokio::spawn(async move {
        monitor.run().await;
    });

    (handle, shutdown_tx)
}
