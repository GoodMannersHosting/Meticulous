//! Agent controller process: gRPC (`AgentService`) for registration and heartbeats.

use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use met_controller::config::ControllerConfig;
use met_controller::grpc::AgentServiceImpl;
use met_controller::nats::NatsDispatcher;
use met_controller::registry::AgentRegistry;
use met_core::redact::database_url_for_log;
use met_core::MetConfig;
use met_proto::agent::v1::agent_service_server::AgentServiceServer;
use met_store::{PoolConfig, create_pool};
use tokio::signal;
use tonic::transport::Server;
use tracing::info;

#[derive(Parser)]
#[command(name = "met-controller")]
#[command(about = "Meticulous agent controller (gRPC)")]
struct Args {
    /// PostgreSQL URL (overrides config file / `MET_DATABASE__URL`)
    #[arg(long, env = "MET_DATABASE_URL")]
    database_url: Option<String>,

    /// gRPC listen address (overrides `MET_CONTROLLER_GRPC_ADDR` then config `grpc.listen_addr`)
    #[arg(long, env = "MET_CONTROLLER_GRPC_ADDR")]
    grpc_addr: Option<String>,

    /// NATS URL (overrides `MET_NATS_URL` then config `nats.url`)
    #[arg(long, env = "MET_NATS_URL")]
    nats_url: Option<String>,

    /// JWT signing secret for agent tokens (min 32 characters). Not stored in files.
    #[arg(long, env = "MET_CONTROLLER_JWT_SECRET")]
    jwt_secret: Option<String>,

    /// Require agents to report NTP synchronized (default: false for local dev)
    #[arg(long, env = "MET_CONTROLLER_REQUIRE_NTP_SYNC")]
    require_ntp_sync: Option<bool>,

    /// Log level filter (default: info)
    #[arg(long, env = "MET_LOG_LEVEL", default_value = "info")]
    log_level: String,

    /// Path to NATS `.creds` for the controller when the server requires JWT auth.
    #[arg(long, env = "MET_NATS_CREDS_PATH")]
    nats_creds_path: Option<PathBuf>,

    /// NATS account signing seed (`SU...`) for per-agent user JWTs (optional; omit for anonymous NATS dev).
    #[arg(long, env = "MET_NATS_ACCOUNT_SIGNING_SEED")]
    nats_account_signing_seed: Option<String>,

    /// Account identity public key when using a delegated signing key (`MET_NATS_ACCOUNT_SIGNING_SEED`).
    #[arg(long, env = "MET_NATS_ACCOUNT_ISSUER_PUBKEY")]
    nats_account_issuer_pubkey: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| format!("{},met_controller=info", args.log_level).into()),
        )
        .init();

    let met_config = MetConfig::load()?;

    let mut ctrl = ControllerConfig::default();
    ctrl.grpc_addr = args
        .grpc_addr
        .or_else(|| std::env::var("MET_CONTROLLER_GRPC_ADDR").ok())
        .unwrap_or_else(|| met_config.grpc.listen_addr.clone());

    ctrl.nats_url = args
        .nats_url
        .or_else(|| std::env::var("MET_NATS_URL").ok())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| met_config.nats.url.clone());

    let jwt_secret = args
        .jwt_secret
        .or_else(|| std::env::var("MET_CONTROLLER_JWT_SECRET").ok())
        .ok_or_else(|| {
            "MET_CONTROLLER_JWT_SECRET must be set (32+ characters). \
             Generate a random secret; do not commit it to source control."
        })?;
    if jwt_secret.len() < 32 {
        return Err("MET_CONTROLLER_JWT_SECRET must be at least 32 characters".into());
    }
    ctrl.jwt_secret = jwt_secret;

    ctrl.require_ntp_sync = args
        .require_ntp_sync
        .or_else(|| {
            std::env::var("MET_CONTROLLER_REQUIRE_NTP_SYNC")
                .ok()
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        })
        .unwrap_or(false);

    ctrl.nats_creds_path = args
        .nats_creds_path
        .or_else(|| std::env::var("MET_NATS_CREDS_PATH").ok().map(PathBuf::from))
        .filter(|p| !p.as_os_str().is_empty());
    ctrl.nats_account_signing_seed = args
        .nats_account_signing_seed
        .or_else(|| std::env::var("MET_NATS_ACCOUNT_SIGNING_SEED").ok())
        .filter(|s| !s.trim().is_empty());
    ctrl.nats_account_issuer_pubkey = args
        .nats_account_issuer_pubkey
        .or_else(|| std::env::var("MET_NATS_ACCOUNT_ISSUER_PUBKEY").ok())
        .filter(|s| !s.trim().is_empty());

    ctrl.validate()
        .map_err(|e| format!("invalid controller config: {e}"))?;

    let mut pool_config = PoolConfig::from(&met_config.database);
    if let Some(url) = args.database_url {
        pool_config.url = url;
    }

    info!(url = %database_url_for_log(&pool_config.url), "connecting to database");
    let pool = Arc::new(create_pool(&pool_config).await?);

    info!(url = %ctrl.nats_url, "connecting to NATS");
    let nats = NatsDispatcher::connect(&ctrl.nats_url, ctrl.nats_creds_path.as_deref()).await?;
    nats.spawn_max_deliveries_dlq_forwarder();

    let stored_secret_crypto = std::env::var("MET_BUILTIN_SECRETS_MASTER_KEY")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .and_then(|k| met_secrets::BuiltinStoredCrypto::from_master_key_b64(&k, None).ok())
        .map(std::sync::Arc::new);

    let registry = AgentRegistry::new();
    let grpc = AgentServiceImpl::new(
        ctrl.clone(),
        pool,
        registry,
        nats,
        None,
        stored_secret_crypto,
    );

    let addr = ctrl
        .grpc_addr
        .parse::<std::net::SocketAddr>()
        .map_err(|e| format!("invalid gRPC listen address '{}': {e}", ctrl.grpc_addr))?;

    info!(
        grpc_addr = %addr,
        version = env!("CARGO_PKG_VERSION"),
        "starting met-controller gRPC server"
    );

    Server::builder()
        .add_service(AgentServiceServer::new(grpc))
        .serve_with_shutdown(addr, shutdown_signal())
        .await?;

    info!("met-controller shutdown complete");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {
            info!("received Ctrl+C, shutting down");
        }
        () = terminate => {
            info!("received SIGTERM, shutting down");
        }
    }
}
