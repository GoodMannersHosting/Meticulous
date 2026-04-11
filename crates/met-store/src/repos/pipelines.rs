//! Pipeline repository.

use chrono::Utc;
use met_core::ids::{OrganizationId, PipelineId, ProjectId};
use met_core::models::{CreatePipeline, Pipeline, UpdatePipeline};
use sqlx::PgPool;

use crate::error::{Result, StoreError};

/// Repository for pipeline operations.
pub struct PipelineRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> PipelineRepo<'a> {
    /// Create a new pipeline repository.
    #[must_use]
    pub const fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Create a new pipeline.
    ///
    /// The project's owner is inherited and inserted as an `admin` pipeline member.
    pub async fn create(&self, project_id: ProjectId, input: &CreatePipeline) -> Result<Pipeline> {
        let id = PipelineId::new();
        let now = Utc::now();

        let pipeline = sqlx::query_as::<_, Pipeline>(
            r#"
            INSERT INTO pipelines (
                id, project_id, name, slug, description, definition, definition_path,
                scm_provider, scm_repository, scm_ref, scm_path, scm_credentials_secret_path, scm_revision,
                visibility, enabled, archived_at, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, true, NULL, $15, $15)
            RETURNING id, project_id, name, slug, description, definition, definition_path,
                      scm_provider, scm_repository, scm_ref, scm_path, scm_credentials_secret_path, scm_revision,
                      owner_type, owner_id, visibility, enabled, archived_at, created_at, updated_at
            "#,
        )
        .bind(id.as_uuid())
        .bind(project_id.as_uuid())
        .bind(&input.name)
        .bind(&input.slug)
        .bind(&input.description)
        .bind(&input.definition)
        .bind(&input.definition_path)
        .bind(&input.scm_provider)
        .bind(&input.scm_repository)
        .bind(&input.scm_ref)
        .bind(&input.scm_path)
        .bind(&input.scm_credentials_secret_path)
        .bind(&input.scm_revision)
        .bind(&input.visibility)
        .bind(now)
        .fetch_one(self.pool)
        .await?;

        Ok(pipeline)
    }

    /// Get a pipeline by ID.
    pub async fn get(&self, id: PipelineId) -> Result<Pipeline> {
        sqlx::query_as::<_, Pipeline>(
            r#"
            SELECT id, project_id, name, slug, description, definition, definition_path,
                   scm_provider, scm_repository, scm_ref, scm_path, scm_credentials_secret_path, scm_revision,
                   owner_type, owner_id, visibility, enabled, archived_at, created_at, updated_at
            FROM pipelines
            WHERE id = $1
            "#,
        )
        .bind(id.as_uuid())
        .fetch_optional(self.pool)
        .await?
        .ok_or_else(|| StoreError::not_found("pipeline", id))
    }

    /// Get a pipeline by project and slug.
    pub async fn get_by_slug(&self, project_id: ProjectId, slug: &str) -> Result<Pipeline> {
        sqlx::query_as::<_, Pipeline>(
            r#"
            SELECT id, project_id, name, slug, description, definition, definition_path,
                   scm_provider, scm_repository, scm_ref, scm_path, scm_credentials_secret_path, scm_revision,
                   owner_type, owner_id, visibility, enabled, archived_at, created_at, updated_at
            FROM pipelines
            WHERE project_id = $1 AND slug = $2
            "#,
        )
        .bind(project_id.as_uuid())
        .bind(slug)
        .fetch_optional(self.pool)
        .await?
        .ok_or_else(|| StoreError::not_found("pipeline", slug))
    }

    /// List pipelines in a project.
    pub async fn list_by_project(
        &self,
        project_id: ProjectId,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Pipeline>> {
        let pipelines = sqlx::query_as::<_, Pipeline>(
            r#"
            SELECT id, project_id, name, slug, description, definition, definition_path,
                   scm_provider, scm_repository, scm_ref, scm_path, scm_credentials_secret_path, scm_revision,
                   owner_type, owner_id, visibility, enabled, archived_at, created_at, updated_at
            FROM pipelines
            WHERE project_id = $1 AND archived_at IS NULL
            ORDER BY created_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(project_id.as_uuid())
        .bind(limit)
        .bind(offset)
        .fetch_all(self.pool)
        .await?;

        Ok(pipelines)
    }

    /// Update a pipeline.
    pub async fn update(&self, id: PipelineId, input: &UpdatePipeline) -> Result<Pipeline> {
        let existing = self.get(id).await?;

        let name = input.name.as_ref().unwrap_or(&existing.name);
        let description = input.description.as_ref().or(existing.description.as_ref());
        let definition = input.definition.as_ref().unwrap_or(&existing.definition);
        let enabled = input.enabled.unwrap_or(existing.enabled);
        let scm_provider = input
            .scm_provider
            .clone()
            .or_else(|| existing.scm_provider.clone());
        let scm_repository = input
            .scm_repository
            .clone()
            .or_else(|| existing.scm_repository.clone());
        let scm_ref = input.scm_ref.clone().or_else(|| existing.scm_ref.clone());
        let scm_path = input.scm_path.clone().or_else(|| existing.scm_path.clone());
        let scm_credentials = input
            .scm_credentials_secret_path
            .clone()
            .or_else(|| existing.scm_credentials_secret_path.clone());
        let scm_revision = input
            .scm_revision
            .clone()
            .or_else(|| existing.scm_revision.clone());

        let pipeline = sqlx::query_as::<_, Pipeline>(
            r#"
            UPDATE pipelines
            SET name = $2,
                description = $3,
                definition = $4,
                enabled = $5,
                scm_provider = $6,
                scm_repository = $7,
                scm_ref = $8,
                scm_path = $9,
                scm_credentials_secret_path = $10,
                scm_revision = $11,
                updated_at = NOW()
            WHERE id = $1
            RETURNING id, project_id, name, slug, description, definition, definition_path,
                      scm_provider, scm_repository, scm_ref, scm_path, scm_credentials_secret_path, scm_revision,
                      owner_type, owner_id, visibility, enabled, archived_at, created_at, updated_at
            "#,
        )
        .bind(id.as_uuid())
        .bind(name)
        .bind(description)
        .bind(definition)
        .bind(enabled)
        .bind(scm_provider)
        .bind(scm_repository)
        .bind(scm_ref)
        .bind(scm_path)
        .bind(scm_credentials)
        .bind(scm_revision)
        .fetch_one(self.pool)
        .await?;

        Ok(pipeline)
    }

    /// Delete a pipeline.
    pub async fn delete(&self, id: PipelineId) -> Result<()> {
        let result = sqlx::query(
            r#"
            DELETE FROM pipelines
            WHERE id = $1
            "#,
        )
        .bind(id.as_uuid())
        .execute(self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(StoreError::not_found("pipeline", id));
        }

        Ok(())
    }

    /// Count pipelines in a project.
    pub async fn count_by_project(&self, project_id: ProjectId) -> Result<i64> {
        let (count,): (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*)
            FROM pipelines
            WHERE project_id = $1 AND archived_at IS NULL
            "#,
        )
        .bind(project_id.as_uuid())
        .fetch_one(self.pool)
        .await?;

        Ok(count)
    }

    /// Archive every non-archived pipeline in a project (e.g. when the project is archived).
    pub async fn archive_all_in_project(&self, project_id: ProjectId) -> Result<u64> {
        let res = sqlx::query(
            r#"
            UPDATE pipelines
            SET archived_at = NOW(), updated_at = NOW()
            WHERE project_id = $1 AND archived_at IS NULL
            "#,
        )
        .bind(project_id.as_uuid())
        .execute(self.pool)
        .await?;

        Ok(res.rows_affected())
    }

    /// Unarchive all archived pipelines belonging to a project (e.g. after admin restores the project).
    pub async fn unarchive_all_in_project(&self, project_id: ProjectId) -> Result<u64> {
        let res = sqlx::query(
            r#"
            UPDATE pipelines
            SET archived_at = NULL, updated_at = NOW()
            WHERE project_id = $1 AND archived_at IS NOT NULL
            "#,
        )
        .bind(project_id.as_uuid())
        .execute(self.pool)
        .await?;

        Ok(res.rows_affected())
    }

    /// Move pipeline to archived state (still in DB; hidden from normal lists).
    pub async fn archive(&self, id: PipelineId) -> Result<Pipeline> {
        sqlx::query_as::<_, Pipeline>(
            r#"
            UPDATE pipelines
            SET archived_at = NOW(), updated_at = NOW()
            WHERE id = $1 AND archived_at IS NULL
            RETURNING id, project_id, name, slug, description, definition, definition_path,
                      scm_provider, scm_repository, scm_ref, scm_path, scm_credentials_secret_path, scm_revision,
                      owner_type, owner_id, visibility, enabled, archived_at, created_at, updated_at
            "#,
        )
        .bind(id.as_uuid())
        .fetch_optional(self.pool)
        .await?
        .ok_or_else(|| StoreError::not_found("pipeline", id))
    }

    pub async fn unarchive(&self, id: PipelineId) -> Result<Pipeline> {
        sqlx::query_as::<_, Pipeline>(
            r#"
            UPDATE pipelines
            SET archived_at = NULL, updated_at = NOW()
            WHERE id = $1 AND archived_at IS NOT NULL
            RETURNING id, project_id, name, slug, description, definition, definition_path,
                      scm_provider, scm_repository, scm_ref, scm_path, scm_credentials_secret_path, scm_revision,
                      owner_type, owner_id, visibility, enabled, archived_at, created_at, updated_at
            "#,
        )
        .bind(id.as_uuid())
        .fetch_optional(self.pool)
        .await?
        .ok_or_else(|| StoreError::not_found("pipeline", id))
    }

    /// Archived pipelines in an organization (across projects).
    pub async fn list_archived_for_org(&self, org_id: OrganizationId) -> Result<Vec<Pipeline>> {
        sqlx::query_as::<_, Pipeline>(
            r#"
            SELECT p.id, p.project_id, p.name, p.slug, p.description, p.definition, p.definition_path,
                   p.scm_provider, p.scm_repository, p.scm_ref, p.scm_path, p.scm_credentials_secret_path, p.scm_revision,
                   p.owner_type, p.owner_id, p.visibility, p.enabled, p.archived_at, p.created_at, p.updated_at
            FROM pipelines p
            INNER JOIN projects pr ON pr.id = p.project_id
            WHERE pr.org_id = $1 AND p.archived_at IS NOT NULL AND pr.deleted_at IS NULL
            ORDER BY p.archived_at DESC
            "#,
        )
        .bind(org_id.as_uuid())
        .fetch_all(self.pool)
        .await
        .map_err(Into::into)
    }
}
