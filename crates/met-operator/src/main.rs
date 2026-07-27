//! Meticulous Kubernetes operator binary.
//!
//! This operator manages agent pools on Kubernetes, creating and destroying
//! agent pods based on AgentPool custom resources.

use kube::Client;
use met_operator::reconciler::AgentPoolReconciler;
use rustls::crypto::CryptoProvider;
use tracing::{error, info};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Install rustls crypto provider before creating any TLS clients (kube needs this)
    if CryptoProvider::get_default().is_none() {
        let ring = rustls::crypto::ring::default_provider();
        CryptoProvider::install_default(ring).expect("failed to install default rustls crypto provider");
    }

    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_target(true)
        .init();

    info!(
        version = env!("CARGO_PKG_VERSION"),
        "starting meticulous operator"
    );

    // Create Kubernetes client
    let client = Client::try_default().await?;

    // Optionally connect to NATS for queue metrics
    let nats = match std::env::var("NATS_URL") {
        Ok(url) => {
            info!(url = %url, "connecting to NATS for metrics");
            match async_nats::connect(&url).await {
                Ok(c) => Some(c),
                Err(e) => {
                    error!(error = %e, "failed to connect to NATS");
                    None
                }
            }
        }
        Err(_) => None,
    };

    // Run the reconciler
    AgentPoolReconciler::run(client, nats).await?;

    Ok(())
}
