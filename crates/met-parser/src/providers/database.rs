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
            WorkflowScope::Project => Err(WorkflowFetchError::NotFound {
                scope: "project".to_string(),
                name: name.to_string(),
            }),
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
            SELECT rw.version
            FROM reusable_workflows rw
            INNER JOIN organizations o ON o.id = rw.org_id
            WHERE rw.org_id = $1 AND rw.scope = 'global' AND rw.name = $2
              AND rw.deprecated = false
              AND rw.deleted_at IS NULL
              AND rw.submission_status = 'approved'
              AND (rw.trust_state = 'trusted' OR o.allow_untrusted_workflows)
            ORDER BY rw.created_at DESC
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
            SELECT rw.definition, rw.description
            FROM reusable_workflows rw
            INNER JOIN organizations o ON o.id = rw.org_id
            WHERE rw.org_id = $1 AND rw.scope = 'global' AND rw.name = $2 AND rw.version = $3
              AND rw.deleted_at IS NULL
              AND rw.submission_status = 'approved'
              AND (rw.trust_state = 'trusted' OR o.allow_untrusted_workflows)
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
            SELECT rw.version
            FROM reusable_workflows rw
            INNER JOIN organizations o ON o.id = rw.org_id
            WHERE rw.org_id = $1 AND rw.scope = 'global' AND rw.name = $2 AND rw.deprecated = false
              AND rw.deleted_at IS NULL
              AND rw.submission_status = 'approved'
              AND (rw.trust_state = 'trusted' OR o.allow_untrusted_workflows)
            ORDER BY rw.created_at DESC
            LIMIT 1
            "#,
        )
        .bind(self.org_id)
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| WorkflowFetchError::Network(e.to_string()))?;

        row.map(|(v,)| v)
            .ok_or_else(|| WorkflowFetchError::NotFound {
                scope: "global".to_string(),
                name: name.to_string(),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{RawJob, RawStep};
    use indexmap::IndexMap;

    /// Create a test workflow definition.
    fn test_workflow_def() -> RawWorkflowDef {
        RawWorkflowDef {
            name: "Test Workflow".to_string(),
            description: Some("A test workflow".to_string()),
            version: Some("1.0.0".to_string()),
            inputs: IndexMap::new(),
            outputs: IndexMap::new(),
            jobs: vec![RawJob {
                id: "test-job".to_string(),
                name: "Test Job".to_string(),
                runs_on: None,
                environment: None,
                steps: vec![RawStep {
                    name: "Test Step".to_string(),
                    id: Some("step1".to_string()),
                    run: Some("echo 'hello'".to_string()),
                    shell: None,
                    uses: None,
                    action_inputs: IndexMap::new(),
                    env: IndexMap::new(),
                    working_directory: None,
                    timeout: None,
                    continue_on_error: false,
                    outputs: IndexMap::new(),
                }],
                services: vec![],
                depends_on: vec![],
                condition: None,
                timeout: None,
                retry: None,
            }],
        }
    }

    #[sqlx::test(migrations = "../met-store/migrations")]
    async fn test_fetch_global_workflow(pool: PgPool) {
        let org_id = uuid::Uuid::new_v4();
        sqlx::query(
            r#"INSERT INTO organizations (id, name, slug, created_at, updated_at) VALUES ($1, 'torg', $2, NOW(), NOW())"#,
        )
        .bind(org_id)
        .bind(format!("torg-{}", org_id))
        .execute(&pool)
        .await
        .unwrap();

        // Insert a test workflow
        let workflow_def = test_workflow_def();
        let definition = serde_json::to_value(&workflow_def).unwrap();

        sqlx::query(
            r#"
            INSERT INTO reusable_workflows (id, org_id, scope, name, version, definition, deprecated, created_at)
            VALUES ($1, $2, 'global', $3, $4, $5, false, NOW())
            "#,
        )
        .bind(uuid::Uuid::new_v4())
        .bind(org_id)
        .bind("test-workflow")
        .bind("1.0.0")
        .bind(&definition)
        .execute(&pool)
        .await
        .unwrap();

        // Test the provider
        let provider = DatabaseWorkflowProvider::new(pool, org_id);

        let result = provider
            .fetch(WorkflowScope::Global, "test-workflow", "1.0.0")
            .await;
        assert!(result.is_ok(), "Expected Ok, got: {:?}", result);

        let workflow = result.unwrap();
        assert_eq!(workflow.name, "Test Workflow");
        assert_eq!(workflow.version, Some("1.0.0".to_string()));
    }

    #[sqlx::test(migrations = "../met-store/migrations")]
    async fn test_fetch_latest_version(pool: PgPool) {
        let org_id = uuid::Uuid::new_v4();
        sqlx::query(
            r#"INSERT INTO organizations (id, name, slug, created_at, updated_at) VALUES ($1, 'torg', $2, NOW(), NOW())"#,
        )
        .bind(org_id)
        .bind(format!("torg-{}", org_id))
        .execute(&pool)
        .await
        .unwrap();

        // Insert multiple versions
        for version in &["1.0.0", "1.1.0", "2.0.0"] {
            let mut workflow_def = test_workflow_def();
            workflow_def.version = Some(version.to_string());
            let definition = serde_json::to_value(&workflow_def).unwrap();

            sqlx::query(
                r#"
                INSERT INTO reusable_workflows (id, org_id, scope, name, version, definition, deprecated, created_at)
                VALUES ($1, $2, 'global', $3, $4, $5, false, NOW() + interval '1 second' * $6)
                "#,
            )
            .bind(uuid::Uuid::new_v4())
            .bind(org_id)
            .bind("versioned-workflow")
            .bind(version)
            .bind(&definition)
            .bind(version.chars().next().unwrap().to_digit(10).unwrap() as i32)
            .execute(&pool)
            .await
            .unwrap();
        }

        let provider = DatabaseWorkflowProvider::new(pool, org_id);

        // Fetch latest should return the most recently created (2.0.0)
        let result = provider
            .fetch(WorkflowScope::Global, "versioned-workflow", "latest")
            .await;
        assert!(result.is_ok());
        let workflow = result.unwrap();
        assert_eq!(workflow.version, Some("2.0.0".to_string()));
    }

    #[sqlx::test(migrations = "../met-store/migrations")]
    async fn test_fetch_nonexistent_workflow(pool: PgPool) {
        let org_id = uuid::Uuid::new_v4();
        sqlx::query(
            r#"INSERT INTO organizations (id, name, slug, created_at, updated_at) VALUES ($1, 'torg', $2, NOW(), NOW())"#,
        )
        .bind(org_id)
        .bind(format!("torg-{}", org_id))
        .execute(&pool)
        .await
        .unwrap();
        let provider = DatabaseWorkflowProvider::new(pool, org_id);

        let result = provider
            .fetch(WorkflowScope::Global, "nonexistent", "1.0.0")
            .await;
        assert!(matches!(result, Err(WorkflowFetchError::NotFound { .. })));
    }

    #[sqlx::test(migrations = "../met-store/migrations")]
    async fn test_list_versions(pool: PgPool) {
        let org_id = uuid::Uuid::new_v4();
        sqlx::query(
            r#"INSERT INTO organizations (id, name, slug, created_at, updated_at) VALUES ($1, 'torg', $2, NOW(), NOW())"#,
        )
        .bind(org_id)
        .bind(format!("torg-{}", org_id))
        .execute(&pool)
        .await
        .unwrap();

        // Insert multiple versions
        for version in &["1.0.0", "1.1.0", "2.0.0"] {
            let workflow_def = test_workflow_def();
            let definition = serde_json::to_value(&workflow_def).unwrap();

            sqlx::query(
                r#"
                INSERT INTO reusable_workflows (id, org_id, scope, name, version, definition, deprecated, created_at)
                VALUES ($1, $2, 'global', $3, $4, $5, false, NOW())
                "#,
            )
            .bind(uuid::Uuid::new_v4())
            .bind(org_id)
            .bind("multi-version")
            .bind(version)
            .bind(&definition)
            .execute(&pool)
            .await
            .unwrap();
        }

        let provider = DatabaseWorkflowProvider::new(pool, org_id);

        let versions = provider
            .list_versions(WorkflowScope::Global, "multi-version")
            .await
            .unwrap();
        assert_eq!(versions.len(), 3);
        assert!(versions.contains(&"1.0.0".to_string()));
        assert!(versions.contains(&"1.1.0".to_string()));
        assert!(versions.contains(&"2.0.0".to_string()));
    }

    #[sqlx::test(migrations = "../met-store/migrations")]
    async fn test_deprecated_workflow_excluded(pool: PgPool) {
        let org_id = uuid::Uuid::new_v4();
        sqlx::query(
            r#"INSERT INTO organizations (id, name, slug, created_at, updated_at) VALUES ($1, 'torg', $2, NOW(), NOW())"#,
        )
        .bind(org_id)
        .bind(format!("torg-{}", org_id))
        .execute(&pool)
        .await
        .unwrap();

        // Insert a deprecated workflow
        let workflow_def = test_workflow_def();
        let definition = serde_json::to_value(&workflow_def).unwrap();

        sqlx::query(
            r#"
            INSERT INTO reusable_workflows (id, org_id, scope, name, version, definition, deprecated, created_at)
            VALUES ($1, $2, 'global', $3, $4, $5, true, NOW())
            "#,
        )
        .bind(uuid::Uuid::new_v4())
        .bind(org_id)
        .bind("deprecated-workflow")
        .bind("1.0.0")
        .bind(&definition)
        .execute(&pool)
        .await
        .unwrap();

        let provider = DatabaseWorkflowProvider::new(pool, org_id);

        // list_versions should not include deprecated workflows
        let versions = provider
            .list_versions(WorkflowScope::Global, "deprecated-workflow")
            .await
            .unwrap();
        assert!(versions.is_empty());

        // fetch latest should fail for deprecated-only workflows
        let result = provider
            .fetch(WorkflowScope::Global, "deprecated-workflow", "latest")
            .await;
        assert!(matches!(result, Err(WorkflowFetchError::NotFound { .. })));
    }
}
