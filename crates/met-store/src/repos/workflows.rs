//! Workflow repository for reusable workflow definitions.

use chrono::{DateTime, Utc};
use met_core::ids::{OrganizationId, ProjectId, UserId};
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

/// How the workflow row was created or last updated from a lineage perspective.
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type)]
#[sqlx(type_name = "workflow_source", rename_all = "snake_case")]
pub enum WorkflowSource {
    Git,
    Api,
    ProjectSync,
}

/// Admin review state for catalog (global) workflows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type)]
#[sqlx(type_name = "workflow_submission_status", rename_all = "lowercase")]
pub enum WorkflowSubmissionStatus {
    Pending,
    Approved,
    Rejected,
}

/// Trust tier for catalog policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type)]
#[sqlx(type_name = "workflow_trust_state", rename_all = "lowercase")]
pub enum WorkflowTrustState {
    Trusted,
    Untrusted,
}

/// Which versions are visible for `list_versions` on global workflows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowVersionListMode {
    /// Approved, not deleted, not deprecated; respects trust only when listing for semver resolution (versions must be executable).
    Execution,
    /// Any non-deleted row for catalog / diagnostics (includes pending and untrusted).
    Catalog,
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
    pub source: WorkflowSource,
    pub scm_repository: Option<String>,
    pub scm_ref: Option<String>,
    pub scm_path: Option<String>,
    pub scm_revision: Option<String>,
    pub submission_status: WorkflowSubmissionStatus,
    pub trust_state: WorkflowTrustState,
    pub submitted_by: Option<Uuid>,
    pub reviewed_by: Option<Uuid>,
    pub reviewed_at: Option<DateTime<Utc>>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub catalog_metadata: serde_json::Value,
    /// Date/time after which pipelines using this version are hard-blocked.
    /// Before this date a warning diagnostic is emitted.
    pub deprecated_after: Option<DateTime<Utc>>,
    /// Human-readable markdown note explaining the deprecation reason.
    pub deprecation_note: Option<String>,
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

/// New global row from a Git-backed catalog import.
#[derive(Debug)]
pub struct CreateGlobalCatalogGit {
    pub name: String,
    pub version: String,
    pub definition: serde_json::Value,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub scm_repository: String,
    pub scm_ref: String,
    pub scm_path: String,
    pub scm_revision: String,
    pub catalog_metadata: serde_json::Value,
    pub submitted_by: UserId,
}

const WF_SELECT: &str = r#"
    id, org_id, project_id, scope, name, version, definition, description, deprecated, tags, created_at, updated_at,
    source, scm_repository, scm_ref, scm_path, scm_revision, submission_status, trust_state,
    submitted_by, reviewed_by, reviewed_at, deleted_at, catalog_metadata,
    deprecated_after, deprecation_note
"#;

const WF_RW: &str = r#"
    rw.id, rw.org_id, rw.project_id, rw.scope, rw.name, rw.version, rw.definition, rw.description, rw.deprecated, rw.tags, rw.created_at, rw.updated_at,
    rw.source, rw.scm_repository, rw.scm_ref, rw.scm_path, rw.scm_revision, rw.submission_status, rw.trust_state,
    rw.submitted_by, rw.reviewed_by, rw.reviewed_at, rw.deleted_at, rw.catalog_metadata,
    rw.deprecated_after, rw.deprecation_note
"#;

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

    /// Create a global workflow (direct API; approved + trusted).
    pub async fn create_global(
        &self,
        org_id: OrganizationId,
        input: &CreateWorkflow,
    ) -> Result<ReusableWorkflow> {
        let id = Uuid::new_v4();
        let now = Utc::now();

        let workflow = sqlx::query_as::<_, ReusableWorkflow>(&format!(
            r#"
            INSERT INTO reusable_workflows (
                id, org_id, scope, name, version, definition, description, tags, created_at, updated_at,
                source, submission_status, trust_state, catalog_metadata
            )
            VALUES ($1, $2, 'global', $3, $4, $5, $6, $7, $8, $8, 'api', 'approved', 'trusted', '{{}}'::jsonb)
            RETURNING {WF_SELECT}
            "#
        ))
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

        let workflow = sqlx::query_as::<_, ReusableWorkflow>(&format!(
            r#"
            INSERT INTO reusable_workflows (
                id, org_id, project_id, scope, name, version, definition, description, tags, created_at, updated_at,
                source, submission_status, trust_state, catalog_metadata
            )
            VALUES ($1, $2, $3, 'project', $4, $5, $6, $7, $8, $9, $9, 'api', 'approved', 'trusted', '{{}}'::jsonb)
            RETURNING {WF_SELECT}
            "#
        ))
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

        let workflow = sqlx::query_as::<_, ReusableWorkflow>(&format!(
            r#"
            INSERT INTO reusable_workflows (
                id, org_id, project_id, scope, name, version, definition, description, tags, created_at, updated_at,
                source, submission_status, trust_state, catalog_metadata
            )
            VALUES ($1, $2, $3, 'project', $4, $5, $6, $7, $8, $9, $9, 'project_sync', 'approved', 'trusted', '{{}}'::jsonb)
            ON CONFLICT (org_id, project_id, name, version) DO UPDATE
            SET definition = EXCLUDED.definition,
                description = COALESCE(EXCLUDED.description, reusable_workflows.description),
                tags = EXCLUDED.tags,
                deprecated = false,
                source = 'project_sync',
                submission_status = 'approved',
                trust_state = 'trusted',
                deleted_at = NULL,
                updated_at = EXCLUDED.updated_at
            RETURNING {WF_SELECT}
            "#
        ))
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

    /// Global catalog import: pending review, untrusted by default for **new** rows.
    ///
    /// On conflict (same org, name, version), content and SCM pointers are refreshed. If the
    /// existing row was already **`approved`**, submission status, trust, reviewer fields, and
    /// submitter are left unchanged so scheduled sync and bulk re-import do not queue it again.
    pub async fn create_global_catalog_git(
        &self,
        org_id: OrganizationId,
        input: &CreateGlobalCatalogGit,
    ) -> Result<ReusableWorkflow> {
        let id = Uuid::new_v4();
        let now = Utc::now();

        let q = format!(
            r#"
            INSERT INTO reusable_workflows (
                id, org_id, scope, name, version, definition, description, tags, created_at, updated_at,
                source, scm_repository, scm_ref, scm_path, scm_revision,
                submission_status, trust_state, catalog_metadata, submitted_by
            )
            VALUES ($1, $2, 'global', $3, $4, $5, $6, $7, $8, $8, 'git', $9, $10, $11, $12, 'pending', 'untrusted', $13, $14)
            ON CONFLICT (org_id, name, version)
                WHERE scope = 'global' AND project_id IS NULL
            DO UPDATE SET
                definition = EXCLUDED.definition,
                description = COALESCE(EXCLUDED.description, reusable_workflows.description),
                tags = EXCLUDED.tags,
                scm_repository = EXCLUDED.scm_repository,
                scm_ref = EXCLUDED.scm_ref,
                scm_path = EXCLUDED.scm_path,
                scm_revision = EXCLUDED.scm_revision,
                catalog_metadata = EXCLUDED.catalog_metadata,
                submission_status = CASE
                    WHEN reusable_workflows.submission_status = 'approved' THEN reusable_workflows.submission_status
                    ELSE 'pending'
                END,
                trust_state = CASE
                    WHEN reusable_workflows.submission_status = 'approved' THEN reusable_workflows.trust_state
                    ELSE 'untrusted'
                END,
                submitted_by = CASE
                    WHEN reusable_workflows.submission_status = 'approved' THEN reusable_workflows.submitted_by
                    ELSE EXCLUDED.submitted_by
                END,
                reviewed_by = CASE
                    WHEN reusable_workflows.submission_status = 'approved' THEN reusable_workflows.reviewed_by
                    ELSE NULL
                END,
                reviewed_at = CASE
                    WHEN reusable_workflows.submission_status = 'approved' THEN reusable_workflows.reviewed_at
                    ELSE NULL
                END,
                deleted_at = NULL,
                deprecated = false,
                updated_at = EXCLUDED.updated_at
            RETURNING {WF_SELECT}
            "#
        );

        let workflow = sqlx::query_as::<_, ReusableWorkflow>(&q)
            .bind(id)
            .bind(org_id.as_uuid())
            .bind(&input.name)
            .bind(&input.version)
            .bind(&input.definition)
            .bind(&input.description)
            .bind(&input.tags)
            .bind(now)
            .bind(&input.scm_repository)
            .bind(&input.scm_ref)
            .bind(&input.scm_path)
            .bind(&input.scm_revision)
            .bind(&input.catalog_metadata)
            .bind(input.submitted_by.as_uuid())
            .fetch_one(self.pool)
            .await?;

        Ok(workflow)
    }

    /// Get by primary key within org (includes deleted and non-approved — for API detail).
    pub async fn get_by_id(
        &self,
        org_id: OrganizationId,
        workflow_id: Uuid,
    ) -> Result<ReusableWorkflow> {
        sqlx::query_as::<_, ReusableWorkflow>(&format!(
            r#"SELECT {WF_SELECT} FROM reusable_workflows WHERE id = $1 AND org_id = $2"#
        ))
        .bind(workflow_id)
        .bind(org_id.as_uuid())
        .fetch_optional(self.pool)
        .await?
        .ok_or_else(|| StoreError::not_found("workflow", workflow_id))
    }

    /// Get a workflow by scope, name, and version (execution path: global rows are gated).
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
                sqlx::query_as::<_, ReusableWorkflow>(&format!(
                    r#"
                    SELECT {WF_RW}
                    FROM reusable_workflows rw
                    INNER JOIN organizations o ON o.id = rw.org_id
                    WHERE rw.org_id = $1 AND rw.scope = 'global' AND rw.name = $2 AND rw.version = $3
                      AND rw.deleted_at IS NULL
                      AND rw.submission_status = 'approved'
                      AND (rw.trust_state = 'trusted' OR o.allow_untrusted_workflows)
                    "#
                ))
                .bind(org_id.as_uuid())
                .bind(name)
                .bind(version)
                .fetch_optional(self.pool)
                .await?
            }
            WorkflowScope::Project => {
                let pid = project_id.ok_or_else(|| StoreError::not_found("workflow", name))?;
                sqlx::query_as::<_, ReusableWorkflow>(&format!(
                    r#"SELECT {WF_SELECT} FROM reusable_workflows
                    WHERE org_id = $1 AND project_id = $2 AND scope = 'project' AND name = $3 AND version = $4
                      AND deleted_at IS NULL"#
                ))
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

    /// Latest **executable** global version, or latest project version.
    pub async fn get_latest(
        &self,
        org_id: OrganizationId,
        project_id: Option<ProjectId>,
        scope: WorkflowScope,
        name: &str,
    ) -> Result<ReusableWorkflow> {
        let workflow = match scope {
            WorkflowScope::Global => {
                sqlx::query_as::<_, ReusableWorkflow>(&format!(
                    r#"
                    SELECT {WF_RW}
                    FROM reusable_workflows rw
                    INNER JOIN organizations o ON o.id = rw.org_id
                    WHERE rw.org_id = $1 AND rw.scope = 'global' AND rw.name = $2
                      AND rw.deprecated = false
                      AND rw.deleted_at IS NULL
                      AND rw.submission_status = 'approved'
                      AND (rw.trust_state = 'trusted' OR o.allow_untrusted_workflows)
                    ORDER BY rw.created_at DESC
                    LIMIT 1
                    "#
                ))
                .bind(org_id.as_uuid())
                .bind(name)
                .fetch_optional(self.pool)
                .await?
            }
            WorkflowScope::Project => {
                let pid = project_id.ok_or_else(|| StoreError::not_found("workflow", name))?;
                sqlx::query_as::<_, ReusableWorkflow>(&format!(
                    r#"
                    SELECT {WF_SELECT} FROM reusable_workflows
                    WHERE org_id = $1 AND project_id = $2 AND scope = 'project' AND name = $3
                      AND deprecated = false AND deleted_at IS NULL
                    ORDER BY created_at DESC
                    LIMIT 1
                    "#
                ))
                .bind(org_id.as_uuid())
                .bind(pid.as_uuid())
                .bind(name)
                .fetch_optional(self.pool)
                .await?
            }
        };

        workflow.ok_or_else(|| StoreError::not_found("workflow", name))
    }

    /// Row for `name` + `version` without execution gating (not deleted). For diagnostics.
    pub async fn get_global_row_any_status(
        &self,
        org_id: OrganizationId,
        name: &str,
        version: &str,
    ) -> Result<ReusableWorkflow> {
        sqlx::query_as::<_, ReusableWorkflow>(&format!(
            r#"
            SELECT {WF_SELECT} FROM reusable_workflows
            WHERE org_id = $1 AND scope = 'global' AND project_id IS NULL AND name = $2 AND version = $3
              AND deleted_at IS NULL
            "#
        ))
        .bind(org_id.as_uuid())
        .bind(name)
        .bind(version)
        .fetch_optional(self.pool)
        .await?
        .ok_or_else(|| StoreError::not_found("workflow", format!("{}/{}", name, version)))
    }

    /// Deleted global row for version (soft-delete tombstone).
    pub async fn get_global_deleted_row(
        &self,
        org_id: OrganizationId,
        name: &str,
        version: &str,
    ) -> Result<Option<ReusableWorkflow>> {
        let row = sqlx::query_as::<_, ReusableWorkflow>(&format!(
            r#"
            SELECT {WF_SELECT} FROM reusable_workflows
            WHERE org_id = $1 AND scope = 'global' AND project_id IS NULL AND name = $2 AND version = $3
              AND deleted_at IS NOT NULL
            LIMIT 1
            "#
        ))
        .bind(org_id.as_uuid())
        .bind(name)
        .bind(version)
        .fetch_optional(self.pool)
        .await?;
        Ok(row)
    }

    fn global_version_where(mode: WorkflowVersionListMode) -> &'static str {
        match mode {
            WorkflowVersionListMode::Execution => {
                "rw.deleted_at IS NULL AND rw.deprecated = false AND rw.submission_status = 'approved'
                 AND (rw.trust_state = 'trusted' OR o.allow_untrusted_workflows)"
            }
            WorkflowVersionListMode::Catalog => "rw.deleted_at IS NULL",
        }
    }

    /// List all versions of a workflow.
    pub async fn list_versions(
        &self,
        org_id: OrganizationId,
        project_id: Option<ProjectId>,
        scope: WorkflowScope,
        name: &str,
        mode: WorkflowVersionListMode,
    ) -> Result<Vec<String>> {
        let versions: Vec<(String,)> = match scope {
            WorkflowScope::Global => {
                match mode {
                    WorkflowVersionListMode::Execution => {
                        let cond = Self::global_version_where(mode);
                        let sql = format!(
                            r#"
                            SELECT rw.version
                            FROM reusable_workflows rw
                            INNER JOIN organizations o ON o.id = rw.org_id
                            WHERE rw.org_id = $1 AND rw.scope = 'global' AND rw.name = $2 AND {cond}
                            ORDER BY rw.created_at DESC
                            "#
                        );
                        sqlx::query_as(&sql)
                            .bind(org_id.as_uuid())
                            .bind(name)
                            .fetch_all(self.pool)
                            .await?
                    }
                    WorkflowVersionListMode::Catalog => {
                        sqlx::query_as(
                            r#"
                            SELECT version
                            FROM reusable_workflows
                            WHERE org_id = $1 AND scope = 'global' AND project_id IS NULL AND name = $2
                              AND deleted_at IS NULL
                            ORDER BY created_at DESC
                            "#,
                        )
                        .bind(org_id.as_uuid())
                        .bind(name)
                        .fetch_all(self.pool)
                        .await?
                    }
                }
            }
            WorkflowScope::Project => {
                let pid = project_id.ok_or_else(|| StoreError::not_found("workflow", name))?;
                sqlx::query_as(
                    r#"
                    SELECT version
                    FROM reusable_workflows
                    WHERE org_id = $1 AND project_id = $2 AND scope = 'project' AND name = $3
                      AND deleted_at IS NULL
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

    /// Paginated global catalog rows for one workflow name (search on version, scm_revision, description).
    pub async fn list_global_catalog_versions(
        &self,
        org_id: OrganizationId,
        name: &str,
        q: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ReusableWorkflow>> {
        let needle = q.map(|s| format!("%{}%", s.replace('%', "\\%").replace('_', "\\_")));
        let rows = if let Some(ref pattern) = needle {
            sqlx::query_as::<_, ReusableWorkflow>(&format!(
                r#"
                SELECT {WF_SELECT} FROM reusable_workflows
                WHERE org_id = $1 AND scope = 'global' AND project_id IS NULL AND name = $2
                  AND deleted_at IS NULL
                  AND (
                    version ILIKE $3 ESCAPE '\'
                    OR COALESCE(scm_revision, '') ILIKE $3 ESCAPE '\'
                    OR COALESCE(description, '') ILIKE $3 ESCAPE '\'
                  )
                ORDER BY created_at DESC
                LIMIT $4 OFFSET $5
                "#
            ))
            .bind(org_id.as_uuid())
            .bind(name)
            .bind(pattern)
            .bind(limit)
            .bind(offset)
            .fetch_all(self.pool)
            .await?
        } else {
            sqlx::query_as::<_, ReusableWorkflow>(&format!(
                r#"
                SELECT {WF_SELECT} FROM reusable_workflows
                WHERE org_id = $1 AND scope = 'global' AND project_id IS NULL AND name = $2
                  AND deleted_at IS NULL
                ORDER BY created_at DESC
                LIMIT $3 OFFSET $4
                "#
            ))
            .bind(org_id.as_uuid())
            .bind(name)
            .bind(limit)
            .bind(offset)
            .fetch_all(self.pool)
            .await?
        };

        Ok(rows)
    }

    /// Global catalog list: latest row per workflow `name` (non-deleted).
    pub async fn list_global_catalog(
        &self,
        org_id: OrganizationId,
        limit: i64,
        offset: i64,
        submission_status: Option<WorkflowSubmissionStatus>,
    ) -> Result<Vec<ReusableWorkflow>> {
        let rows = if let Some(st) = submission_status {
            sqlx::query_as::<_, ReusableWorkflow>(&format!(
                r#"
                SELECT DISTINCT ON (name) {WF_SELECT}
                FROM reusable_workflows
                WHERE org_id = $1 AND scope = 'global' AND project_id IS NULL AND deleted_at IS NULL
                  AND submission_status = $2
                ORDER BY name, created_at DESC
                LIMIT $3 OFFSET $4
                "#
            ))
            .bind(org_id.as_uuid())
            .bind(st)
            .bind(limit)
            .bind(offset)
            .fetch_all(self.pool)
            .await?
        } else {
            sqlx::query_as::<_, ReusableWorkflow>(&format!(
                r#"
                SELECT DISTINCT ON (name) {WF_SELECT}
                FROM reusable_workflows
                WHERE org_id = $1 AND scope = 'global' AND project_id IS NULL AND deleted_at IS NULL
                ORDER BY name, created_at DESC
                LIMIT $2 OFFSET $3
                "#
            ))
            .bind(org_id.as_uuid())
            .bind(limit)
            .bind(offset)
            .fetch_all(self.pool)
            .await?
        };

        Ok(rows)
    }

    /// List global workflows for **execution discovery** (legacy `/workflows/global`): latest per name, gated like fetch.
    pub async fn list_global(
        &self,
        org_id: OrganizationId,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ReusableWorkflow>> {
        let workflows = sqlx::query_as::<_, ReusableWorkflow>(&format!(
            r#"
            SELECT DISTINCT ON (rw.name) {WF_RW}
            FROM reusable_workflows rw
            INNER JOIN organizations o ON o.id = rw.org_id
            WHERE rw.org_id = $1 AND rw.scope = 'global' AND rw.project_id IS NULL
              AND rw.deleted_at IS NULL
              AND rw.deprecated = false
              AND rw.submission_status = 'approved'
              AND (rw.trust_state = 'trusted' OR o.allow_untrusted_workflows)
            ORDER BY rw.name, rw.created_at DESC
            LIMIT $2 OFFSET $3
            "#
        ))
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
        let workflows = sqlx::query_as::<_, ReusableWorkflow>(&format!(
            r#"
            SELECT DISTINCT ON (name) {WF_SELECT}
            FROM reusable_workflows
            WHERE project_id = $1 AND scope = 'project' AND deprecated = false AND deleted_at IS NULL
            ORDER BY name, created_at DESC
            LIMIT $2 OFFSET $3
            "#
        ))
        .bind(project_id.as_uuid())
        .bind(limit)
        .bind(offset)
        .fetch_all(self.pool)
        .await?;

        Ok(workflows)
    }

    /// Admin: approve catalog submission.
    pub async fn approve_global(
        &self,
        org_id: OrganizationId,
        workflow_id: Uuid,
        reviewer: UserId,
    ) -> Result<ReusableWorkflow> {
        sqlx::query_as::<_, ReusableWorkflow>(&format!(
            r#"
            UPDATE reusable_workflows
            SET submission_status = 'approved',
                reviewed_by = $3,
                reviewed_at = NOW(),
                updated_at = NOW()
            WHERE id = $1 AND org_id = $2 AND scope = 'global' AND deleted_at IS NULL
            RETURNING {WF_SELECT}
            "#
        ))
        .bind(workflow_id)
        .bind(org_id.as_uuid())
        .bind(reviewer.as_uuid())
        .fetch_optional(self.pool)
        .await?
        .ok_or_else(|| StoreError::not_found("workflow", workflow_id))
    }

    /// Admin: reject catalog submission.
    pub async fn reject_global(
        &self,
        org_id: OrganizationId,
        workflow_id: Uuid,
        reviewer: UserId,
    ) -> Result<ReusableWorkflow> {
        sqlx::query_as::<_, ReusableWorkflow>(&format!(
            r#"
            UPDATE reusable_workflows
            SET submission_status = 'rejected',
                reviewed_by = $3,
                reviewed_at = NOW(),
                updated_at = NOW()
            WHERE id = $1 AND org_id = $2 AND scope = 'global' AND deleted_at IS NULL
            RETURNING {WF_SELECT}
            "#
        ))
        .bind(workflow_id)
        .bind(org_id.as_uuid())
        .bind(reviewer.as_uuid())
        .fetch_optional(self.pool)
        .await?
        .ok_or_else(|| StoreError::not_found("workflow", workflow_id))
    }

    /// Admin: set trust on a global catalog row.
    pub async fn set_global_trust(
        &self,
        org_id: OrganizationId,
        workflow_id: Uuid,
        trust: WorkflowTrustState,
        reviewer: UserId,
    ) -> Result<ReusableWorkflow> {
        sqlx::query_as::<_, ReusableWorkflow>(&format!(
            r#"
            UPDATE reusable_workflows
            SET trust_state = $3,
                reviewed_by = $4,
                reviewed_at = NOW(),
                updated_at = NOW()
            WHERE id = $1 AND org_id = $2 AND scope = 'global' AND deleted_at IS NULL
            RETURNING {WF_SELECT}
            "#
        ))
        .bind(workflow_id)
        .bind(org_id.as_uuid())
        .bind(trust)
        .bind(reviewer.as_uuid())
        .fetch_optional(self.pool)
        .await?
        .ok_or_else(|| StoreError::not_found("workflow", workflow_id))
    }

    /// Admin: soft-delete global catalog version.
    pub async fn soft_delete_global(
        &self,
        org_id: OrganizationId,
        workflow_id: Uuid,
        reviewer: UserId,
    ) -> Result<()> {
        let res = sqlx::query(
            r#"
            UPDATE reusable_workflows
            SET deleted_at = NOW(),
                reviewed_by = $3,
                reviewed_at = NOW(),
                updated_at = NOW()
            WHERE id = $1 AND org_id = $2 AND scope = 'global' AND deleted_at IS NULL
            "#,
        )
        .bind(workflow_id)
        .bind(org_id.as_uuid())
        .bind(reviewer.as_uuid())
        .execute(self.pool)
        .await?;

        if res.rows_affected() == 0 {
            return Err(StoreError::not_found("workflow", workflow_id));
        }
        Ok(())
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
                    UPDATE reusable_workflows rw
                    SET deprecated = true, updated_at = NOW()
                    FROM organizations o
                    WHERE rw.org_id = o.id AND rw.org_id = $1 AND rw.scope = 'global' AND rw.name = $2 AND rw.version = $3
                      AND rw.deleted_at IS NULL
                      AND rw.submission_status = 'approved'
                      AND (rw.trust_state = 'trusted' OR o.allow_untrusted_workflows)
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

    /// List all live global versions of a workflow that have SCM coordinates (for auto-sync).
    pub async fn list_global_catalog_versions_with_scm(
        &self,
        org_id: OrganizationId,
        workflow_name: &str,
    ) -> Result<Vec<ReusableWorkflow>> {
        sqlx::query_as::<_, ReusableWorkflow>(&format!(
            r#"
            SELECT {WF_SELECT}
            FROM reusable_workflows
            WHERE org_id = $1
              AND scope = 'global'
              AND project_id IS NULL
              AND name = $2
              AND deleted_at IS NULL
              AND source = 'git'
              AND scm_repository IS NOT NULL
              AND scm_path IS NOT NULL
            "#
        ))
        .bind(org_id.as_uuid())
        .bind(workflow_name)
        .fetch_all(self.pool)
        .await
        .map_err(StoreError::from)
    }

    /// Admin: set or clear the date-gated deprecation period and note.
    pub async fn set_deprecation(
        &self,
        org_id: OrganizationId,
        workflow_id: Uuid,
        deprecated_after: Option<DateTime<Utc>>,
        deprecation_note: Option<&str>,
    ) -> Result<ReusableWorkflow> {
        sqlx::query_as::<_, ReusableWorkflow>(&format!(
            r#"
            UPDATE reusable_workflows
            SET deprecated_after = $1,
                deprecation_note = $2,
                updated_at = NOW()
            WHERE id = $3 AND org_id = $4 AND deleted_at IS NULL
            RETURNING {WF_SELECT}
            "#
        ))
        .bind(deprecated_after)
        .bind(deprecation_note)
        .bind(workflow_id)
        .bind(org_id.as_uuid())
        .fetch_optional(self.pool)
        .await?
        .ok_or_else(|| StoreError::not_found("workflow", workflow_id))
    }
}
