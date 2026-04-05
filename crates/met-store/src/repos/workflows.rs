//! Workflow repository for reusable workflow definitions.

use chrono::{DateTime, Utc};
use met_core::ids::{OrganizationId, ProjectId};
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{Result, StoreError};

/// Workflow scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type)]
#[sqlx(type_name = "workflow_scope", rename_all = "lowercase")]
pub enum WorkflowScope {
    Global,
    Project,
}

/// Reusable workflow definition.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ReusableWorkflow {
    pub id: Uuid,
    pub org_id: Uuid,
    pub project_id: Option<Uuid>,
    pub scope: WorkflowScope,
    pub name: String,
    pub version: String,
    pub definition: serde_json::Value,
    pub description: Option<String>,
    pub deprecated: bool,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Input for creating a workflow.
#[derive(Debug)]
pub struct CreateWorkflow {
    pub name: String,
    pub version: String,
    pub definition: serde_json::Value,
    pub description: Option<String>,
    pub tags: Vec<String>,
}

/// Repository for workflow operations.
pub struct WorkflowRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> WorkflowRepo<'a> {
    /// Create a new workflow repository.
    #[must_use]
    pub const fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Create a global workflow.
    pub async fn create_global(
        &self,
        org_id: OrganizationId,
        input: &CreateWorkflow,
    ) -> Result<ReusableWorkflow> {
        let id = Uuid::new_v4();
        let now = Utc::now();

        let workflow = sqlx::query_as::<_, ReusableWorkflow>(
            r#"
            INSERT INTO reusable_workflows (id, org_id, scope, name, version, definition, description, tags, created_at, updated_at)
            VALUES ($1, $2, 'global', $3, $4, $5, $6, $7, $8, $8)
            RETURNING id, org_id, project_id, scope, name, version, definition, description, deprecated, tags, created_at, updated_at
            "#,
        )
        .bind(id)
        .bind(org_id.as_uuid())
        .bind(&input.name)
        .bind(&input.version)
        .bind(&input.definition)
        .bind(&input.description)
        .bind(&input.tags)
        .bind(now)
        .fetch_one(self.pool)
        .await?;

        Ok(workflow)
    }

    /// Create a project workflow.
    pub async fn create_project(
        &self,
        org_id: OrganizationId,
        project_id: ProjectId,
        input: &CreateWorkflow,
    ) -> Result<ReusableWorkflow> {
        let id = Uuid::new_v4();
        let now = Utc::now();

        let workflow = sqlx::query_as::<_, ReusableWorkflow>(
            r#"
            INSERT INTO reusable_workflows (id, org_id, project_id, scope, name, version, definition, description, tags, created_at, updated_at)
            VALUES ($1, $2, $3, 'project', $4, $5, $6, $7, $8, $9, $9)
            RETURNING id, org_id, project_id, scope, name, version, definition, description, deprecated, tags, created_at, updated_at
            "#,
        )
        .bind(id)
        .bind(org_id.as_uuid())
        .bind(project_id.as_uuid())
        .bind(&input.name)
        .bind(&input.version)
        .bind(&input.definition)
        .bind(&input.description)
        .bind(&input.tags)
        .bind(now)
        .fetch_one(self.pool)
        .await?;

        Ok(workflow)
    }

    /// Insert or update a project-scoped workflow (same `org_id`, `project_id`, `name`, `version`).
    pub async fn upsert_project(
        &self,
        org_id: OrganizationId,
        project_id: ProjectId,
        input: &CreateWorkflow,
    ) -> Result<ReusableWorkflow> {
        let id = Uuid::new_v4();
        let now = Utc::now();

        let workflow = sqlx::query_as::<_, ReusableWorkflow>(
            r#"
            INSERT INTO reusable_workflows (id, org_id, project_id, scope, name, version, definition, description, tags, created_at, updated_at)
            VALUES ($1, $2, $3, 'project', $4, $5, $6, $7, $8, $9, $9)
            ON CONFLICT (org_id, project_id, name, version) DO UPDATE
            SET definition = EXCLUDED.definition,
                description = COALESCE(EXCLUDED.description, reusable_workflows.description),
                tags = EXCLUDED.tags,
                deprecated = false,
                updated_at = EXCLUDED.updated_at
            RETURNING id, org_id, project_id, scope, name, version, definition, description, deprecated, tags, created_at, updated_at
            "#,
        )
        .bind(id)
        .bind(org_id.as_uuid())
        .bind(project_id.as_uuid())
        .bind(&input.name)
        .bind(&input.version)
        .bind(&input.definition)
        .bind(&input.description)
        .bind(&input.tags)
        .bind(now)
        .fetch_one(self.pool)
        .await?;

        Ok(workflow)
    }

    /// Get a workflow by scope, name, and version.
    pub async fn get(
        &self,
        org_id: OrganizationId,
        project_id: Option<ProjectId>,
        scope: WorkflowScope,
        name: &str,
        version: &str,
    ) -> Result<ReusableWorkflow> {
        let workflow = match scope {
            WorkflowScope::Global => {
                sqlx::query_as::<_, ReusableWorkflow>(
                    r#"
                    SELECT id, org_id, project_id, scope, name, version, definition, description, deprecated, tags, created_at, updated_at
                    FROM reusable_workflows
                    WHERE org_id = $1 AND scope = 'global' AND name = $2 AND version = $3
                    "#,
                )
                .bind(org_id.as_uuid())
                .bind(name)
                .bind(version)
                .fetch_optional(self.pool)
                .await?
            }
            WorkflowScope::Project => {
                let pid = project_id.ok_or_else(|| StoreError::not_found("workflow", name))?;
                sqlx::query_as::<_, ReusableWorkflow>(
                    r#"
                    SELECT id, org_id, project_id, scope, name, version, definition, description, deprecated, tags, created_at, updated_at
                    FROM reusable_workflows
                    WHERE org_id = $1 AND project_id = $2 AND scope = 'project' AND name = $3 AND version = $4
                    "#,
                )
                .bind(org_id.as_uuid())
                .bind(pid.as_uuid())
                .bind(name)
                .bind(version)
                .fetch_optional(self.pool)
                .await?
            }
        };

        workflow.ok_or_else(|| StoreError::not_found("workflow", format!("{}/{}", name, version)))
    }

    /// Get the latest version of a workflow.
    pub async fn get_latest(
        &self,
        org_id: OrganizationId,
        project_id: Option<ProjectId>,
        scope: WorkflowScope,
        name: &str,
    ) -> Result<ReusableWorkflow> {
        let workflow = match scope {
            WorkflowScope::Global => {
                sqlx::query_as::<_, ReusableWorkflow>(
                    r#"
                    SELECT id, org_id, project_id, scope, name, version, definition, description, deprecated, tags, created_at, updated_at
                    FROM reusable_workflows
                    WHERE org_id = $1 AND scope = 'global' AND name = $2 AND deprecated = false
                    ORDER BY created_at DESC
                    LIMIT 1
                    "#,
                )
                .bind(org_id.as_uuid())
                .bind(name)
                .fetch_optional(self.pool)
                .await?
            }
            WorkflowScope::Project => {
                let pid = project_id.ok_or_else(|| StoreError::not_found("workflow", name))?;
                sqlx::query_as::<_, ReusableWorkflow>(
                    r#"
                    SELECT id, org_id, project_id, scope, name, version, definition, description, deprecated, tags, created_at, updated_at
                    FROM reusable_workflows
                    WHERE org_id = $1 AND project_id = $2 AND scope = 'project' AND name = $3 AND deprecated = false
                    ORDER BY created_at DESC
                    LIMIT 1
                    "#,
                )
                .bind(org_id.as_uuid())
                .bind(pid.as_uuid())
                .bind(name)
                .fetch_optional(self.pool)
                .await?
            }
        };

        workflow.ok_or_else(|| StoreError::not_found("workflow", name))
    }

    /// List all versions of a workflow.
    pub async fn list_versions(
        &self,
        org_id: OrganizationId,
        project_id: Option<ProjectId>,
        scope: WorkflowScope,
        name: &str,
    ) -> Result<Vec<String>> {
        let versions: Vec<(String,)> = match scope {
            WorkflowScope::Global => {
                sqlx::query_as(
                    r#"
                    SELECT version
                    FROM reusable_workflows
                    WHERE org_id = $1 AND scope = 'global' AND name = $2
                    ORDER BY created_at DESC
                    "#,
                )
                .bind(org_id.as_uuid())
                .bind(name)
                .fetch_all(self.pool)
                .await?
            }
            WorkflowScope::Project => {
                let pid = project_id.ok_or_else(|| StoreError::not_found("workflow", name))?;
                sqlx::query_as(
                    r#"
                    SELECT version
                    FROM reusable_workflows
                    WHERE org_id = $1 AND project_id = $2 AND scope = 'project' AND name = $3
                    ORDER BY created_at DESC
                    "#,
                )
                .bind(org_id.as_uuid())
                .bind(pid.as_uuid())
                .bind(name)
                .fetch_all(self.pool)
                .await?
            }
        };

        Ok(versions.into_iter().map(|(v,)| v).collect())
    }

    /// List global workflows.
    pub async fn list_global(
        &self,
        org_id: OrganizationId,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ReusableWorkflow>> {
        let workflows = sqlx::query_as::<_, ReusableWorkflow>(
            r#"
            SELECT DISTINCT ON (name) id, org_id, project_id, scope, name, version, definition, description, deprecated, tags, created_at, updated_at
            FROM reusable_workflows
            WHERE org_id = $1 AND scope = 'global' AND deprecated = false
            ORDER BY name, created_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(org_id.as_uuid())
        .bind(limit)
        .bind(offset)
        .fetch_all(self.pool)
        .await?;

        Ok(workflows)
    }

    /// List project workflows.
    pub async fn list_project(
        &self,
        project_id: ProjectId,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ReusableWorkflow>> {
        let workflows = sqlx::query_as::<_, ReusableWorkflow>(
            r#"
            SELECT DISTINCT ON (name) id, org_id, project_id, scope, name, version, definition, description, deprecated, tags, created_at, updated_at
            FROM reusable_workflows
            WHERE project_id = $1 AND scope = 'project' AND deprecated = false
            ORDER BY name, created_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(project_id.as_uuid())
        .bind(limit)
        .bind(offset)
        .fetch_all(self.pool)
        .await?;

        Ok(workflows)
    }

    /// Mark a workflow version as deprecated.
    pub async fn deprecate(
        &self,
        org_id: OrganizationId,
        project_id: Option<ProjectId>,
        scope: WorkflowScope,
        name: &str,
        version: &str,
    ) -> Result<()> {
        match scope {
            WorkflowScope::Global => {
                sqlx::query(
                    r#"
                    UPDATE reusable_workflows
                    SET deprecated = true, updated_at = NOW()
                    WHERE org_id = $1 AND scope = 'global' AND name = $2 AND version = $3
                    "#,
                )
                .bind(org_id.as_uuid())
                .bind(name)
                .bind(version)
                .execute(self.pool)
                .await?;
            }
            WorkflowScope::Project => {
                let pid = project_id.ok_or_else(|| StoreError::not_found("workflow", name))?;
                sqlx::query(
                    r#"
                    UPDATE reusable_workflows
                    SET deprecated = true, updated_at = NOW()
                    WHERE org_id = $1 AND project_id = $2 AND scope = 'project' AND name = $3 AND version = $4
                    "#,
                )
                .bind(org_id.as_uuid())
                .bind(pid.as_uuid())
                .bind(name)
                .bind(version)
                .execute(self.pool)
                .await?;
            }
        }

        Ok(())
    }
}
