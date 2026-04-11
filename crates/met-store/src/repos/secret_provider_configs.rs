//! Repository for `secret_provider_configs` (ADR-020, Phase 1.2).

use met_core::ids::{OrganizationId, ProjectId};
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{Result, StoreError};

/// Provider config row (metadata only — encrypted config is opaque bytes).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SecretProviderConfigRow {
    pub id: Uuid,
    pub org_id: Uuid,
    pub project_id: Option<Uuid>,
    pub name: String,
    pub provider_type: String,
    pub config_encrypted: Vec<u8>,
    pub resolution_mode: String,
    pub enabled: bool,
    pub last_tested_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_test_ok: Option<bool>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Public metadata for listing (no encrypted config).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SecretProviderConfigMeta {
    pub id: Uuid,
    pub org_id: Uuid,
    pub project_id: Option<Uuid>,
    pub name: String,
    pub provider_type: String,
    pub resolution_mode: String,
    pub enabled: bool,
    pub last_tested_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_test_ok: Option<bool>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

pub struct SecretProviderConfigRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> SecretProviderConfigRepo<'a> {
    #[must_use]
    pub const fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// List configs visible to a project (project-scoped + org-scoped).
    pub async fn list_for_project(
        &self,
        org_id: OrganizationId,
        project_id: ProjectId,
    ) -> Result<Vec<SecretProviderConfigMeta>> {
        let rows = sqlx::query_as::<_, SecretProviderConfigMeta>(
            r#"
            SELECT id, org_id, project_id, name, provider_type, resolution_mode, enabled,
                   last_tested_at, last_test_ok, created_at, updated_at
            FROM secret_provider_configs
            WHERE org_id = $1 AND (project_id IS NULL OR project_id = $2)
            ORDER BY name
            "#,
        )
        .bind(org_id.as_uuid())
        .bind(project_id.as_uuid())
        .fetch_all(self.pool)
        .await?;
        Ok(rows)
    }

    /// List org-scoped configs only.
    pub async fn list_for_org(
        &self,
        org_id: OrganizationId,
    ) -> Result<Vec<SecretProviderConfigMeta>> {
        let rows = sqlx::query_as::<_, SecretProviderConfigMeta>(
            r#"
            SELECT id, org_id, project_id, name, provider_type, resolution_mode, enabled,
                   last_tested_at, last_test_ok, created_at, updated_at
            FROM secret_provider_configs
            WHERE org_id = $1 AND project_id IS NULL
            ORDER BY name
            "#,
        )
        .bind(org_id.as_uuid())
        .fetch_all(self.pool)
        .await?;
        Ok(rows)
    }

    /// Get a config by ID (includes encrypted config for resolution).
    pub async fn get(&self, id: Uuid) -> Result<SecretProviderConfigRow> {
        sqlx::query_as::<_, SecretProviderConfigRow>(
            r#"
            SELECT id, org_id, project_id, name, provider_type, config_encrypted,
                   resolution_mode, enabled, last_tested_at, last_test_ok, created_at, updated_at
            FROM secret_provider_configs
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(self.pool)
        .await?
        .ok_or_else(|| StoreError::not_found("secret_provider_config", id))
    }

    /// Find a config by name within org/project scope (project-scoped shadows org-scoped).
    pub async fn find_by_name(
        &self,
        org_id: OrganizationId,
        project_id: Option<ProjectId>,
        name: &str,
    ) -> Result<Option<SecretProviderConfigRow>> {
        let row = sqlx::query_as::<_, SecretProviderConfigRow>(
            r#"
            SELECT id, org_id, project_id, name, provider_type, config_encrypted,
                   resolution_mode, enabled, last_tested_at, last_test_ok, created_at, updated_at
            FROM secret_provider_configs
            WHERE org_id = $1
              AND (project_id IS NULL OR project_id = $2)
              AND name = $3
            ORDER BY
              CASE WHEN project_id IS NOT NULL THEN 0 ELSE 1 END
            LIMIT 1
            "#,
        )
        .bind(org_id.as_uuid())
        .bind(project_id.map(|p| p.as_uuid()))
        .bind(name)
        .fetch_optional(self.pool)
        .await?;
        Ok(row)
    }

    /// Insert a new provider config.
    pub async fn create(
        &self,
        org_id: OrganizationId,
        project_id: Option<ProjectId>,
        name: &str,
        provider_type: &str,
        config_encrypted: &[u8],
        resolution_mode: &str,
    ) -> Result<SecretProviderConfigMeta> {
        let row = sqlx::query_as::<_, SecretProviderConfigMeta>(
            r#"
            INSERT INTO secret_provider_configs (org_id, project_id, name, provider_type, config_encrypted, resolution_mode)
            VALUES ($1, $2, $3, $4::secret_provider_type, $5, $6)
            RETURNING id, org_id, project_id, name, provider_type::text, resolution_mode, enabled,
                      last_tested_at, last_test_ok, created_at, updated_at
            "#,
        )
        .bind(org_id.as_uuid())
        .bind(project_id.map(|p| p.as_uuid()))
        .bind(name)
        .bind(provider_type)
        .bind(config_encrypted)
        .bind(resolution_mode)
        .fetch_one(self.pool)
        .await?;
        Ok(row)
    }

    /// Delete a provider config.
    pub async fn delete(&self, id: Uuid) -> Result<()> {
        let r = sqlx::query("DELETE FROM secret_provider_configs WHERE id = $1")
            .bind(id)
            .execute(self.pool)
            .await?;
        if r.rows_affected() == 0 {
            return Err(StoreError::not_found("secret_provider_config", id));
        }
        Ok(())
    }

    /// Record a connectivity test result.
    pub async fn record_test_result(&self, id: Uuid, ok: bool) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE secret_provider_configs
            SET last_tested_at = NOW(), last_test_ok = $2, updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(id)
        .bind(ok)
        .execute(self.pool)
        .await?;
        Ok(())
    }
}
