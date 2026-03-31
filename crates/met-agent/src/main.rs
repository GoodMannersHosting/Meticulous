//! Meticulous build agent binary.
//!
//! The agent connects to the controller, receives job assignments,
//! and executes steps in isolated environments.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use clap::Parser;
use met_agent::backend;
use met_agent::config::AgentConfig;
use met_agent::executor::JobExecutor;
use met_agent::heartbeat::{spawn_heartbeat_loop, HeartbeatState};
use met_agent::registration::AgentRegistration;
use met_proto::agent::v1::HeartbeatAction;
use tokio::signal;
use tokio::sync::{watch, RwLock};
use tracing::{error, info, warn};

#[derive(Parser)]
#[command(name = "met-agent")]
#[command(about = "Meticulous build agent")]
#[command(
    long_about = "Without --agent-config, searches (first hit wins): ./meticulous-agent.toml, ~/.met/agentconfig*, XDG agent.toml, /opt/met-agent/agentconfig*, /etc/meticulous/agent.toml. MET_CONFIG env is a deprecated alias for the config path."
)]
#[command(version)]
struct Args {
    /// Controller address
    #[arg(
        long,
        env = "MET_CONTROLLER_URL",
        default_value = "http://localhost:9090"
    )]
    controller_url: String,

    /// Join token for registration
    #[arg(long, env = "MET_JOIN_TOKEN")]
    join_token: Option<String>,

    /// Agent name
    #[arg(long, env = "MET_AGENT_NAME")]
    name: Option<String>,

    /// Agent pool
    #[arg(long, env = "MET_AGENT_POOL")]
    pool: Option<String>,

    /// Pool tags (comma-separated)
    #[arg(long, env = "MET_AGENT_TAGS", value_delimiter = ',')]
    tags: Vec<String>,

    /// Agent configuration file (TOML or YAML). If unset, searches defaults (see help).
    #[arg(
        short = 'c',
        long = "agent-config",
        visible_alias = "config",
        env = "MET_AGENT_CONFIG",
        value_name = "PATH"
    )]
    agent_config: Option<PathBuf>,

    /// Log level
    #[arg(long, env = "MET_LOG_LEVEL", default_value = "info")]
    log_level: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let agent_config_path = args
        .agent_config
        .or_else(|| std::env::var_os("MET_CONFIG").map(PathBuf::from));

    // Initialize logging
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&args.log_level));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .init();

    info!(
        version = env!("CARGO_PKG_VERSION"),
        controller = %args.controller_url,
        "starting meticulous agent"
    );

    // Load configuration
    let config = AgentConfig::load(
        agent_config_path.as_deref(),
        Some(args.controller_url.clone()),
        args.join_token.clone(),
        args.name.clone(),
        args.pool.clone(),
        args.tags.clone(),
    )?;

    // Create shutdown channel
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    // Register with controller
    let mut registration = AgentRegistration::new(config.clone()).await?;
    let identity = registration.register_or_load().await?;

    info!(
        agent_id = %identity.agent_id,
        nats_subjects = ?identity.nats_subjects,
        "agent registered"
    );

    // Create execution backend
    let backend: Arc<dyn backend::ExecutionBackend> = Arc::from(backend::default_backend().await);
    info!(backend = backend.name(), "using execution backend");

    // Create heartbeat state
    let heartbeat_state = Arc::new(RwLock::new(HeartbeatState::default()));

    // Start heartbeat loop
    let client = registration.client().clone();
    let (heartbeat_handle, heartbeat_shutdown, mut action_rx) = spawn_heartbeat_loop(
        client.clone(),
        identity.clone(),
        Duration::from_secs(15),
        heartbeat_state.clone(),
    );

    // Connect to NATS
    let nats_url = identity.nats_url.clone();
    let nats_client = async_nats::connect(&nats_url).await?;
    info!(url = %nats_url, "connected to NATS");

    // Create job executor
    let executor = JobExecutor::new(
        config.clone(),
        identity.clone(),
        client,
        backend,
        heartbeat_state.clone(),
        shutdown_rx.clone(),
    );

    // Start executor in background
    let executor_handle = tokio::spawn(async move {
        if let Err(e) = executor.run(nats_client).await {
            error!(error = %e, "executor error");
        }
    });

    // Wait for shutdown signal or heartbeat action
    tokio::select! {
        _ = signal::ctrl_c() => {
            info!("received SIGINT, shutting down");
        }
        action = action_rx.recv() => {
            match action {
                Some(HeartbeatAction::Drain) => {
                    info!("received DRAIN command, finishing current jobs");
                }
                Some(HeartbeatAction::Terminate) => {
                    warn!("received TERMINATE command, shutting down immediately");
                }
                _ => {}
            }
        }
    }

    // Signal shutdown (executor + heartbeat respond and exit their loops).
    let _ = shutdown_tx.send(true);
    let _ = heartbeat_shutdown.send(true);

    // Wait for tasks to finish. Do not wrap in `timeout` + drop: that cancels the await and
    // leaves work running until runtime teardown, which often surfaces as noisy errors/panics.
    match heartbeat_handle.await {
        Ok(Ok(())) => {}
        Ok(Err(e)) => warn!(error = %e, "heartbeat loop exited with error"),
        Err(e) => warn!(error = %e, "heartbeat task panicked or was cancelled"),
    }
    match executor_handle.await {
        Ok(()) => {}
        Err(e) => warn!(error = %e, "executor task panicked or was cancelled"),
    }

    info!("agent shutdown complete");
    Ok(())
}
