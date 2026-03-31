//! Agent configuration loading.

use std::path::{Path, PathBuf};
use std::time::Duration;

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::error::{AgentError, Result};

/// Agent configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentConfig {
    /// Controller gRPC URL.
    pub controller_url: String,
    /// Join token for registration.
    #[serde(skip_serializing)]
    pub join_token: Option<String>,
    /// Agent name/hostname.
    pub name: Option<String>,
    /// Agent pool membership.
    pub pool: Option<String>,
    /// Pool tags for job matching.
    pub pool_tags: Vec<String>,
    /// Agent labels.
    pub labels: Vec<String>,
    /// Maximum concurrent jobs.
    pub concurrency: i32,
    /// Workspace directory for job execution.
    pub workspace_dir: PathBuf,
    /// Log level.
    pub log_level: String,
    /// TLS configuration.
    pub tls: TlsConfig,
}

impl Default for AgentConfig {
    fn default() -> Self {
        let workspace_dir = if cfg!(unix) {
            PathBuf::from("/var/lib/meticulous/workspaces")
        } else {
            directories::BaseDirs::new()
                .map(|d| d.data_local_dir().join("meticulous").join("workspaces"))
                .unwrap_or_else(|| PathBuf::from("./workspaces"))
        };

        Self {
            controller_url: "http://localhost:9090".to_string(),
            join_token: None,
            name: None,
            pool: None,
            pool_tags: vec!["_default".to_string()],
            labels: Vec::new(),
            concurrency: 1,
            workspace_dir,
            log_level: "info".to_string(),
            tls: TlsConfig::default(),
        }
    }
}

/// TLS configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TlsConfig {
    /// CA certificate for verifying controller.
    pub ca_cert: Option<PathBuf>,
    /// Client certificate (populated after registration).
    pub client_cert: Option<PathBuf>,
    /// Client key (populated after registration).
    pub client_key: Option<PathBuf>,
}

impl AgentConfig {
    /// Load configuration from file, environment, and CLI args.
    ///
    /// Precedence (highest to lowest):
    /// 1. CLI arguments (passed in)
    /// 2. Environment variables (MET_*)
    /// 3. Config file
    /// 4. Defaults
    pub fn load(
        config_path: Option<&Path>,
        controller_url: Option<String>,
        join_token: Option<String>,
        name: Option<String>,
        pool: Option<String>,
        tags: Vec<String>,
    ) -> Result<Self> {
        // Start with defaults
        let mut config = Self::default();

        // Try to load from config file
        let config_file = config_path
            .map(PathBuf::from)
            .or_else(Self::default_config_path);

        if let Some(path) = config_file {
            if path.exists() {
                info!(path = %path.display(), "loading config file");
                let contents = std::fs::read_to_string(&path)?;
                config = toml::from_str(&contents)?;
            }
        }

        // Apply environment variables
        config.apply_env();

        // Apply CLI overrides
        if let Some(url) = controller_url {
            config.controller_url = url;
        }
        if let Some(token) = join_token {
            config.join_token = Some(token);
        }
        if let Some(n) = name {
            config.name = Some(n);
        }
        if let Some(p) = pool {
            config.pool = Some(p);
        }
        if !tags.is_empty() {
            config.pool_tags = tags;
        }

        // Validate
        config.validate()?;

        Ok(config)
    }

    /// Get the default config file path.
    fn default_config_path() -> Option<PathBuf> {
        // Check platform-specific locations
        let paths: Vec<PathBuf> = vec![
            // System-wide
            PathBuf::from("/etc/meticulous/agent.toml"),
            // Current directory
            PathBuf::from("./meticulous-agent.toml"),
        ];

        // Add user-specific path if available
        let mut all_paths = paths;
        if let Some(dirs) = ProjectDirs::from("dev", "meticulous", "agent") {
            all_paths.insert(1, dirs.config_dir().join("agent.toml"));
        }

        for path in all_paths {
            if path.exists() {
                return Some(path);
            }
        }

        None
    }

    /// Apply environment variables.
    fn apply_env(&mut self) {
        if let Ok(url) = std::env::var("MET_CONTROLLER_URL") {
            self.controller_url = url;
        }
        if let Ok(token) = std::env::var("MET_JOIN_TOKEN") {
            self.join_token = Some(token);
        }
        if let Ok(name) = std::env::var("MET_AGENT_NAME") {
            self.name = Some(name);
        }
        if let Ok(pool) = std::env::var("MET_AGENT_POOL") {
            self.pool = Some(pool);
        }
        if let Ok(tags) = std::env::var("MET_AGENT_TAGS") {
            self.pool_tags = tags.split(',').map(|s| s.trim().to_string()).collect();
        }
        if let Ok(labels) = std::env::var("MET_AGENT_LABELS") {
            self.labels = labels.split(',').map(|s| s.trim().to_string()).collect();
        }
        if let Ok(concurrency) = std::env::var("MET_AGENT_CONCURRENCY") {
            if let Ok(c) = concurrency.parse() {
                self.concurrency = c;
            }
        }
        if let Ok(workspace) = std::env::var("MET_WORKSPACE_DIR") {
            self.workspace_dir = PathBuf::from(workspace);
        }
        if let Ok(level) = std::env::var("MET_LOG_LEVEL") {
            self.log_level = level;
        }
    }

    /// Validate the configuration.
    fn validate(&self) -> Result<()> {
        if self.controller_url.is_empty() {
            return Err(AgentError::Config("controller_url is required".to_string()));
        }
        if self.concurrency < 1 {
            return Err(AgentError::Config(
                "concurrency must be at least 1".to_string(),
            ));
        }
        Ok(())
    }

    /// Get the agent name, falling back to hostname.
    pub fn agent_name(&self) -> String {
        self.name
            .clone()
            .unwrap_or_else(|| hostname::get()
                .ok()
                .and_then(|h| h.into_string().ok())
                .unwrap_or_else(|| "unknown".to_string()))
    }

    /// Get the data directory for storing agent state.
    pub fn data_dir(&self) -> PathBuf {
        if cfg!(unix) {
            PathBuf::from("/var/lib/meticulous")
        } else {
            directories::BaseDirs::new()
                .map(|d| d.data_local_dir().join("meticulous"))
                .unwrap_or_else(|| PathBuf::from("./data"))
        }
    }

    /// Get the path to the agent identity file.
    pub fn identity_path(&self) -> PathBuf {
        self.data_dir().join("agent-identity.json")
    }
}

/// Persisted agent identity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentIdentity {
    /// Agent ID assigned by controller.
    pub agent_id: String,
    /// Organization ID.
    pub org_id: String,
    /// JWT token.
    pub jwt_token: String,
    /// JWT expiration timestamp.
    pub jwt_expires_at: i64,
    /// Whether the JWT is renewable.
    pub renewable: bool,
    /// NATS subjects to subscribe to.
    pub nats_subjects: Vec<String>,
    /// NATS URL.
    pub nats_url: String,
}

impl AgentIdentity {
    /// Load identity from file.
    pub fn load(path: &Path) -> Result<Option<Self>> {
        if !path.exists() {
            return Ok(None);
        }
        let contents = std::fs::read_to_string(path)?;
        let identity: Self = serde_json::from_str(&contents)?;
        Ok(Some(identity))
    }

    /// Save identity to file.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let contents = serde_json::to_string_pretty(self)?;
        std::fs::write(path, contents)?;
        Ok(())
    }

    /// Check if the JWT is expired.
    pub fn is_jwt_expired(&self) -> bool {
        let now = chrono::Utc::now().timestamp();
        now >= self.jwt_expires_at
    }

    /// Check if the JWT needs renewal (within 10% of expiry).
    pub fn needs_jwt_renewal(&self) -> bool {
        if !self.renewable {
            return false;
        }
        let now = chrono::Utc::now().timestamp();
        let remaining = self.jwt_expires_at - now;
        let total = 24 * 60 * 60; // Assume 24h validity
        remaining <= (total as f64 * 0.1) as i64
    }
}
