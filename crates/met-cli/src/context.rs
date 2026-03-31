use crate::config::CliConfig;

/// Resolved runtime context for org/project/server selection.
///
/// Resolution order (highest priority first):
/// 1. CLI flags (--org, --project, --server)
/// 2. Environment variables (MET_ORG, MET_PROJECT, MET_SERVER_URL)
/// 3. Project-local `.meticulous.toml`
/// 4. Global config `~/.config/meticulous/config.toml`
#[derive(Debug, Clone)]
pub struct ResolvedContext {
    pub server_url: String,
    pub org: Option<String>,
    pub project: Option<String>,
}

impl ResolvedContext {
    pub fn resolve(
        config: &CliConfig,
        flag_server: Option<&str>,
        flag_org: Option<&str>,
        flag_project: Option<&str>,
    ) -> Self {
        let server_url = flag_server
            .map(String::from)
            .or_else(|| std::env::var("MET_SERVER_URL").ok())
            .unwrap_or_else(|| config.server.url.clone());

        let org = flag_org
            .map(String::from)
            .or_else(|| std::env::var("MET_ORG").ok())
            .or_else(|| config.context.org.clone());

        let project = flag_project
            .map(String::from)
            .or_else(|| std::env::var("MET_PROJECT").ok())
            .or_else(|| config.context.project.clone());

        Self {
            server_url,
            org,
            project,
        }
    }

    pub fn require_org(&self) -> Result<&str, crate::api_client::ApiError> {
        self.org.as_deref().ok_or_else(|| {
            crate::api_client::ApiError::Config(
                "No organization set. Use --org, MET_ORG env var, or `met org switch`.".into(),
            )
        })
    }

    pub fn require_project(&self) -> Result<&str, crate::api_client::ApiError> {
        self.project.as_deref().ok_or_else(|| {
            crate::api_client::ApiError::Config(
                "No project set. Use --project, MET_PROJECT env var, or `met project switch`."
                    .into(),
            )
        })
    }
}
