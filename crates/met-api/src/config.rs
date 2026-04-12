//! API-specific configuration.
//!
//! This module extends the base `MetConfig` with API-specific settings
//! like JWT secrets, rate limit parameters, and auth configuration.

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// API server configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ApiConfig {
    /// HTTP listen address.
    pub listen_addr: String,

    /// Allowed CORS origins.
    pub cors_origins: Vec<String>,

    /// Allow any origin for CORS (development only).
    pub cors_allow_any: bool,

    /// Request body size limit in bytes.
    pub body_limit_bytes: usize,

    /// Request timeout.
    #[serde(with = "humantime_serde")]
    pub request_timeout: Duration,

    /// JWT configuration.
    pub jwt: JwtConfig,

    /// Rate limiting configuration.
    pub rate_limit: RateLimitConfig,

    /// Auth configuration.
    pub auth: AuthConfig,

    /// Mark agents offline if last heartbeat is older than this (matches [`met_core::config::HttpConfig::agent_stale_after_secs`]).
    pub agent_stale_after_secs: u64,
    /// Interval for the stale-agent sweep background task.
    pub agent_stale_sweep_interval_secs: u64,

    /// Maximum pipeline runs executing in-process inside `met-api` at once.
    pub max_concurrent_engine_runs: usize,

    /// Default list page size when the client omits `limit` / `per_page`.
    pub pagination_default_limit: u32,
    /// Maximum list page size (client requests are clamped to this).
    pub pagination_max_limit: u32,

    /// Public base URL for OIDC issuer / discovery (ADR-017). Overrides [`met_core::config::HttpConfig::public_base_url`] when set from layered config.
    #[serde(default)]
    pub public_base_url: Option<String>,

    /// Emit `Strict-Transport-Security` on responses (enable behind HTTPS / TLS-terminating proxies).
    #[serde(default)]
    pub enable_hsts: bool,

    /// CI mode: bootstrap a known admin + service-account on startup and seed fake data.
    /// Enabled by `MET_CI_MODE=true`. Never enable in production.
    #[serde(default)]
    pub ci_mode: bool,

    /// Password for the CI bootstrap admin user (default: `ci-bootstrap`).
    /// Read from `MET_CI_BOOTSTRAP_PASSWORD`. Only used when `ci_mode` is true.
    #[serde(default)]
    pub ci_bootstrap_password: Option<String>,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            listen_addr: "0.0.0.0:8080".to_string(),
            cors_origins: vec!["http://localhost:5173".to_string()],
            cors_allow_any: false,
            body_limit_bytes: 10 * 1024 * 1024,
            request_timeout: Duration::from_secs(30),
            jwt: JwtConfig::default(),
            rate_limit: RateLimitConfig::default(),
            auth: AuthConfig::default(),
            agent_stale_after_secs: 90,
            agent_stale_sweep_interval_secs: 30,
            max_concurrent_engine_runs: 8,
            pagination_default_limit: 10_000,
            pagination_max_limit: 10_000,
            public_base_url: None,
            enable_hsts: false,
            ci_mode: false,
            ci_bootstrap_password: None,
        }
    }
}

impl From<&met_core::config::HttpConfig> for ApiConfig {
    fn from(http: &met_core::config::HttpConfig) -> Self {
        Self {
            listen_addr: http.listen_addr.clone(),
            cors_origins: http.cors_origins.clone(),
            body_limit_bytes: http.body_limit_bytes,
            request_timeout: Duration::from_secs(http.request_timeout_secs),
            agent_stale_after_secs: http.agent_stale_after_secs,
            agent_stale_sweep_interval_secs: http.agent_stale_sweep_interval_secs,
            auth: AuthConfig::default(),
            max_concurrent_engine_runs: 8,
            pagination_default_limit: http.pagination_default_limit,
            pagination_max_limit: http.pagination_max_limit,
            public_base_url: http.public_base_url.clone(),
            enable_hsts: false,
            ci_mode: false,
            ci_bootstrap_password: None,
            ..Default::default()
        }
    }
}

/// JWT authentication configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct JwtConfig {
    /// Secret key for HS256 signing (should be 32+ bytes).
    /// In production, use RS256 with public key validation.
    pub secret: String,

    /// JWT issuer claim.
    pub issuer: String,

    /// JWT audience claim.
    pub audience: String,

    /// Token expiration time.
    #[serde(with = "humantime_serde")]
    pub expiration: Duration,

    /// Max lifetime (`exp` − `iat`) allowed for Meticulous App JWTs (integration auth).
    pub app_max_ttl_secs: u64,

    /// Clock skew leeway (seconds) when validating App JWT `exp` / `nbf`.
    pub app_leeway_secs: u64,
}

impl Default for JwtConfig {
    fn default() -> Self {
        Self {
            secret: "development-secret-change-in-production".to_string(),
            issuer: "meticulous".to_string(),
            audience: "meticulous-api".to_string(),
            expiration: Duration::from_secs(3600),
            app_max_ttl_secs: 600,
            app_leeway_secs: 60,
        }
    }
}

/// Rate limiting configuration using token bucket algorithm.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RateLimitConfig {
    /// Enable rate limiting.
    pub enabled: bool,

    /// Requests per second per client.
    pub requests_per_second: u32,

    /// Burst capacity (max tokens).
    pub burst_size: u32,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            requests_per_second: 100,
            burst_size: 200,
        }
    }
}

/// Authentication configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AuthConfig {
    /// Enable password-based authentication.
    /// When false, users must use SSO/OIDC providers.
    pub password_enabled: bool,

    /// Require email verification for new accounts.
    pub require_email_verification: bool,

    /// Minimum password length.
    pub min_password_length: usize,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            password_enabled: true,
            require_email_verification: false,
            min_password_length: 8,
        }
    }
}

mod humantime_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        duration.as_secs().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let secs = u64::deserialize(deserializer)?;
        Ok(Duration::from_secs(secs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = ApiConfig::default();
        assert_eq!(config.listen_addr, "0.0.0.0:8080");
        assert_eq!(config.body_limit_bytes, 10 * 1024 * 1024);
        assert_eq!(config.pagination_default_limit, 10_000);
        assert_eq!(config.pagination_max_limit, 10_000);
        assert!(config.rate_limit.enabled);
    }
}
