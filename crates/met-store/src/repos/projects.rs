//! Project repository.

use chrono::Utc;
use met_core::ids::{OrganizationId, ProjectId};
use met_core::models::{CreateProject, OwnerType, Project, UpdateProject};
use sqlx::PgPool;

use crate::error::{Result, StoreError};

/// Repository for project operations.
pub struct ProjectRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> ProjectRepo<'a> {
    /// Create a new project repository.
    #[must_use]
    pub const fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Create a new project.
    pub async fn create(&self, org_id: OrganizationId, input: &CreateProject) -> Result<Project> {
        let id = ProjectId::new();
        let now = Utc::now();

        let project = sqlx::query_as::<_, Project>(
            r#"
            INSERT INTO projects (id, org_id, name, slug, description, owner_type, owner_id, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $8)
            RETURNING id, org_id, name, slug, description, owner_type, owner_id, created_at, updated_at, deleted_at
            "#,
        )
        .bind(id.as_uuid())
        .bind(org_id.as_uuid())
        .bind(&input.name)
        .bind(&input.slug)
        .bind(&input.description)
        .bind(&input.owner_type)
        .bind(&input.owner_id)
        .bind(now)
        .fetch_one(self.pool)
        .await?;

        Ok(project)
    }

    /// Get a project by ID.
    pub async fn get(&self, id: ProjectId) -> Result<Project> {
        sqlx::query_as::<_, Project>(
            r#"
            SELECT id, org_id, name, slug, description, owner_type, owner_id, created_at, updated_at, deleted_at
            FROM projects
            WHERE id = $1 AND deleted_at IS NULL
            "#,
        )
        .bind(id.as_uuid())
        .fetch_optional(self.pool)
        .await?
        .ok_or_else(|| StoreError::not_found("project", id))
    }

    /// Get a project by org and slug.
    pub async fn get_by_slug(&self, org_id: OrganizationId, slug: &str) -> Result<Project> {
        sqlx::query_as::<_, Project>(
            r#"
            SELECT id, org_id, name, slug, description, owner_type, owner_id, created_at, updated_at, deleted_at
            FROM projects
            WHERE org_id = $1 AND slug = $2 AND deleted_at IS NULL
            "#,
        )
        .bind(org_id.as_uuid())
        .bind(slug)
        .fetch_optional(self.pool)
        .await?
        .ok_or_else(|| StoreError::not_found("project", slug))
    }

    /// List projects in an organization.
    pub async fn list_by_org(
        &self,
        org_id: OrganizationId,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Project>> {
        let projects = sqlx::query_as::<_, Project>(
            r#"
            SELECT id, org_id, name, slug, description, owner_type, owner_id, created_at, updated_at, deleted_at
            FROM projects
            WHERE org_id = $1 AND deleted_at IS NULL
            ORDER BY created_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(org_id.as_uuid())
        .bind(limit)
        .bind(offset)
        .fetch_all(self.pool)
        .await?;

        Ok(projects)
    }

    /// Update a project.
    pub async fn update(&self, id: ProjectId, input: &UpdateProject) -> Result<Project> {
        let existing = self.get(id).await?;

        let name = input.name.as_ref().unwrap_or(&existing.name);
        let description = input.description.as_ref().or(existing.description.as_ref());

        let project = sqlx::query_as::<_, Project>(
            r#"
            UPDATE projects
            SET name = $2, description = $3, updated_at = NOW()
            WHERE id = $1 AND deleted_at IS NULL
            RETURNING id, org_id, name, slug, description, owner_type, owner_id, created_at, updated_at, deleted_at
            "#,
        )
        .bind(id.as_uuid())
        .bind(name)
        .bind(description)
        .fetch_one(self.pool)
        .await?;

        Ok(project)
    }

    /// Soft-delete a project.
    pub async fn delete(&self, id: ProjectId) -> Result<()> {
        let result = sqlx::query(
            r#"
            UPDATE projects
            SET deleted_at = NOW()
            WHERE id = $1 AND deleted_at IS NULL
            "#,
        )
        .bind(id.as_uuid())
        .execute(self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(StoreError::not_found("project", id));
        }

        Ok(())
    }

    /// Count projects in an organization.
    pub async fn count_by_org(&self, org_id: OrganizationId) -> Result<i64> {
        let (count,): (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*)
            FROM projects
            WHERE org_id = $1 AND deleted_at IS NULL
            "#,
        )
        .bind(org_id.as_uuid())
        .fetch_one(self.pool)
        .await?;

        Ok(count)
    }
}

// Suppress unused warning for OwnerType which is used in the query binding
const _: () = {
    fn _assert_owner_type_used(_: OwnerType) {}
};
