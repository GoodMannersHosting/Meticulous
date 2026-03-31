//! Controller configuration.

use std::path::PathBuf;
use std::time::Duration;

/// Configuration for the agent controller.
#[derive(Debug, Clone)]
pub struct ControllerConfig {
    /// gRPC listen address.
    pub grpc_addr: String,
    /// NATS server URL.
    pub nats_url: String,
    /// Path to a NATS `.creds` file for the controller (required when the server disables anonymous access).
    pub nats_creds_path: Option<PathBuf>,
    /// Account signing seed (`SU...`) used to sign per-agent NATS user JWTs. Loaded from env only; never committed.
    pub nats_account_signing_seed: Option<String>,
    /// Account identity public key (`A...`) when the signing seed is a delegated key; sets `issuer_account` on user JWTs.
    pub nats_account_issuer_pubkey: Option<String>,
    /// Lifetime for issued NATS user JWTs.
    pub nats_agent_jwt_ttl: Duration,
    /// JWT secret for signing agent tokens.
    pub jwt_secret: String,
    /// JWT token validity duration.
    pub jwt_validity: Duration,
    /// Whether JWT tokens are renewable.
    pub jwt_renewable: bool,
    /// Expected heartbeat interval from agents.
    pub heartbeat_interval: Duration,
    /// Time after which an agent is considered stale (offline).
    pub stale_threshold: Duration,
    /// Time after which an agent is considered dead.
    pub dead_threshold: Duration,
    /// How often to run the health monitor.
    pub health_check_interval: Duration,
    /// Whether to require NTP synchronization on agents.
    pub require_ntp_sync: bool,
    /// Allowed OS/arch combinations (empty = all allowed).
    pub allowed_platforms: Vec<String>,
}

impl Default for ControllerConfig {
    fn default() -> Self {
        Self {
            grpc_addr: "0.0.0.0:9090".to_string(),
            nats_url: "nats://localhost:4222".to_string(),
            nats_creds_path: None,
            nats_account_signing_seed: None,
            nats_account_issuer_pubkey: None,
            nats_agent_jwt_ttl: Duration::from_secs(90 * 24 * 60 * 60),
            jwt_secret: "change-me-in-production".to_string(),
            jwt_validity: Duration::from_secs(24 * 60 * 60), // 24 hours
            jwt_renewable: true,
            heartbeat_interval: Duration::from_secs(15),
            stale_threshold: Duration::from_secs(45),   // 3x heartbeat
            dead_threshold: Duration::from_secs(120),   // 8x heartbeat
            health_check_interval: Duration::from_secs(10),
            require_ntp_sync: true,
            allowed_platforms: Vec::new(),
        }
    }
}

impl ControllerConfig {
    /// Create a new configuration with the given JWT secret.
    pub fn with_jwt_secret(secret: impl Into<String>) -> Self {
        Self {
            jwt_secret: secret.into(),
            ..Default::default()
        }
    }

    /// Set the gRPC listen address.
    pub fn grpc_addr(mut self, addr: impl Into<String>) -> Self {
        self.grpc_addr = addr.into();
        self
    }

    /// Set the NATS URL.
    pub fn nats_url(mut self, url: impl Into<String>) -> Self {
        self.nats_url = url.into();
        self
    }

    /// Set the heartbeat interval.
    pub fn heartbeat_interval(mut self, interval: Duration) -> Self {
        self.heartbeat_interval = interval;
        self
    }

    /// Validate the configuration.
    pub fn validate(&self) -> Result<(), String> {
        if self.jwt_secret.len() < 32 {
            return Err("JWT secret must be at least 32 characters".to_string());
        }
        if self.heartbeat_interval.is_zero() {
            return Err("heartbeat_interval must be > 0".to_string());
        }
        if self.stale_threshold <= self.heartbeat_interval {
            return Err("stale_threshold must be > heartbeat_interval".to_string());
        }
        if self.dead_threshold <= self.stale_threshold {
            return Err("dead_threshold must be > stale_threshold".to_string());
        }
        Ok(())
    }
}
