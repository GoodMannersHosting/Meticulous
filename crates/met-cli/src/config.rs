use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct CliConfig {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub context: ContextConfig,
    #[serde(default)]
    pub output: OutputConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_server_url")]
    pub url: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            url: default_server_url(),
        }
    }
}

fn default_server_url() -> String {
    "http://localhost:8080".to_string()
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ContextConfig {
    pub org: Option<String>,
    pub project: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputConfig {
    #[serde(default = "default_format")]
    pub format: String,
    #[serde(default = "default_color")]
    pub color: String,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            format: default_format(),
            color: default_color(),
        }
    }
}

fn default_format() -> String {
    "text".to_string()
}

fn default_color() -> String {
    "auto".to_string()
}

impl CliConfig {
    pub fn load() -> Self {
        let global = Self::load_global().unwrap_or_default();
        let local = Self::load_local().unwrap_or_default();
        global.merge(local)
    }

    fn load_global() -> Option<Self> {
        let path = global_config_path()?;
        Self::load_from_path(&path)
    }

    fn load_local() -> Option<Self> {
        let path = Path::new(".meticulous.toml");
        Self::load_from_path(path)
    }

    fn load_from_path(path: &Path) -> Option<Self> {
        let contents = std::fs::read_to_string(path).ok()?;
        toml::from_str(&contents).ok()
    }

    fn merge(self, other: Self) -> Self {
        Self {
            server: ServerConfig {
                url: if other.server.url != default_server_url() {
                    other.server.url
                } else {
                    self.server.url
                },
            },
            context: ContextConfig {
                org: other.context.org.or(self.context.org),
                project: other.context.project.or(self.context.project),
            },
            output: OutputConfig {
                format: if other.output.format != default_format() {
                    other.output.format
                } else {
                    self.output.format
                },
                color: if other.output.color != default_color() {
                    other.output.color
                } else {
                    self.output.color
                },
            },
        }
    }

    pub fn save_global(&self) -> Result<(), Box<dyn std::error::Error>> {
        let path = global_config_path().ok_or("Could not determine config directory")?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let contents = toml::to_string_pretty(self)?;
        std::fs::write(&path, contents)?;
        Ok(())
    }
}

pub fn global_config_path() -> Option<PathBuf> {
    ProjectDirs::from("com", "meticulous", "met").map(|dirs| dirs.config_dir().join("config.toml"))
}

#[allow(dead_code)] // Used by non-keyring token path; callers may grow later
pub fn global_config_dir() -> Option<PathBuf> {
    ProjectDirs::from("com", "meticulous", "met").map(|dirs| dirs.config_dir().to_path_buf())
}
