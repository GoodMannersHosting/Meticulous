//! Meticulous REST API server.
//!
//! The API server provides HTTP endpoints for managing pipelines, runs,
//! agents, and other resources, plus WebSocket streaming for logs.

use clap::Parser;
use met_api::{
    ApiDoc, ci_bootstrap,
    config::ApiConfig,
    routes,
    state::{AppState, ObjectStoragePublicConfig},
};
use met_core::MetConfig;
use met_core::redact::database_url_for_log;
use met_store::repos::AgentRepo;
use met_store::{PoolConfig, create_pool};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::signal;
use utoipa::OpenApi;

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

    /// Print the OpenAPI spec as JSON to stdout and exit
    #[arg(long)]
    dump_openapi: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse CLI args
    let args = Args::parse();

    if args.dump_openapi {
        let spec = ApiDoc::openapi();
        println!("{}", serde_json::to_string_pretty(&spec)?);
        return Ok(());
    }

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

    if let Ok(secret) = std::env::var("MET_JWT__SECRET")
        && !secret.is_empty()
    {
        api_config.jwt.secret = secret;
    }

    if std::env::var("MET_HTTP__ENABLE_HSTS")
        .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
        .unwrap_or(false)
    {
        api_config.enable_hsts = true;
    }

    if std::env::var("MET_CI_MODE")
        .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
        .unwrap_or(false)
    {
        api_config.ci_mode = true;
        // Force password auth on in CI mode regardless of other config.
        api_config.auth.password_enabled = true;
        api_config.ci_bootstrap_password = std::env::var("MET_CI_BOOTSTRAP_PASSWORD")
            .ok()
            .filter(|s| !s.is_empty());
    }

    // Create database pool
    let mut pool_config = PoolConfig::from(&met_config.database);
    if let Some(url) = args.database_url {
        pool_config.url = url;
    }

    tracing::info!(url = %database_url_for_log(&pool_config.url), "connecting to database");
    let db = create_pool(&pool_config).await?;

    if std::env::var("MET_API__RUN_MIGRATIONS")
        .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
        .unwrap_or(false)
    {
        met_store::run_migrations(&db).await?;
    }

    // CI mode: bootstrap org/users/data before serving.
    if api_config.ci_mode {
        let pw = api_config.ci_bootstrap_password.as_deref();
        ci_bootstrap::run(&db, pw)
            .await
            .map_err(|e| format!("CI bootstrap failed: {e}"))?;
    }

    let stale_after = api_config.agent_stale_after_secs;
    let sweep_secs = api_config.agent_stale_sweep_interval_secs.max(5);
    let db_stale = db.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(sweep_secs));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            interval.tick().await;
            let repo = AgentRepo::new(&db_stale);
            match repo.mark_stale_offline(stale_after as i64).await {
                Ok(n) if n > 0 => {
                    tracing::info!(count = n, "marked stale agents offline");
                }
                Ok(_) => {}
                Err(e) => tracing::warn!(error = %e, "agent stale sweep failed"),
            }
        }
    });

    let stored_secret_crypto = match std::env::var("MET_BUILTIN_SECRETS_MASTER_KEY") {
        Err(_) => None,
        Ok(s) if s.trim().is_empty() => None,
        Ok(k) => {
            let trimmed = k.trim();
            let r = met_secrets::BuiltinStoredCrypto::from_master_key_b64(trimmed, None);
            if let Err(ref e) = r {
                tracing::warn!(
                    error = %e,
                    "MET_BUILTIN_SECRETS_MASTER_KEY is set but could not be loaded; stored secrets API disabled. Expect standard base64 encoding with at least 16 bytes after decode (e.g. openssl rand -base64 32)."
                );
            }
            r.ok().map(std::sync::Arc::new)
        }
    };

    if let Some(crypto) = stored_secret_crypto.as_ref() {
        match met_store::repos::ensure_initial_oidc_signing_key(&db, crypto).await {
            Ok(()) => {}
            Err(e) => tracing::warn!(
                error = %e,
                "OIDC workload signing key bootstrap failed; /.well-known/jwks.json may be empty until resolved"
            ),
        }
    }

    let nats_creds = met_config.nats.credentials_file.as_deref();

    let nats_ops =
        match met_controller::nats::NatsDispatcher::connect(&met_config.nats.url, nats_creds).await
        {
            Ok(n) => Some(Arc::new(n)),
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "NATS ops client not connected; admin JOBS_DLQ preview disabled"
                );
                None
            }
        };

    let object_storage = ObjectStoragePublicConfig {
        endpoint: met_config.storage.endpoint.clone(),
        bucket: met_config.storage.bucket.clone(),
        path_style: met_config.storage.path_style,
    };

    let object_store = match met_objstore::S3ObjectStore::new(met_config.storage.clone().into())
        .await
    {
        Ok(s) => Some(std::sync::Arc::new(s)),
        Err(e) => {
            tracing::warn!(
                error = %e,
                "object store client not initialized; Platform Health will show degraded object storage status"
            );
            None
        }
    };

    let workspace_snapshots_disabled = matches!(
        std::env::var("MET_WORKSPACE_SNAPSHOTS_DISABLED").as_deref(),
        Ok("1") | Ok("true") | Ok("yes")
    );
    let workspace_snapshots = met_engine::WorkspaceSnapshotConfig {
        enabled: object_store.is_some() && !workspace_snapshots_disabled,
        object_ttl_hours: std::env::var("MET_WORKSPACE_SNAPSHOT_TTL_HOURS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(24)
            .clamp(1, 168),
        ..Default::default()
    };
    let workspace_snapshot_presigner = object_store.clone().map(|s| {
        std::sync::Arc::new(met_api::workspace_presigner::S3WorkspaceSnapshotPresigner::new(s))
            as std::sync::Arc<dyn met_engine::WorkspaceSnapshotPresigner>
    });

    let (engine, engine_init_error) = match met_engine::Engine::new(met_engine::EngineConfig {
        nats_url: met_config.nats.url.clone(),
        nats_credentials_file: met_config.nats.credentials_file.clone(),
        pool: db.clone(),
        executor: Default::default(),
        scheduler: Default::default(),
        cache_prefix: String::new(),
        builtin_secrets_master_key: std::env::var("MET_BUILTIN_SECRETS_MASTER_KEY").ok(),
        builtin_secrets_key_id: None,
        workspace_snapshots,
        workspace_snapshot_presigner,
    })
    .await
    {
        Ok(e) => (Some(Arc::new(e)), None),
        Err(e) => {
            tracing::warn!(
                error = %e,
                "met-engine failed to initialize (is NATS up?); pipeline trigger returns 503 until fixed"
            );
            (None, Some(e.to_string()))
        }
    };

    // Build application state
    let state = AppState::new(
        db,
        api_config.clone(),
        stored_secret_crypto,
        engine,
        engine_init_error,
        api_config.max_concurrent_engine_runs,
        nats_ops,
        object_storage,
        object_store,
    );

    // Spawn background tasks
    met_api::tasks::workflow_sync_task::spawn(state.clone());
    met_api::tasks::data_retention_task::spawn(state.clone());

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
    axum::serve(
        listener,
        router.into_make_service_with_connect_info::<SocketAddr>(),
    )
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
