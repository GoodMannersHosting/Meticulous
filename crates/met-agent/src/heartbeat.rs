//! Heartbeat loop for agent liveness.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use met_proto::AgentStatus;
use met_proto::agent::v1::{
    AgentStatusInfo, HeartbeatAction, HeartbeatRequest, ResourceSnapshot,
    agent_service_client::AgentServiceClient,
};
use sysinfo::System;
use tokio::sync::{RwLock, mpsc, watch};
use tonic::Code;
use tonic::transport::Channel;
use tracing::{debug, error, info};

use crate::config::AgentIdentity;
use crate::error::Result;

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
    /// Path to persisted identity; removed if controller returns NotFound (stale id).
    identity_path: PathBuf,
    interval: Duration,
    state: Arc<RwLock<HeartbeatState>>,
    shutdown_rx: watch::Receiver<bool>,
    /// When set to true, the job executor stops pulling from NATS (drain).
    job_pause_tx: watch::Sender<bool>,
    action_tx: mpsc::Sender<HeartbeatAction>,
    /// One message per busy/idle transition; each recv triggers a heartbeat so rapid job cycles are not
    /// collapsed into a single update (unlike `Notify`, which can drop back-to-back wakes).
    transition_wake: mpsc::UnboundedReceiver<()>,
}

impl HeartbeatLoop {
    /// Create a new heartbeat loop.
    pub fn new(
        client: AgentServiceClient<Channel>,
        identity: AgentIdentity,
        identity_path: PathBuf,
        interval: Duration,
        state: Arc<RwLock<HeartbeatState>>,
        shutdown_rx: watch::Receiver<bool>,
        job_pause_tx: watch::Sender<bool>,
        action_tx: mpsc::Sender<HeartbeatAction>,
        transition_wake: mpsc::UnboundedReceiver<()>,
    ) -> Self {
        Self {
            client,
            identity,
            identity_path,
            interval,
            state,
            shutdown_rx,
            job_pause_tx,
            action_tx,
            transition_wake,
        }
    }

    async fn pump_heartbeat(&mut self, system: &mut System) -> Result<bool> {
        if *self.shutdown_rx.borrow() {
            return Ok(true);
        }
        let mut shutdown_wake = self.shutdown_rx.clone();
        tokio::select! {
            _ = shutdown_wake.changed() => {
                if *shutdown_wake.borrow() {
                    info!("heartbeat loop shutting down");
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            res = self.send_heartbeat(system) => {
                match res {
                    Ok(action) => {
                        if action != HeartbeatAction::Continue {
                            info!(action = ?action, "received heartbeat action");
                            if self.action_tx.send(action).await.is_err() {
                                return Ok(true);
                            }
                        }
                        Ok(false)
                    }
                    Err(e) => {
                        error!(error = %e, "heartbeat failed");
                        Ok(false)
                    }
                }
            }
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
                break 'hb;
            }

            tokio::select! {
                _ = self.shutdown_rx.changed() => {
                    if *self.shutdown_rx.borrow() {
                        info!("heartbeat loop shutting down");
                        break 'hb;
                    }
                }
                _ = interval.tick() => {
                    if self.pump_heartbeat(&mut system).await? {
                        break 'hb;
                    }
                }
                msg = self.transition_wake.recv() => {
                    match msg {
                        Some(()) => {
                            if self.pump_heartbeat(&mut system).await? {
                                break 'hb;
                            }
                        }
                        None => {
                            break 'hb;
                        }
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

        let response = match self.client.heartbeat(request).await {
            Ok(r) => r.into_inner(),
            Err(status) if status.code() == Code::NotFound => {
                error!(
                    agent_id = %self.identity.agent_id,
                    identity_path = %self.identity_path.display(),
                    "controller heartbeat: agent not found (no row for this id — DB reset, different controller, or deleted agent)"
                );
                if self.identity_path.exists() {
                    match std::fs::remove_file(&self.identity_path) {
                        Ok(()) => info!(
                            path = %self.identity_path.display(),
                            "removed stale agent identity file so the next start can re-register"
                        ),
                        Err(e) => error!(
                            error = %e,
                            path = %self.identity_path.display(),
                            "could not remove stale identity file"
                        ),
                    }
                }
                eprintln!(
                    "\nmeticulous-agent: this agent id is not registered with the controller.\n\
                     If you intended to enroll with a join token, run again with MET_JOIN_TOKEN set and \
                     MET_FORCE_REGISTER=1 (or delete the identity file at {}).\n",
                    self.identity_path.display()
                );
                std::process::exit(1);
            }
            Err(e) => return Err(e.into()),
        };

        // Handle JWT renewal if provided
        if let Some(new_jwt) = response.new_jwt_token {
            debug!("received renewed JWT token");
            // Would update the identity here
        }

        let action =
            HeartbeatAction::try_from(response.action).unwrap_or(HeartbeatAction::Continue);

        if action == HeartbeatAction::Drain {
            let mut s = self.state.write().await;
            s.status = AgentStatus::Draining;
            let _ = self.job_pause_tx.send(true);
        }

        if action == HeartbeatAction::Resume {
            let mut s = self.state.write().await;
            s.status = if s.running_jobs > 0 {
                AgentStatus::Busy
            } else {
                AgentStatus::Online
            };
            let _ = self.job_pause_tx.send(false);
        }

        Ok(action)
    }
}

/// Spawn the heartbeat loop in a background task.
///
/// Returns an [`mpsc::UnboundedSender`] — send `()` after updating busy/idle heartbeat state so the
/// controller (and Agents UI) see transitions without waiting for the next interval.
pub fn spawn_heartbeat_loop(
    client: AgentServiceClient<Channel>,
    identity: AgentIdentity,
    identity_path: PathBuf,
    interval: Duration,
    state: Arc<RwLock<HeartbeatState>>,
    job_pause_tx: watch::Sender<bool>,
) -> (
    tokio::task::JoinHandle<Result<()>>,
    watch::Sender<bool>,
    mpsc::Receiver<HeartbeatAction>,
    mpsc::UnboundedSender<()>,
) {
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let (action_tx, action_rx) = mpsc::channel(16);
    let (transition_wake_tx, transition_wake_rx) = mpsc::unbounded_channel();

    let heartbeat = HeartbeatLoop::new(
        client,
        identity,
        identity_path,
        interval,
        state,
        shutdown_rx,
        job_pause_tx,
        action_tx,
        transition_wake_rx,
    );

    let handle = tokio::spawn(async move { heartbeat.run().await });

    (handle, shutdown_tx, action_rx, transition_wake_tx)
}
