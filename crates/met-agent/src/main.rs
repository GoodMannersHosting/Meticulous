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
use met_agent::error::AgentError;
use met_agent::registration::{AgentRegistration, RegistrationSource};
use met_proto::agent::v1::HeartbeatAction;
use nkeys::KeyPair;
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

    /// Ignore cached identity and register again (requires a valid `MET_JOIN_TOKEN`).
    #[arg(long, env = "MET_FORCE_REGISTER")]
    force_register: bool,
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

    // Create shutdown channel (full process exit) and job-pause (drain: stop NATS pulls).
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let (job_pause_tx, job_pause_rx) = watch::channel(false);

    // Register with controller
    let mut registration = AgentRegistration::new(config.clone()).await?;
    let force_register = args.force_register
        || std::env::var("MET_FORCE_REGISTER")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

    let (identity, registration_source) = match registration.register_or_load(force_register).await {
        Ok(pair) => pair,
        Err(e) if registration_failure_should_exit(&e) => {
            error!(error = %e, "registration failed; invalid join token or enrollment rejected");
            std::process::exit(1);
        }
        Err(e) => return Err(e.into()),
    };

    match registration_source {
        RegistrationSource::LoadedFromDisk => {
            info!(
                agent_id = %identity.agent_id,
                nats_subjects = ?identity.nats_subjects,
                "using persisted agent identity (skipped registration; set MET_FORCE_REGISTER=1 to re-enroll with a join token)"
            );
        }
        RegistrationSource::RegisteredWithController => {
            info!(
                agent_id = %identity.agent_id,
                nats_subjects = ?identity.nats_subjects,
                "registered with controller and saved identity"
            );
        }
    }

    // Create execution backend
    let backend: Arc<dyn backend::ExecutionBackend> = Arc::from(backend::default_backend().await);
    info!(backend = backend.name(), "using execution backend");

    // Create heartbeat state
    let heartbeat_state = Arc::new(RwLock::new(HeartbeatState::default()));

    // Start heartbeat loop
    let client = registration.client().clone();
    let identity_path = config.identity_path();
    let (heartbeat_handle, heartbeat_shutdown, mut action_rx) = spawn_heartbeat_loop(
        client.clone(),
        identity.clone(),
        identity_path,
        Duration::from_secs(15),
        heartbeat_state.clone(),
        job_pause_tx,
    );

    // Connect to NATS
    let require_nats_jwt = std::env::var("MET_AGENT_REQUIRE_NATS_JWT")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let nats_url = identity.nats_url.clone();
    let nats_client = match connect_nats(&identity).await {
        Ok(c) => c,
        Err(e) if require_nats_jwt => {
            error!(
                error = %e,
                url = %nats_url,
                "NATS connection failed with MET_AGENT_REQUIRE_NATS_JWT enabled"
            );
            std::process::exit(1);
        }
        Err(e) => return Err(e.into()),
    };
    info!(url = %nats_url, "connected to NATS");

    // Create job executor
    let executor = JobExecutor::new(
        config.clone(),
        identity.clone(),
        client,
        backend,
        heartbeat_state.clone(),
        shutdown_rx.clone(),
        job_pause_rx,
    );

    // Start executor in background
    let executor_handle = tokio::spawn(async move {
        if let Err(e) = executor.run(nats_client).await {
            error!(error = %e, "executor error");
        }
    });

    // Wait for shutdown; DRAIN only pauses NATS pulls (handled in heartbeat + executor), not process exit.
    loop {
        tokio::select! {
            _ = signal::ctrl_c() => {
                info!("received SIGINT, shutting down");
                break;
            }
            action = action_rx.recv() => {
                match action {
                    Some(HeartbeatAction::Drain) => {
                        info!("received DRAIN command, no longer accepting new jobs from NATS");
                    }
                    Some(HeartbeatAction::Terminate) => {
                        warn!("received TERMINATE command, shutting down immediately");
                        break;
                    }
                    Some(_) => {}
                    None => break,
                }
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

fn registration_failure_should_exit(err: &AgentError) -> bool {
    match err {
        AgentError::Registration(_) => true,
        AgentError::Config(msg) if msg.contains("join_token") => true,
        _ => false,
    }
}

async fn connect_nats(
    identity: &met_agent::config::AgentIdentity,
) -> Result<async_nats::Client, AgentError> {
    let url = identity.nats_url.as_str();
    match (&identity.nats_user_jwt, &identity.nats_user_seed) {
        (Some(jwt), Some(seed))
            if !jwt.trim().is_empty() && !seed.trim().is_empty() =>
        {
            let kp = std::sync::Arc::new(
                KeyPair::from_seed(seed.trim())
                    .map_err(|e| AgentError::Config(format!("invalid NATS user seed in identity: {e}")))?,
            );
            let jwt = jwt.clone();
            async_nats::ConnectOptions::with_jwt(jwt, move |nonce| {
                let kp = kp.clone();
                async move { kp.sign(&nonce).map_err(async_nats::AuthError::new) }
            })
            .connect(url)
            .await
            .map_err(|e| AgentError::Internal(format!("NATS connect: {e}")))
        }
        _ => async_nats::connect(url)
            .await
            .map_err(|e| AgentError::Internal(format!("NATS connect: {e}"))),
    }
}
