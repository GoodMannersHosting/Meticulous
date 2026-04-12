//! Configuration loading with layered overrides.
//!
//! Configuration is loaded from multiple sources in order (later overrides earlier):
//! 1. Compiled defaults
//! 2. `/etc/meticulous/config.toml` (system-wide)
//! 3. `~/.config/meticulous/config.toml` (user)
//! 4. `./meticulous.toml` (project-local)
//! 5. Environment variables prefixed `MET_` (e.g., `MET_DATABASE__URL`)

use crate::error::{MetError, Result};
use config::{Config, Environment, File, FileFormat};
use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Split comma-separated origins (common for `MET_HTTP__CORS_ORIGINS` in Kubernetes).
fn split_cors_origins_csv(s: &str) -> Vec<String> {
    s.split(',')
        .map(str::trim)
        .filter(|x| !x.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn deserialize_cors_origins<'de, D>(deserializer: D) -> std::result::Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    struct CorsVisitor;

    impl<'de> de::Visitor<'de> for CorsVisitor {
        type Value = Vec<String>;

        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("a list of origin strings or one comma-separated string")
        }

        fn visit_seq<A>(self, mut seq: A) -> std::result::Result<Self::Value, A::Error>
        where
            A: de::SeqAccess<'de>,
        {
            let mut v = Vec::new();
            while let Some(s) = seq.next_element::<String>()? {
                v.push(s);
            }
            Ok(v)
        }

        fn visit_str<E: de::Error>(self, s: &str) -> std::result::Result<Self::Value, E> {
            Ok(split_cors_origins_csv(s))
        }

        fn visit_string<E: de::Error>(self, s: String) -> std::result::Result<Self::Value, E> {
            Ok(split_cors_origins_csv(&s))
        }
    }

    deserializer.deserialize_any(CorsVisitor)
}

/// Top-level configuration for Meticulous components.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MetConfig {
    /// Database configuration.
    pub database: DatabaseConfig,
    /// NATS message broker configuration.
    pub nats: NatsConfig,
    /// gRPC server configuration.
    pub grpc: GrpcConfig,
    /// HTTP server configuration.
    pub http: HttpConfig,
    /// Object storage configuration.
    pub storage: StorageConfig,
    /// Logging configuration.
    pub log: LogConfig,
}

impl Default for MetConfig {
    fn default() -> Self {
        Self {
            database: DatabaseConfig::default(),
            nats: NatsConfig::default(),
            grpc: GrpcConfig::default(),
            http: HttpConfig::default(),
            storage: StorageConfig::default(),
            log: LogConfig::default(),
        }
    }
}

impl MetConfig {
    /// Load configuration from all sources with layered overrides.
    ///
    /// # Errors
    ///
    /// Returns an error if configuration loading or parsing fails.
    pub fn load() -> Result<Self> {
        Self::load_with_paths(&default_config_paths())
    }

    /// Load configuration from specified paths (for testing).
    ///
    /// # Errors
    ///
    /// Returns an error if configuration loading or parsing fails.
    pub fn load_with_paths(paths: &[PathBuf]) -> Result<Self> {
        let mut builder = Config::builder();

        // Add config files (optional - missing files are ignored)
        for path in paths {
            if path.exists() {
                tracing::debug!(?path, "loading configuration file");
                builder = builder.add_source(File::from(path.as_ref()).required(false));
            }
        }

        // Add environment variables (MET_ prefix, __ for nesting).
        // `MET_DATABASE__URL` lowercases to `met_database__url`. Without an explicit prefix
        // separator, config 0.15 defaults the prefix joiner to the same as `separator` ("__"),
        // producing a `met__` prefix pattern that never matches — and `database.url` falls back to
        // compiled defaults (localhost). Use `prefix_separator("_")` so the pattern is `met_`.
        builder = builder.add_source(
            Environment::with_prefix("MET")
                .prefix_separator("_")
                .separator("__")
                .try_parsing(true),
        );

        let config = builder
            .build()
            .map_err(|e| MetError::Config(e.to_string()))?;

        config
            .try_deserialize()
            .map_err(|e| MetError::Config(e.to_string()))
    }

    /// Load configuration from a TOML string (for testing).
    ///
    /// # Errors
    ///
    /// Returns an error if parsing fails.
    pub fn from_toml(toml: &str) -> Result<Self> {
        let config = Config::builder()
            .add_source(File::from_str(toml, FileFormat::Toml))
            .build()
            .map_err(|e| MetError::Config(e.to_string()))?;

        config
            .try_deserialize()
            .map_err(|e| MetError::Config(e.to_string()))
    }
}

/// Database connection configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DatabaseConfig {
    /// PostgreSQL connection URL.
    pub url: String,
    /// Maximum number of connections in the pool.
    pub max_connections: u32,
    /// Minimum number of connections to keep open.
    pub min_connections: u32,
    /// Connection timeout in seconds.
    pub connect_timeout_secs: u64,
    /// Enable statement-level query logging.
    pub log_statements: bool,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: "postgres://meticulous:meticulous@localhost:5432/meticulous".to_string(),
            max_connections: 10,
            min_connections: 1,
            connect_timeout_secs: 5,
            log_statements: false,
        }
    }
}

/// NATS message broker configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NatsConfig {
    /// NATS server URL.
    pub url: String,
    /// Path to credentials file (optional).
    pub credentials_file: Option<PathBuf>,
    /// Client name for identification.
    pub client_name: String,
}

impl Default for NatsConfig {
    fn default() -> Self {
        Self {
            url: "nats://localhost:4222".to_string(),
            credentials_file: None,
            client_name: "meticulous".to_string(),
        }
    }
}

/// gRPC server configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GrpcConfig {
    /// Address to listen on.
    pub listen_addr: String,
    /// Path to TLS certificate (optional).
    pub tls_cert: Option<PathBuf>,
    /// Path to TLS private key (optional).
    pub tls_key: Option<PathBuf>,
    /// Enable gRPC reflection for debugging.
    pub enable_reflection: bool,
}

impl Default for GrpcConfig {
    fn default() -> Self {
        Self {
            listen_addr: "0.0.0.0:9090".to_string(),
            tls_cert: None,
            tls_key: None,
            enable_reflection: true,
        }
    }
}

/// HTTP server configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HttpConfig {
    /// Address to listen on.
    pub listen_addr: String,
    /// Allowed CORS origins.
    #[serde(deserialize_with = "deserialize_cors_origins")]
    pub cors_origins: Vec<String>,
    /// Request body size limit in bytes.
    pub body_limit_bytes: usize,
    /// Request timeout in seconds.
    pub request_timeout_secs: u64,
    /// Mark agents offline in the database if their last heartbeat is older than this (API sweep).
    pub agent_stale_after_secs: u64,
    /// How often the API runs the stale-agent sweep.
    pub agent_stale_sweep_interval_secs: u64,
    /// Default page size for cursor-based list endpoints when `limit` / `per_page` is omitted.
    pub pagination_default_limit: u32,
    /// Maximum page size the API will return for list endpoints (client `limit` is clamped).
    pub pagination_max_limit: u32,
    /// Public base URL of this deployment (HTTPS in production), used as the OIDC `issuer`
    /// for workload identity (ADR-017). When unset, the first CORS origin is used as a dev fallback.
    #[serde(default)]
    pub public_base_url: Option<String>,
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            listen_addr: "0.0.0.0:8080".to_string(),
            cors_origins: vec!["http://localhost:5173".to_string()],
            body_limit_bytes: 10 * 1024 * 1024, // 10 MB
            request_timeout_secs: 30,
            agent_stale_after_secs: 90,
            agent_stale_sweep_interval_secs: 30,
            pagination_default_limit: 10_000,
            pagination_max_limit: 10_000,
            public_base_url: None,
        }
    }
}

/// Object storage configuration (S3-compatible).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct StorageConfig {
    /// S3-compatible endpoint URL.
    pub endpoint: String,
    /// Bucket name for artifacts.
    pub bucket: String,
    /// Access key (optional, can use IAM).
    pub access_key: Option<String>,
    /// Secret key (optional, can use IAM).
    pub secret_key: Option<String>,
    /// AWS region (optional).
    pub region: Option<String>,
    /// Use path-style URLs (required for some S3-compatible stores).
    pub path_style: bool,
    /// On startup, create the bucket if it does not exist (same credentials as the client).
    #[serde(default = "default_auto_create_bucket")]
    pub auto_create_bucket: bool,
}

fn default_auto_create_bucket() -> bool {
    true
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://localhost:8333".to_string(),
            bucket: "meticulous".to_string(),
            access_key: None,
            secret_key: None,
            region: None,
            path_style: true,
            auto_create_bucket: true,
        }
    }
}

/// Logging configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LogConfig {
    /// Log level filter (e.g., "info", "debug", "warn").
    pub level: String,
    /// Output format.
    pub format: LogFormat,
    /// Include span events in output.
    pub include_spans: bool,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            format: LogFormat::Text,
            include_spans: false,
        }
    }
}

/// Log output format.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    /// Human-readable text format.
    #[default]
    Text,
    /// Structured JSON format.
    Json,
    /// Compact single-line format.
    Compact,
}

/// Get the default configuration file search paths.
fn default_config_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    // System-wide config
    paths.push(PathBuf::from("/etc/meticulous/config.toml"));

    // User config
    if let Some(config_dir) = directories::ProjectDirs::from("", "", "meticulous") {
        paths.push(config_dir.config_dir().join("config.toml"));
    }

    // Project-local config
    paths.push(PathBuf::from("meticulous.toml"));

    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = MetConfig::default();
        assert_eq!(config.http.listen_addr, "0.0.0.0:8080");
        assert_eq!(config.http.pagination_default_limit, 10_000);
        assert_eq!(config.http.pagination_max_limit, 10_000);
        assert_eq!(config.database.max_connections, 10);
        assert_eq!(config.log.format, LogFormat::Text);
    }

    #[test]
    fn test_config_from_toml() {
        let toml = r#"
            [http]
            listen_addr = "127.0.0.1:3000"
            
            [database]
            max_connections = 20
            
            [log]
            level = "debug"
            format = "json"
        "#;

        let config = MetConfig::from_toml(toml).unwrap();
        assert_eq!(config.http.listen_addr, "127.0.0.1:3000");
        assert_eq!(config.database.max_connections, 20);
        assert_eq!(config.log.level, "debug");
        assert_eq!(config.log.format, LogFormat::Json);
    }

    #[test]
    fn test_config_serialization() {
        let config = MetConfig::default();
        let serialized = serde_json::to_string(&config).unwrap();
        assert!(serialized.contains("meticulous"));
    }

    #[test]
    fn test_met_database_double_underscore_url_maps_to_nested_url() {
        use config::Environment;
        use std::collections::HashMap;

        let mut env_map = HashMap::new();
        env_map.insert(
            "MET_DATABASE__URL".to_string(),
            "postgres://u:pw@pg.example:5432/app".to_string(),
        );
        let config: MetConfig = Config::builder()
            .add_source(
                Environment::default()
                    .prefix("MET")
                    .prefix_separator("_")
                    .separator("__")
                    .try_parsing(true)
                    .source(Some(env_map)),
            )
            .build()
            .unwrap()
            .try_deserialize()
            .unwrap();

        assert_eq!(config.database.url, "postgres://u:pw@pg.example:5432/app");
    }

    #[test]
    fn test_cors_origins_comma_separated_env_string() {
        use config::Environment;
        use std::collections::HashMap;

        let mut env_map = HashMap::new();
        env_map.insert(
            "MET_HTTP__CORS_ORIGINS".to_string(),
            "https://ci.example.com,http://localhost:5173".to_string(),
        );
        let config: MetConfig = Config::builder()
            .add_source(
                Environment::default()
                    .prefix("MET")
                    .prefix_separator("_")
                    .separator("__")
                    .try_parsing(true)
                    .source(Some(env_map)),
            )
            .build()
            .unwrap()
            .try_deserialize()
            .unwrap();

        assert_eq!(
            config.http.cors_origins,
            vec![
                "https://ci.example.com".to_string(),
                "http://localhost:5173".to_string(),
            ]
        );
    }

    #[test]
    fn test_cors_origins_toml_array_unchanged() {
        let toml = r#"
            [http]
            cors_origins = ["https://a.example", "https://b.example"]
        "#;
        let config = MetConfig::from_toml(toml).unwrap();
        assert_eq!(
            config.http.cors_origins,
            vec!["https://a.example", "https://b.example"]
        );
    }

    #[test]
    fn test_http_public_base_url_env() {
        use config::Environment;
        use std::collections::HashMap;

        let mut env_map = HashMap::new();
        env_map.insert(
            "MET_HTTP__PUBLIC_BASE_URL".to_string(),
            "https://issuer.example.com".to_string(),
        );
        let config: MetConfig = Config::builder()
            .add_source(
                Environment::default()
                    .prefix("MET")
                    .prefix_separator("_")
                    .separator("__")
                    .try_parsing(true)
                    .source(Some(env_map)),
            )
            .build()
            .unwrap()
            .try_deserialize()
            .unwrap();

        assert_eq!(
            config.http.public_base_url.as_deref(),
            Some("https://issuer.example.com")
        );
    }
}
