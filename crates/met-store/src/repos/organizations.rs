//! Organization repository.

use chrono::Utc;
use met_core::ids::OrganizationId;
use met_core::models::{CreateOrganization, Organization, UpdateOrganization};
use sqlx::PgPool;

use crate::error::{Result, StoreError};

/// Repository for organization operations.
pub struct OrganizationRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> OrganizationRepo<'a> {
    /// Create a new organization repository.
    #[must_use]
    pub const fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Create a new organization.
    pub async fn create(&self, input: &CreateOrganization) -> Result<Organization> {
        let id = OrganizationId::new();
        let now = Utc::now();

        let org = sqlx::query_as::<_, Organization>(
            r#"
            INSERT INTO organizations (id, name, slug, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $4)
            RETURNING id, name, slug, created_at, updated_at, deleted_at, allow_untrusted_workflows
            "#,
        )
        .bind(id.as_uuid())
        .bind(&input.name)
        .bind(&input.slug)
        .bind(now)
        .fetch_one(self.pool)
        .await?;

        Ok(org)
    }

    /// Get an organization by ID.
    pub async fn get(&self, id: OrganizationId) -> Result<Organization> {
        sqlx::query_as::<_, Organization>(
            r#"
            SELECT id, name, slug, created_at, updated_at, deleted_at, allow_untrusted_workflows
            FROM organizations
            WHERE id = $1 AND deleted_at IS NULL
            "#,
        )
        .bind(id.as_uuid())
        .fetch_optional(self.pool)
        .await?
        .ok_or_else(|| StoreError::not_found("organization", id))
    }

    /// Get an organization by slug.
    pub async fn get_by_slug(&self, slug: &str) -> Result<Organization> {
        sqlx::query_as::<_, Organization>(
            r#"
            SELECT id, name, slug, created_at, updated_at, deleted_at, allow_untrusted_workflows
            FROM organizations
            WHERE slug = $1 AND deleted_at IS NULL
            "#,
        )
        .bind(slug)
        .fetch_optional(self.pool)
        .await?
        .ok_or_else(|| StoreError::not_found("organization", slug))
    }

    /// List all active organizations.
    pub async fn list(&self, limit: i64, offset: i64) -> Result<Vec<Organization>> {
        let orgs = sqlx::query_as::<_, Organization>(
            r#"
            SELECT id, name, slug, created_at, updated_at, deleted_at, allow_untrusted_workflows
            FROM organizations
            WHERE deleted_at IS NULL
            ORDER BY created_at DESC
            LIMIT $1 OFFSET $2
            "#,
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(self.pool)
        .await?;

        Ok(orgs)
    }

    /// Update an organization.
    pub async fn update(
        &self,
        id: OrganizationId,
        input: &UpdateOrganization,
    ) -> Result<Organization> {
        let existing = self.get(id).await?;

        let name = input.name.as_ref().unwrap_or(&existing.name);
        let allow_untrusted = input
            .allow_untrusted_workflows
            .unwrap_or(existing.allow_untrusted_workflows);

        let org = sqlx::query_as::<_, Organization>(
            r#"
            UPDATE organizations
            SET name = $2,
                allow_untrusted_workflows = $3,
                updated_at = NOW()
            WHERE id = $1 AND deleted_at IS NULL
            RETURNING id, name, slug, created_at, updated_at, deleted_at, allow_untrusted_workflows
            "#,
        )
        .bind(id.as_uuid())
        .bind(name)
        .bind(allow_untrusted)
        .fetch_one(self.pool)
        .await?;

        Ok(org)
    }

    /// Soft-delete an organization.
    pub async fn delete(&self, id: OrganizationId) -> Result<()> {
        let result = sqlx::query(
            r#"
            UPDATE organizations
            SET deleted_at = NOW()
            WHERE id = $1 AND deleted_at IS NULL
            "#,
        )
        .bind(id.as_uuid())
        .execute(self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(StoreError::not_found("organization", id));
        }

        Ok(())
    }

    /// Count active organizations.
    pub async fn count(&self) -> Result<i64> {
        let (count,): (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*)
            FROM organizations
            WHERE deleted_at IS NULL
            "#,
        )
        .fetch_one(self.pool)
        .await?;

        Ok(count)
    }
}
