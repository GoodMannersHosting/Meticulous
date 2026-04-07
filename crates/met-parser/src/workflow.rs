//! Workflow resolution and inlining.
//!
//! Handles fetching and resolving reusable workflow references.

use crate::error::{ErrorCode, ParseDiagnostics, ParseError, SourceLocation};
use crate::ir::{WorkflowRef, WorkflowScope, defaults};
use crate::schema::RawWorkflowDef;
use async_trait::async_trait;
use std::collections::HashSet;

/// Trait for workflow providers.
///
/// Implementations fetch workflow definitions from various sources:
/// - Global workflows from the database or a dedicated git repo
/// - Project workflows from the project's repository
#[async_trait]
pub trait WorkflowProvider: Send + Sync {
    /// Fetch a workflow by scope, name, and version.
    async fn fetch(
        &self,
        scope: WorkflowScope,
        name: &str,
        version: &str,
    ) -> Result<RawWorkflowDef, WorkflowFetchError>;

    /// List available versions for a workflow.
    async fn list_versions(
        &self,
        scope: WorkflowScope,
        name: &str,
    ) -> Result<Vec<String>, WorkflowFetchError>;
}

/// Error fetching a workflow.
#[derive(Debug, thiserror::Error)]
pub enum WorkflowFetchError {
    #[error("workflow not found: {scope}/{name}")]
    NotFound { scope: String, name: String },

    #[error("version not found: {scope}/{name}@{version}")]
    VersionNotFound {
        scope: String,
        name: String,
        version: String,
    },

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("parse error: {0}")]
    Parse(String),

    #[error("network error: {0}")]
    Network(String),
}

/// Parse a workflow reference string.
///
/// Format: "scope/name" where scope is "global" or "project".
pub fn parse_workflow_ref(
    reference: &str,
    version: Option<&str>,
) -> Result<(WorkflowScope, String, String), ParseError> {
    let parts: Vec<&str> = reference.split('/').collect();

    if parts.len() != 2 {
        return Err(ParseError::new(
            ErrorCode::E3002,
            format!(
                "invalid workflow reference '{}': expected 'scope/name' format",
                reference
            ),
        )
        .with_hint("use 'global/workflow-name' or 'project/workflow-name'"));
    }

    let scope = match parts[0] {
        "global" => WorkflowScope::Global,
        "project" => WorkflowScope::Project,
        other => {
            return Err(ParseError::new(
                ErrorCode::E3002,
                format!("invalid workflow scope '{}': expected 'global' or 'project'", other),
            )
            .with_hint(
                "workflow must be 'project/<workflow>' or 'global/<workflow>' (file stem under .stable/workflows/). It is not a GitHub owner/repo — put owner/repo in vars and pass it via inputs.",
            ));
        }
    };

    let name = parts[1].to_string();
    let version = version.unwrap_or("latest").to_string();

    Ok((scope, name, version))
}

/// Workflow resolution context.
pub struct WorkflowResolver<'a> {
    provider: &'a dyn WorkflowProvider,
    /// Workflows currently being resolved (for cycle detection).
    resolving: HashSet<String>,
    /// Current nesting depth.
    depth: usize,
}

impl<'a> WorkflowResolver<'a> {
    /// Create a new resolver.
    pub fn new(provider: &'a dyn WorkflowProvider) -> Self {
        Self {
            provider,
            resolving: HashSet::new(),
            depth: 0,
        }
    }

    /// Resolve a workflow reference.
    pub async fn resolve(
        &mut self,
        reference: &str,
        version: Option<&str>,
        diagnostics: &mut ParseDiagnostics,
        location: SourceLocation,
    ) -> Option<(RawWorkflowDef, WorkflowRef)> {
        // Check nesting depth
        if self.depth >= defaults::MAX_WORKFLOW_DEPTH {
            diagnostics.push(
                ParseError::new(
                    ErrorCode::E3005,
                    format!(
                        "maximum workflow nesting depth ({}) exceeded",
                        defaults::MAX_WORKFLOW_DEPTH
                    ),
                )
                .with_source(location)
                .with_hint("reduce workflow nesting or consolidate workflows"),
            );
            return None;
        }

        // Parse the reference
        let (scope, name, version) = match parse_workflow_ref(reference, version) {
            Ok(r) => r,
            Err(e) => {
                diagnostics.push(e.with_source(location));
                return None;
            }
        };

        // Check for circular references
        let key = format!("{:?}/{}", scope, name);
        if self.resolving.contains(&key) {
            diagnostics.push(
                ParseError::new(
                    ErrorCode::E3004,
                    format!("circular workflow reference: {}", reference),
                )
                .with_source(location),
            );
            return None;
        }

        // Fetch the workflow
        self.resolving.insert(key.clone());
        self.depth += 1;

        let result = match self.provider.fetch(scope, &name, &version).await {
            Ok(workflow) => {
                let workflow_ref = WorkflowRef {
                    scope,
                    name: name.clone(),
                    version: workflow.version.clone().unwrap_or(version.clone()),
                };
                Some((workflow, workflow_ref))
            }
            Err(WorkflowFetchError::NotFound { scope, name }) => {
                diagnostics.push(
                    ParseError::new(
                        ErrorCode::E3001,
                        format!("workflow not found: {}/{}", scope, name),
                    )
                    .with_source(location)
                    .with_hint("check the workflow name and scope"),
                );
                None
            }
            Err(WorkflowFetchError::VersionNotFound {
                scope,
                name,
                version,
            }) => {
                diagnostics.push(
                    ParseError::new(
                        ErrorCode::E3003,
                        format!("workflow version not found: {}/{}@{}", scope, name, version),
                    )
                    .with_source(location)
                    .with_hint("check available versions or use 'latest'"),
                );
                None
            }
            Err(e) => {
                diagnostics.push(
                    ParseError::new(ErrorCode::E9001, format!("error fetching workflow: {}", e))
                        .with_source(location),
                );
                None
            }
        };

        self.depth -= 1;
        self.resolving.remove(&key);

        result
    }
}

/// A mock workflow provider for testing.
#[derive(Debug, Default)]
pub struct MockWorkflowProvider {
    workflows: std::collections::HashMap<String, RawWorkflowDef>,
}

impl MockWorkflowProvider {
    /// Create a new mock provider.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a workflow to the mock.
    pub fn add_workflow(&mut self, scope: WorkflowScope, name: &str, workflow: RawWorkflowDef) {
        let key = format!("{:?}/{}", scope, name);
        self.workflows.insert(key, workflow);
    }
}

#[async_trait]
impl WorkflowProvider for MockWorkflowProvider {
    async fn fetch(
        &self,
        scope: WorkflowScope,
        name: &str,
        _version: &str,
    ) -> Result<RawWorkflowDef, WorkflowFetchError> {
        let key = format!("{:?}/{}", scope, name);
        self.workflows
            .get(&key)
            .cloned()
            .ok_or_else(|| WorkflowFetchError::NotFound {
                scope: format!("{:?}", scope).to_lowercase(),
                name: name.to_string(),
            })
    }

    async fn list_versions(
        &self,
        scope: WorkflowScope,
        name: &str,
    ) -> Result<Vec<String>, WorkflowFetchError> {
        let key = format!("{:?}/{}", scope, name);
        if self.workflows.contains_key(&key) {
            Ok(vec!["latest".to_string()])
        } else {
            Err(WorkflowFetchError::NotFound {
                scope: format!("{:?}", scope).to_lowercase(),
                name: name.to_string(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_workflow_ref() {
        let (scope, name, version) =
            parse_workflow_ref("global/docker-build", Some("v1.0")).unwrap();
        assert_eq!(scope, WorkflowScope::Global);
        assert_eq!(name, "docker-build");
        assert_eq!(version, "v1.0");

        let (scope, name, version) = parse_workflow_ref("project/my-workflow", None).unwrap();
        assert_eq!(scope, WorkflowScope::Project);
        assert_eq!(name, "my-workflow");
        assert_eq!(version, "latest");
    }

    #[test]
    fn test_invalid_workflow_ref() {
        assert!(parse_workflow_ref("invalid", None).is_err());
        assert!(parse_workflow_ref("unknown/workflow", None).is_err());
        assert!(parse_workflow_ref("too/many/parts", None).is_err());
    }

    #[tokio::test]
    async fn test_mock_provider() {
        use crate::schema::RawWorkflowDef;
        use indexmap::IndexMap;

        let mut provider = MockWorkflowProvider::new();
        provider.add_workflow(
            WorkflowScope::Global,
            "test",
            RawWorkflowDef {
                name: "Test".to_string(),
                description: None,
                version: Some("1.0.0".to_string()),
                inputs: IndexMap::new(),
                outputs: IndexMap::new(),
                jobs: vec![],
            },
        );

        let result = provider.fetch(WorkflowScope::Global, "test", "1.0.0").await;
        assert!(result.is_ok());

        let result = provider
            .fetch(WorkflowScope::Global, "nonexistent", "1.0.0")
            .await;
        assert!(matches!(result, Err(WorkflowFetchError::NotFound { .. })));
    }
}
