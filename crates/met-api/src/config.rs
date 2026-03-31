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
}

impl Default for JwtConfig {
    fn default() -> Self {
        Self {
            secret: "development-secret-change-in-production".to_string(),
            issuer: "meticulous".to_string(),
            audience: "meticulous-api".to_string(),
            expiration: Duration::from_secs(3600),
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
        assert!(config.rate_limit.enabled);
    }
}
