//! Pipeline repository.

use chrono::Utc;
use met_core::ids::{PipelineId, ProjectId};
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
    pub async fn create(&self, project_id: ProjectId, input: &CreatePipeline) -> Result<Pipeline> {
        let id = PipelineId::new();
        let now = Utc::now();

        let pipeline = sqlx::query_as::<_, Pipeline>(
            r#"
            INSERT INTO pipelines (id, project_id, name, slug, description, definition, definition_path, enabled, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, true, $8, $8)
            RETURNING id, project_id, name, slug, description, definition, definition_path, enabled, created_at, updated_at
            "#,
        )
        .bind(id.as_uuid())
        .bind(project_id.as_uuid())
        .bind(&input.name)
        .bind(&input.slug)
        .bind(&input.description)
        .bind(&input.definition)
        .bind(&input.definition_path)
        .bind(now)
        .fetch_one(self.pool)
        .await?;

        Ok(pipeline)
    }

    /// Get a pipeline by ID.
    pub async fn get(&self, id: PipelineId) -> Result<Pipeline> {
        sqlx::query_as::<_, Pipeline>(
            r#"
            SELECT id, project_id, name, slug, description, definition, definition_path, enabled, created_at, updated_at
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
            SELECT id, project_id, name, slug, description, definition, definition_path, enabled, created_at, updated_at
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
            SELECT id, project_id, name, slug, description, definition, definition_path, enabled, created_at, updated_at
            FROM pipelines
            WHERE project_id = $1
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

        let pipeline = sqlx::query_as::<_, Pipeline>(
            r#"
            UPDATE pipelines
            SET name = $2, description = $3, definition = $4, enabled = $5, updated_at = NOW()
            WHERE id = $1
            RETURNING id, project_id, name, slug, description, definition, definition_path, enabled, created_at, updated_at
            "#,
        )
        .bind(id.as_uuid())
        .bind(name)
        .bind(description)
        .bind(definition)
        .bind(enabled)
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
            WHERE project_id = $1
            "#,
        )
        .bind(project_id.as_uuid())
        .fetch_one(self.pool)
        .await?;

        Ok(count)
    }
}
