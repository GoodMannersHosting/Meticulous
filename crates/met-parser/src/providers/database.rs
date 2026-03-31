//! Database workflow provider for global workflows.
//!
//! Fetches reusable workflow definitions from PostgreSQL using met-store.

use async_trait::async_trait;
use sqlx::PgPool;
use tracing::{debug, instrument};

use crate::ir::WorkflowScope;
use crate::schema::RawWorkflowDef;
use crate::semver::{parse_version_constraint, resolve_version};
use crate::workflow::{WorkflowFetchError, WorkflowProvider};

/// Database-backed workflow provider for global workflows.
///
/// This provider fetches workflow definitions from the `reusable_workflows` table
/// in the database. It supports semver version resolution for workflow references.
pub struct DatabaseWorkflowProvider {
    pool: PgPool,
    org_id: uuid::Uuid,
}

impl DatabaseWorkflowProvider {
    /// Create a new database workflow provider.
    pub fn new(pool: PgPool, org_id: uuid::Uuid) -> Self {
        Self { pool, org_id }
    }
}

#[async_trait]
impl WorkflowProvider for DatabaseWorkflowProvider {
    #[instrument(skip(self), fields(scope = ?scope, name = %name, version = %version))]
    async fn fetch(
        &self,
        scope: WorkflowScope,
        name: &str,
        version: &str,
    ) -> Result<RawWorkflowDef, WorkflowFetchError> {
        match scope {
            WorkflowScope::Global => self.fetch_global(name, version).await,
            WorkflowScope::Project => {
                Err(WorkflowFetchError::NotFound {
                    scope: "project".to_string(),
                    name: name.to_string(),
                })
            }
        }
    }

    #[instrument(skip(self), fields(scope = ?scope, name = %name))]
    async fn list_versions(
        &self,
        scope: WorkflowScope,
        name: &str,
    ) -> Result<Vec<String>, WorkflowFetchError> {
        if scope != WorkflowScope::Global {
            return Ok(Vec::new());
        }

        let versions: Vec<(String,)> = sqlx::query_as(
            r#"
            SELECT version
            FROM reusable_workflows
            WHERE org_id = $1 AND scope = 'global' AND name = $2 AND deprecated = false
            ORDER BY created_at DESC
            "#,
        )
        .bind(self.org_id)
        .bind(name)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| WorkflowFetchError::Network(e.to_string()))?;

        Ok(versions.into_iter().map(|(v,)| v).collect())
    }
}

impl DatabaseWorkflowProvider {
    async fn fetch_global(
        &self,
        name: &str,
        version: &str,
    ) -> Result<RawWorkflowDef, WorkflowFetchError> {
        let resolved_version = if version == "latest" {
            self.get_latest_version(name).await?
        } else if let Ok(constraint) = parse_version_constraint(version) {
            let versions = self.list_versions(WorkflowScope::Global, name).await?;
            resolve_version(&constraint, &versions).ok_or_else(|| {
                WorkflowFetchError::VersionNotFound {
                    scope: "global".to_string(),
                    name: name.to_string(),
                    version: version.to_string(),
                }
            })?
        } else {
            version.to_string()
        };

        debug!(name, version = %resolved_version, "fetching workflow from database");

        let row: Option<(serde_json::Value, Option<String>)> = sqlx::query_as(
            r#"
            SELECT definition, description
            FROM reusable_workflows
            WHERE org_id = $1 AND scope = 'global' AND name = $2 AND version = $3
            "#,
        )
        .bind(self.org_id)
        .bind(name)
        .bind(&resolved_version)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| WorkflowFetchError::Network(e.to_string()))?;

        match row {
            Some((definition, description)) => {
                let mut workflow: RawWorkflowDef = serde_json::from_value(definition)
                    .map_err(|e| WorkflowFetchError::Parse(e.to_string()))?;
                
                workflow.version = Some(resolved_version);
                if workflow.description.is_none() {
                    workflow.description = description;
                }
                
                Ok(workflow)
            }
            None => Err(WorkflowFetchError::NotFound {
                scope: "global".to_string(),
                name: name.to_string(),
            }),
        }
    }

    async fn get_latest_version(&self, name: &str) -> Result<String, WorkflowFetchError> {
        let row: Option<(String,)> = sqlx::query_as(
            r#"
            SELECT version
            FROM reusable_workflows
            WHERE org_id = $1 AND scope = 'global' AND name = $2 AND deprecated = false
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(self.org_id)
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| WorkflowFetchError::Network(e.to_string()))?;

        row.map(|(v,)| v).ok_or_else(|| WorkflowFetchError::NotFound {
            scope: "global".to_string(),
            name: name.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_creation() {
        let org_id = uuid::Uuid::new_v4();
        let _provider = DatabaseWorkflowProvider {
            pool: todo!("need test pool"),
            org_id,
        };
    }
}
