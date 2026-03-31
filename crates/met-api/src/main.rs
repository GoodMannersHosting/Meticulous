//! Meticulous REST API server.
//!
//! The API server provides HTTP endpoints for managing pipelines, runs,
//! agents, and other resources, plus WebSocket streaming for logs.

use clap::Parser;
use met_api::{config::ApiConfig, routes, state::AppState};
use met_core::MetConfig;
use met_store::{PoolConfig, create_pool};
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio::signal;

#[derive(Parser)]
#[command(name = "met-api")]
#[command(about = "Meticulous API server")]
struct Args {
    /// HTTP listen address (overrides config)
    #[arg(long, env = "MET_HTTP_ADDR")]
    listen_addr: Option<String>,

    /// Database URL (overrides config)
    #[arg(long, env = "MET_DATABASE_URL")]
    database_url: Option<String>,

    /// Config file path
    #[arg(long, env = "MET_CONFIG")]
    config: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse CLI args
    let args = Args::parse();

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,met_api=debug,tower_http=debug".into()),
        )
        .init();

    // Load configuration
    let met_config = MetConfig::load()?;
    let mut api_config = ApiConfig::from(&met_config.http);

    // Override with CLI args
    if let Some(addr) = args.listen_addr {
        api_config.listen_addr = addr;
    }

    // Allow CORS from any origin in development
    if std::env::var("MET_HTTP__CORS_ALLOW_ANY")
        .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
        .unwrap_or(false)
    {
        api_config.cors_allow_any = true;
        tracing::warn!("CORS allowed from any origin - DO NOT use in production");
    }

    // Create database pool
    let mut pool_config = PoolConfig::from(&met_config.database);
    if let Some(url) = args.database_url {
        pool_config.url = url;
    }

    tracing::info!(url = %pool_config.url, "connecting to database");
    let db = create_pool(&pool_config).await?;

    // Build application state
    let state = AppState::new(db, api_config.clone());

    // Build router
    let router = routes::build_router(state);

    // Parse listen address
    let addr: SocketAddr = api_config
        .listen_addr
        .parse()
        .map_err(|e| format!("invalid listen address '{}': {e}", api_config.listen_addr))?;

    // Create TCP listener
    let listener = TcpListener::bind(addr).await?;

    tracing::info!(
        listen = %addr,
        version = %met_api::VERSION,
        "starting meticulous api server"
    );

    // Start server with graceful shutdown
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    tracing::info!("server shutdown complete");
    Ok(())
}

/// Wait for shutdown signal (SIGTERM or SIGINT).
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
        _ = ctrl_c => {
            tracing::info!("received Ctrl+C, initiating graceful shutdown");
        }
        _ = terminate => {
            tracing::info!("received SIGTERM, initiating graceful shutdown");
        }
    }
}
