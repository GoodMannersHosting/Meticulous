//! Git workflow provider for project workflows.
//!
//! Fetches reusable workflow definitions from git repositories,
//! specifically from the `.stable/workflows/` directory.

use async_trait::async_trait;
use std::path::{Path, PathBuf};
use tracing::{debug, instrument};

use crate::ir::WorkflowScope;
use crate::schema::RawWorkflowDef;
use crate::semver::{parse_version_constraint, resolve_version};
use crate::workflow::{WorkflowFetchError, WorkflowProvider};

/// Git-backed workflow provider for project workflows.
///
/// This provider fetches workflow definitions from a git repository's
/// `.stable/workflows/` directory. It can work with either a local
/// checkout or by cloning the repository.
pub struct GitWorkflowProvider {
    /// Path to the local repository checkout.
    repo_path: PathBuf,
    /// Workflows directory relative to repo root.
    workflows_dir: String,
}

impl GitWorkflowProvider {
    /// Create a new git workflow provider.
    ///
    /// # Arguments
    ///
    /// * `repo_path` - Path to the local repository checkout
    /// * `workflows_dir` - Directory containing workflows (default: `.stable/workflows`)
    pub fn new(repo_path: impl Into<PathBuf>, workflows_dir: Option<String>) -> Self {
        Self {
            repo_path: repo_path.into(),
            workflows_dir: workflows_dir.unwrap_or_else(|| ".stable/workflows".to_string()),
        }
    }

    /// Get the full path to a workflow file.
    fn workflow_path(&self, name: &str, version: Option<&str>) -> PathBuf {
        let mut path = self.repo_path.join(&self.workflows_dir).join(name);
        if let Some(v) = version
            && v != "latest"
        {
            path = path.with_file_name(format!("{}@{}.yaml", name, v));
        }
        if path.extension().is_none() {
            path.set_extension("yaml");
        }
        path
    }

    /// List workflow files in the directory.
    fn list_workflow_files(&self, name: &str) -> Vec<(String, PathBuf)> {
        let dir = self.repo_path.join(&self.workflows_dir);
        let mut workflows = Vec::new();

        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(filename) = path.file_stem().and_then(|s| s.to_str())
                    && filename.starts_with(name)
                    && let Some(version) = extract_version_from_filename(filename, name)
                {
                    workflows.push((version, path));
                }
            }
        }

        workflows
    }

    /// Read and parse a workflow file.
    async fn read_workflow(&self, path: &Path) -> Result<RawWorkflowDef, WorkflowFetchError> {
        let content = tokio::fs::read_to_string(path)
            .await
            .map_err(WorkflowFetchError::Io)?;

        serde_yaml::from_str(&content).map_err(|e| WorkflowFetchError::Parse(e.to_string()))
    }
}

#[async_trait]
impl WorkflowProvider for GitWorkflowProvider {
    #[instrument(skip(self), fields(scope = ?scope, name = %name, version = %version))]
    async fn fetch(
        &self,
        scope: WorkflowScope,
        name: &str,
        version: &str,
    ) -> Result<RawWorkflowDef, WorkflowFetchError> {
        if scope != WorkflowScope::Project {
            return Err(WorkflowFetchError::NotFound {
                scope: "global".to_string(),
                name: name.to_string(),
            });
        }

        let resolved_version = if version == "latest" {
            let versions = self.list_versions(scope, name).await?;
            versions
                .first()
                .cloned()
                .unwrap_or_else(|| "latest".to_string())
        } else if let Ok(constraint) = parse_version_constraint(version) {
            let versions = self.list_versions(scope, name).await?;
            resolve_version(&constraint, &versions).unwrap_or_else(|| version.to_string())
        } else {
            version.to_string()
        };

        debug!(name, version = %resolved_version, "fetching workflow from git");

        let path = self.workflow_path(name, Some(&resolved_version));

        if path.exists() {
            let mut workflow = self.read_workflow(&path).await?;
            if workflow.version.is_none() {
                workflow.version = Some(resolved_version);
            }
            return Ok(workflow);
        }

        let base_path = self.workflow_path(name, None);
        if base_path.exists() {
            let mut workflow = self.read_workflow(&base_path).await?;
            if workflow.version.is_none() {
                workflow.version = Some(resolved_version);
            }
            return Ok(workflow);
        }

        Err(WorkflowFetchError::NotFound {
            scope: "project".to_string(),
            name: name.to_string(),
        })
    }

    #[instrument(skip(self), fields(scope = ?scope, name = %name))]
    async fn list_versions(
        &self,
        scope: WorkflowScope,
        name: &str,
    ) -> Result<Vec<String>, WorkflowFetchError> {
        if scope != WorkflowScope::Project {
            return Ok(Vec::new());
        }

        let workflows = self.list_workflow_files(name);
        let mut versions: Vec<String> = workflows.into_iter().map(|(v, _)| v).collect();

        let base_path = self.workflow_path(name, None);
        if base_path.exists() && !versions.contains(&"latest".to_string()) {
            versions.push("latest".to_string());
        }

        versions.sort_by(|a, b| {
            if a == "latest" {
                std::cmp::Ordering::Greater
            } else if b == "latest" {
                std::cmp::Ordering::Less
            } else {
                semver_compare(b, a)
            }
        });

        Ok(versions)
    }
}

/// Extract version from a workflow filename.
///
/// Expects format: `name@version.yaml` or just `name.yaml` (returns "latest")
fn extract_version_from_filename(filename: &str, name: &str) -> Option<String> {
    if filename == name {
        return Some("latest".to_string());
    }

    let prefix = format!("{}@", name);
    if filename.starts_with(&prefix) {
        let version = &filename[prefix.len()..];
        return Some(version.to_string());
    }

    None
}

/// Compare two semver strings.
fn semver_compare(a: &str, b: &str) -> std::cmp::Ordering {
    let parse = |s: &str| -> (u32, u32, u32) {
        let s = s.trim_start_matches('v');
        let parts: Vec<&str> = s.split('.').collect();
        (
            parts.first().and_then(|p| p.parse().ok()).unwrap_or(0),
            parts.get(1).and_then(|p| p.parse().ok()).unwrap_or(0),
            parts
                .get(2)
                .and_then(|p| p.split('-').next())
                .and_then(|p| p.parse().ok())
                .unwrap_or(0),
        )
    };

    parse(a).cmp(&parse(b))
}

/// Composite workflow provider that checks multiple sources.
pub struct CompositeWorkflowProvider {
    database: Option<Box<dyn WorkflowProvider>>,
    git: Option<Box<dyn WorkflowProvider>>,
}

impl CompositeWorkflowProvider {
    /// Create a new composite provider.
    pub fn new() -> Self {
        Self {
            database: None,
            git: None,
        }
    }

    /// Add a database provider for global workflows.
    pub fn with_database(mut self, provider: impl WorkflowProvider + 'static) -> Self {
        self.database = Some(Box::new(provider));
        self
    }

    /// Add a git provider for project workflows.
    pub fn with_git(mut self, provider: impl WorkflowProvider + 'static) -> Self {
        self.git = Some(Box::new(provider));
        self
    }
}

impl Default for CompositeWorkflowProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl WorkflowProvider for CompositeWorkflowProvider {
    async fn fetch(
        &self,
        scope: WorkflowScope,
        name: &str,
        version: &str,
    ) -> Result<RawWorkflowDef, WorkflowFetchError> {
        match scope {
            WorkflowScope::Global => {
                if let Some(db) = &self.database {
                    db.fetch(scope, name, version).await
                } else {
                    Err(WorkflowFetchError::NotFound {
                        scope: "global".to_string(),
                        name: name.to_string(),
                    })
                }
            }
            WorkflowScope::Project => {
                if let Some(git) = &self.git {
                    git.fetch(scope, name, version).await
                } else {
                    Err(WorkflowFetchError::NotFound {
                        scope: "project".to_string(),
                        name: name.to_string(),
                    })
                }
            }
        }
    }

    async fn list_versions(
        &self,
        scope: WorkflowScope,
        name: &str,
    ) -> Result<Vec<String>, WorkflowFetchError> {
        match scope {
            WorkflowScope::Global => {
                if let Some(db) = &self.database {
                    db.list_versions(scope, name).await
                } else {
                    Ok(Vec::new())
                }
            }
            WorkflowScope::Project => {
                if let Some(git) = &self.git {
                    git.list_versions(scope, name).await
                } else {
                    Ok(Vec::new())
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_version() {
        assert_eq!(
            extract_version_from_filename("docker-build", "docker-build"),
            Some("latest".to_string())
        );
        assert_eq!(
            extract_version_from_filename("docker-build@1.0.0", "docker-build"),
            Some("1.0.0".to_string())
        );
        assert_eq!(
            extract_version_from_filename("docker-build@v2.1.0", "docker-build"),
            Some("v2.1.0".to_string())
        );
        assert_eq!(
            extract_version_from_filename("other-workflow", "docker-build"),
            None
        );
    }

    #[test]
    fn test_semver_compare() {
        assert_eq!(semver_compare("1.0.0", "1.0.0"), std::cmp::Ordering::Equal);
        assert_eq!(
            semver_compare("2.0.0", "1.0.0"),
            std::cmp::Ordering::Greater
        );
        assert_eq!(
            semver_compare("1.1.0", "1.0.0"),
            std::cmp::Ordering::Greater
        );
        assert_eq!(
            semver_compare("1.0.1", "1.0.0"),
            std::cmp::Ordering::Greater
        );
        assert_eq!(semver_compare("v1.0.0", "1.0.0"), std::cmp::Ordering::Equal);
    }
}
