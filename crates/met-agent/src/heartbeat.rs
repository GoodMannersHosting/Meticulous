//! Heartbeat loop for agent liveness.

use std::sync::Arc;
use std::time::Duration;

use met_proto::agent::v1::{
    agent_service_client::AgentServiceClient, AgentStatusInfo, HeartbeatAction, HeartbeatRequest,
    ResourceSnapshot,
};
use met_proto::AgentStatus;
use sysinfo::System;
use tokio::sync::{mpsc, watch, RwLock};
use tonic::transport::Channel;
use tracing::{debug, error, info, warn};

use crate::config::AgentIdentity;
use crate::error::{AgentError, Result};

/// Heartbeat state shared with the main loop.
#[derive(Debug, Clone)]
pub struct HeartbeatState {
    pub status: AgentStatus,
    pub running_jobs: i32,
    pub current_job_id: Option<String>,
}

impl Default for HeartbeatState {
    fn default() -> Self {
        Self {
            status: AgentStatus::Online,
            running_jobs: 0,
            current_job_id: None,
        }
    }
}

/// Heartbeat loop that sends periodic heartbeats to the controller.
pub struct HeartbeatLoop {
    client: AgentServiceClient<Channel>,
    identity: AgentIdentity,
    interval: Duration,
    state: Arc<RwLock<HeartbeatState>>,
    shutdown_rx: watch::Receiver<bool>,
    action_tx: mpsc::Sender<HeartbeatAction>,
}

impl HeartbeatLoop {
    /// Create a new heartbeat loop.
    pub fn new(
        client: AgentServiceClient<Channel>,
        identity: AgentIdentity,
        interval: Duration,
        state: Arc<RwLock<HeartbeatState>>,
        shutdown_rx: watch::Receiver<bool>,
        action_tx: mpsc::Sender<HeartbeatAction>,
    ) -> Self {
        Self {
            client,
            identity,
            interval,
            state,
            shutdown_rx,
            action_tx,
        }
    }

    /// Run the heartbeat loop.
    pub async fn run(mut self) -> Result<()> {
        info!(
            interval = ?self.interval,
            "starting heartbeat loop"
        );

        let mut interval = tokio::time::interval(self.interval);
        let mut system = System::new_all();

        'hb: loop {
            if *self.shutdown_rx.borrow() {
                info!("heartbeat loop shutting down");
                break;
            }

            tokio::select! {
                _ = interval.tick() => {
                    if *self.shutdown_rx.borrow() {
                        break 'hb;
                    }
                    // Do not block shutdown on a slow gRPC heartbeat: use a cloned watch receiver
                    // so this `select!` does not borrow `self` mutably for `changed()` and `send_heartbeat` at once.
                    let mut shutdown_wake = self.shutdown_rx.clone();
                    tokio::select! {
                        _ = shutdown_wake.changed() => {
                            if *shutdown_wake.borrow() {
                                info!("heartbeat loop shutting down");
                                break 'hb;
                            }
                        }
                        res = self.send_heartbeat(&mut system) => {
                            match res {
                                Ok(action) => {
                                    if action != HeartbeatAction::Continue {
                                        info!(action = ?action, "received heartbeat action");
                                        if self.action_tx.send(action).await.is_err() {
                                            break 'hb;
                                        }
                                    }
                                }
                                Err(e) => {
                                    error!(error = %e, "heartbeat failed");
                                }
                            }
                        }
                    }
                }
                _ = self.shutdown_rx.changed() => {
                    if *self.shutdown_rx.borrow() {
                        info!("heartbeat loop shutting down");
                        break;
                    }
                }
            }
        }

        Ok(())
    }

    /// Send a single heartbeat.
    async fn send_heartbeat(&mut self, system: &mut System) -> Result<HeartbeatAction> {
        // Refresh system info
        system.refresh_all();

        let state = self.state.read().await;

        // Collect resource snapshot
        let resources = ResourceSnapshot {
            cpu_percent: system.global_cpu_usage() / 100.0,
            memory_percent: system.used_memory() as f32 / system.total_memory() as f32,
            disk_percent: 0.0, // Would need to check specific mount points
            available_memory_bytes: system.available_memory() as i64,
            available_disk_bytes: 0,
        };

        let status_info = AgentStatusInfo {
            status: state.status as i32,
            running_jobs: state.running_jobs,
            queued_jobs: 0,
        };

        let request = HeartbeatRequest {
            agent_id: self.identity.agent_id.clone(),
            status: Some(status_info),
            resources: Some(resources),
            current_job_id: state.current_job_id.clone(),
        };

        drop(state);

        let response = self.client.heartbeat(request).await?.into_inner();

        // Handle JWT renewal if provided
        if let Some(new_jwt) = response.new_jwt_token {
            debug!("received renewed JWT token");
            // Would update the identity here
        }

        let action = HeartbeatAction::try_from(response.action)
            .unwrap_or(HeartbeatAction::Continue);

        Ok(action)
    }
}

/// Spawn the heartbeat loop in a background task.
pub fn spawn_heartbeat_loop(
    client: AgentServiceClient<Channel>,
    identity: AgentIdentity,
    interval: Duration,
    state: Arc<RwLock<HeartbeatState>>,
) -> (
    tokio::task::JoinHandle<Result<()>>,
    watch::Sender<bool>,
    mpsc::Receiver<HeartbeatAction>,
) {
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let (action_tx, action_rx) = mpsc::channel(16);

    let heartbeat = HeartbeatLoop::new(client, identity, interval, state, shutdown_rx, action_tx);

    let handle = tokio::spawn(async move { heartbeat.run().await });

    (handle, shutdown_tx, action_rx)
}
