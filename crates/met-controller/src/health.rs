//! Health monitor for tracking agent liveness.

use std::sync::Arc;
use std::time::Duration;

use met_core::models::AgentStatus;
use met_store::repos::AgentRepo;
use met_store::PgPool;
use tokio::sync::watch;
use tracing::{debug, error, info, warn};

use crate::config::ControllerConfig;
use crate::registry::AgentRegistry;

/// Health monitor that detects stale agents and requeues their jobs.
pub struct HealthMonitor {
    config: ControllerConfig,
    registry: AgentRegistry,
    pool: Arc<PgPool>,
    shutdown_rx: watch::Receiver<bool>,
}

impl HealthMonitor {
    /// Create a new health monitor.
    pub fn new(
        config: ControllerConfig,
        registry: AgentRegistry,
        pool: Arc<PgPool>,
        shutdown_rx: watch::Receiver<bool>,
    ) -> Self {
        Self {
            config,
            registry,
            pool,
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

                if let Some(job_id) = agent.current_job {
                    warn!(
                        agent_id = %agent.agent_id,
                        job_id = %job_id,
                        "requeuing job from dead agent"
                    );
                    // TODO: Requeue the job via NATS
                    // This will be implemented with the full job lifecycle
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
) -> (tokio::task::JoinHandle<()>, watch::Sender<bool>) {
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    let monitor = HealthMonitor::new(config, registry, pool, shutdown_rx);
    let handle = tokio::spawn(async move {
        monitor.run().await;
    });

    (handle, shutdown_tx)
}
