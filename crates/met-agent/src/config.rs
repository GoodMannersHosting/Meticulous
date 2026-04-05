//! Agent configuration loading.

use std::path::{Path, PathBuf};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::error::{AgentError, Result};

/// How the agent runs pipeline steps (shell commands).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExecutionRuntime {
    /// Run steps as local processes on the agent host (default).
    #[default]
    Native,
    /// Linux: run inside Docker/Podman (requires a compatible `image` on each step).
    Container,
    /// Linux: use a container runtime if available, otherwise [`Native`](Self::Native).
    Auto,
}

impl std::str::FromStr for ExecutionRuntime {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "native" | "local" => Ok(Self::Native),
            "container" | "docker" | "podman" => Ok(Self::Container),
            "auto" => Ok(Self::Auto),
            _ => Err(format!(
                "expected 'native', 'container', or 'auto', got '{s}'"
            )),
        }
    }
}

/// Where the effective join token came from (used to decide whether to write `~/.met/agentconfig.toml`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JoinTokenSource {
    /// No join token in the merged configuration.
    None,
    /// Read from a config file on disk.
    FromFile(PathBuf),
    /// Set via `MET_JOIN_TOKEN`.
    FromEnv,
    /// Set via CLI (`--join-token`).
    FromCli,
    /// Entered interactively at startup.
    FromInteractive,
}

impl JoinTokenSource {
    /// Whether to write `~/.met/agentconfig.toml` after successful registration.
    #[must_use]
    pub fn should_persist_registration_config(&self) -> bool {
        matches!(
            self,
            Self::FromCli | Self::FromEnv | Self::FromInteractive
        )
    }
}

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
    /// Whether steps run on the host (`native`) or in a Linux container runtime (`container` / `auto`).
    pub execution_runtime: ExecutionRuntime,
    /// TLS configuration.
    pub tls: TlsConfig,
}

fn default_data_dir() -> PathBuf {
    directories::BaseDirs::new()
        .map(|d| d.data_local_dir().join("meticulous"))
        .unwrap_or_else(|| PathBuf::from("./data"))
}

impl Default for AgentConfig {
    fn default() -> Self {
        let root = default_data_dir();
        let workspace_dir = root.join("workspaces");

        Self {
            controller_url: "http://127.0.0.1:9090".to_string(),
            join_token: None,
            name: None,
            pool: None,
            pool_tags: vec!["_default".to_string()],
            labels: Vec::new(),
            concurrency: 1,
            workspace_dir,
            log_level: "info".to_string(),
            execution_runtime: ExecutionRuntime::Native,
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

/// Paths to probe for a basename like `agentconfig` (no extension → try `.toml`, `.yaml`, `.yml`).
fn agentconfig_path_variants(dir: &Path, basename: &str) -> Vec<PathBuf> {
    let base = dir.join(basename);
    vec![
        base.clone(),
        base.with_extension("toml"),
        base.with_extension("yaml"),
        base.with_extension("yml"),
    ]
}

fn parse_config_file(path: &Path, contents: &str) -> Result<AgentConfig> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase());

    match ext.as_deref() {
        Some("yaml") | Some("yml") => serde_yaml::from_str(contents).map_err(Into::into),
        Some("toml") => toml::from_str(contents).map_err(Into::into),
        _ => match toml::from_str::<AgentConfig>(contents) {
            Ok(parsed) => Ok(parsed),
            Err(toml_err) => match serde_yaml::from_str::<AgentConfig>(contents) {
                Ok(parsed) => Ok(parsed),
                Err(yaml_err) => Err(AgentError::Config(format!(
                    "{}: could not parse as TOML ({toml_err}) or YAML ({yaml_err})",
                    path.display()
                ))),
            },
        },
    }
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
    ) -> Result<(Self, JoinTokenSource)> {
        // Start with defaults
        let mut config = Self::default();

        let explicit_path = config_path.map(PathBuf::from);
        let config_file = explicit_path
            .clone()
            .or_else(Self::default_config_path);

        let mut loaded_from_path: Option<PathBuf> = None;

        if let Some(path) = config_file {
            if explicit_path.as_ref().is_some_and(|p| p == &path) && !path.exists() {
                return Err(AgentError::Config(format!(
                    "config file not found: {}",
                    path.display()
                )));
            }
            if path.exists() {
                loaded_from_path = Some(path.clone());
                info!(path = %path.display(), "loading config file");
                let contents = std::fs::read_to_string(&path)?;
                config = parse_config_file(&path, &contents)?;
            }
        }

        let env_set_token = std::env::var("MET_JOIN_TOKEN")
            .ok()
            .filter(|s| !s.is_empty())
            .is_some();

        // Apply environment variables
        config.apply_env();

        // Apply CLI overrides
        if let Some(url) = controller_url {
            config.controller_url = url;
        }
        if let Some(token) = join_token.clone() {
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

        let join_token_source = if join_token.clone().is_some() {
            JoinTokenSource::FromCli
        } else if env_set_token {
            JoinTokenSource::FromEnv
        } else if config.join_token.is_some() {
            loaded_from_path
                .map(JoinTokenSource::FromFile)
                .unwrap_or(JoinTokenSource::None)
        } else {
            JoinTokenSource::None
        };

        // Validate
        config.validate()?;

        Ok((config, join_token_source))
    }

    /// Default path for a user-writable registration config (TOML).
    #[must_use]
    pub fn user_registration_config_path() -> Option<PathBuf> {
        directories::BaseDirs::new().map(|b| b.home_dir().join(".met").join("agentconfig.toml"))
    }

    /// Write the current config (including join token) to [`user_registration_config_path`].
    ///
    /// Creates `~/.met` with restrictive permissions on Unix. The file is written with mode `0600`
    /// on Unix. Does not log secret values.
    pub fn write_user_registration_file(&self) -> Result<PathBuf> {
        let path = Self::user_registration_config_path().ok_or_else(|| {
            AgentError::Config("cannot resolve home directory for agent config".to_string())
        })?;

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(AgentError::from)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700));
            }
        }

        #[derive(Serialize)]
        struct Persisted {
            controller_url: String,
            join_token: Option<String>,
            name: Option<String>,
            pool: Option<String>,
            pool_tags: Vec<String>,
            labels: Vec<String>,
            concurrency: i32,
            workspace_dir: PathBuf,
            log_level: String,
            execution_runtime: ExecutionRuntime,
            tls: TlsConfig,
        }

        let body = Persisted {
            controller_url: self.controller_url.clone(),
            join_token: self.join_token.clone(),
            name: self.name.clone(),
            pool: self.pool.clone(),
            pool_tags: self.pool_tags.clone(),
            labels: self.labels.clone(),
            concurrency: self.concurrency,
            workspace_dir: self.workspace_dir.clone(),
            log_level: self.log_level.clone(),
            execution_runtime: self.execution_runtime,
            tls: self.tls.clone(),
        };

        let toml = toml::to_string_pretty(&body).map_err(|e| AgentError::Config(e.to_string()))?;
        std::fs::write(&path, toml).map_err(AgentError::from)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
                .map_err(AgentError::from)?;
        }

        info!(path = %path.display(), "wrote agent registration config");
        Ok(path)
    }

    /// Get the default config file path (first existing).
    ///
    /// Order: `./meticulous-agent.toml`, `~/.met/agentconfig*`, XDG `agent.toml`,
    /// `/opt/met-agent/agentconfig*`, `/etc/meticulous/agent.toml`.
    fn default_config_path() -> Option<PathBuf> {
        let mut candidates: Vec<PathBuf> = Vec::new();

        candidates.push(PathBuf::from("./meticulous-agent.toml"));

        if let Some(home) = directories::BaseDirs::new().map(|b| b.home_dir().to_path_buf()) {
            candidates.extend(agentconfig_path_variants(&home.join(".met"), "agentconfig"));
        }

        if let Some(dirs) = ProjectDirs::from("dev", "meticulous", "agent") {
            candidates.push(dirs.config_dir().join("agent.toml"));
        }

        candidates.extend(agentconfig_path_variants(
            Path::new("/opt/met-agent"),
            "agentconfig",
        ));

        candidates.push(PathBuf::from("/etc/meticulous/agent.toml"));

        for path in candidates {
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
        if let Ok(v) = std::env::var("MET_EXECUTION_RUNTIME") {
            match v.parse::<ExecutionRuntime>() {
                Ok(rt) => self.execution_runtime = rt,
                Err(e) => warn!(value = %v, error = %e, "ignoring invalid MET_EXECUTION_RUNTIME"),
            }
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

    /// Get the data directory for storing agent state (e.g. identity file).
    ///
    /// Override with `MET_AGENT_DATA_DIR`. For systemd deployments that should use
    /// `/var/lib/meticulous`, set that explicitly in the service environment.
    pub fn data_dir(&self) -> PathBuf {
        if let Ok(p) = std::env::var("MET_AGENT_DATA_DIR") {
            return PathBuf::from(p);
        }
        default_data_dir()
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
    /// NATS user JWT (from controller registration).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nats_user_jwt: Option<String>,
    /// NATS user NKey seed (from controller registration).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nats_user_seed: Option<String>,
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

    /// JetStream pull `filter_subject` for this agent's job inbox.
    ///
    /// Must align with controller dispatch (`met.jobs.{org}.{pool}.{agent_id}`) and
    /// [`met_controller::nats::subjects::job_inbox_filter`]. Older identity files stored
    /// `met.jobs.{org}._default`, which **overlaps** per-agent subjects on a WorkQueue stream
    /// and triggers NATS error 10100 if another consumer also matches those messages.
    pub fn job_pull_filter_subject(&self) -> String {
        if let Ok(org) = uuid::Uuid::parse_str(self.org_id.trim()) {
            format!("met.jobs.{}.*.{}", org, self.agent_id)
        } else {
            self.nats_subjects
                .first()
                .cloned()
                .unwrap_or_else(|| "met.jobs.*._default".to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use tempfile::NamedTempFile;

    use super::*;
    use crate::error::AgentError;

    #[test]
    fn load_toml_file() {
        let mut f = NamedTempFile::new().unwrap();
        writeln!(f, r#"controller_url = "http://example:9090""#).unwrap();
        let (c, _) = AgentConfig::load(Some(f.path()), None, None, None, None, vec![]).unwrap();
        assert_eq!(c.controller_url, "http://example:9090");
    }

    #[test]
    fn job_pull_filter_subject_matches_controller_inbox() {
        let org = uuid::Uuid::new_v4();
        let identity = AgentIdentity {
            agent_id: "agt_test123".to_string(),
            org_id: org.to_string(),
            jwt_token: "x".to_string(),
            jwt_expires_at: 0,
            renewable: true,
            nats_subjects: vec![format!("met.jobs.{org}._default")],
            nats_url: "nats://localhost:4222".to_string(),
            nats_user_jwt: None,
            nats_user_seed: None,
        };
        assert_eq!(
            identity.job_pull_filter_subject(),
            format!("met.jobs.{org}.*.agt_test123")
        );
    }

    #[test]
    fn load_yaml_by_extension() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("agent.yaml");
        std::fs::write(&path, "controller_url: http://yaml:9090\n").unwrap();
        let (c, _) = AgentConfig::load(Some(&path), None, None, None, None, vec![]).unwrap();
        assert_eq!(c.controller_url, "http://yaml:9090");
    }

    #[test]
    fn load_extensionless_as_yaml() {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(b"controller_url: http://extless:9090\n").unwrap();
        let (c, _) = AgentConfig::load(Some(f.path()), None, None, None, None, vec![]).unwrap();
        assert_eq!(c.controller_url, "http://extless:9090");
    }

    #[test]
    fn cli_overrides_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.toml");
        std::fs::write(&path, r#"controller_url = "http://from-file:9090""#).unwrap();
        let (c, _) = AgentConfig::load(
            Some(&path),
            Some("http://from-cli:9090".to_string()),
            None,
            None,
            None,
            vec![],
        )
        .unwrap();
        assert_eq!(c.controller_url, "http://from-cli:9090");
    }

    #[test]
    fn rejects_empty_controller_url() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.toml");
        std::fs::write(&path, r#"controller_url = """#).unwrap();
        let err = AgentConfig::load(Some(&path), None, None, None, None, vec![]).unwrap_err();
        match err {
            AgentError::Config(msg) => assert!(msg.contains("controller")),
            _ => panic!("unexpected error: {err:?}"),
        }
    }

    #[test]
    fn explicit_path_missing_errors() {
        let err = AgentConfig::load(
            Some(Path::new("/nonexistent/met-agent-config.toml")),
            None,
            None,
            None,
            None,
            vec![],
        )
        .unwrap_err();
        match err {
            AgentError::Config(msg) => {
                assert!(msg.contains("not found"), "{msg}");
            }
            _ => panic!("unexpected error: {err:?}"),
        }
    }
}
