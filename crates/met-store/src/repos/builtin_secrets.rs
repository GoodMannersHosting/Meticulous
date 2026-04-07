//! Encrypted platform secrets (`builtin_secrets`).

use chrono::{DateTime, Utc};
use met_core::ids::{OrganizationId, PipelineId, ProjectId};
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{Result, StoreError};

/// Logical kind for stored payloads (matches DB check constraint).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoredSecretKind {
    Kv,
    SshPrivateKey,
    GithubApp,
    ApiKey,
    X509Bundle,
}

impl StoredSecretKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Kv => "kv",
            Self::SshPrivateKey => "ssh_private_key",
            Self::GithubApp => "github_app",
            Self::ApiKey => "api_key",
            Self::X509Bundle => "x509_bundle",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "kv" => Ok(Self::Kv),
            "ssh_private_key" => Ok(Self::SshPrivateKey),
            "github_app" => Ok(Self::GithubApp),
            "api_key" => Ok(Self::ApiKey),
            "x509_bundle" => Ok(Self::X509Bundle),
            _ => Err(StoreError::validation(format!("unknown secret kind: {s}"))),
        }
    }
}

/// Row for resolution (includes ciphertext).
#[derive(Debug, Clone)]
pub struct BuiltinSecretCipherRow {
    pub id: Uuid,
    pub encrypted_value: Vec<u8>,
    pub nonce: Vec<u8>,
    pub key_id: String,
    pub kind: String,
    pub version: i32,
}

/// Public metadata (no ciphertext).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct BuiltinSecretMetaRow {
    pub id: Uuid,
    pub org_id: Uuid,
    pub project_id: Option<Uuid>,
    pub pipeline_id: Option<Uuid>,
    pub path: String,
    pub kind: String,
    pub version: i32,
    pub metadata: serde_json::Value,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// For `project_id` NULL: when `false`, not listed or resolved for pipelines (catalog SCM may still use).
    pub propagate_to_projects: bool,
}

/// Repository for `builtin_secrets`.
pub struct BuiltinSecretsRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> BuiltinSecretsRepo<'a> {
    #[must_use]
    pub const fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Pick the narrowest matching row for org/project/pipeline scope and logical `path`.
    ///
    /// Org-wide rows with [`BuiltinSecretMetaRow::propagate_to_projects`] `false` are excluded (pipelines / `stored:`).
    pub async fn get_current_cipher_row(
        &self,
        org_id: OrganizationId,
        project_id: ProjectId,
        pipeline_id: PipelineId,
        path: &str,
    ) -> Result<Option<BuiltinSecretCipherRow>> {
        self.get_current_cipher_row_impl(
            org_id,
            project_id,
            pipeline_id,
            path,
            false,
        )
        .await
    }

    /// Like [`Self::get_current_cipher_row`], but org-wide secrets that do not propagate are included
    /// (catalog Git import and other platform SCM using a project context path only).
    pub async fn get_current_cipher_row_for_catalog_scm(
        &self,
        org_id: OrganizationId,
        project_id: ProjectId,
        pipeline_id: PipelineId,
        path: &str,
    ) -> Result<Option<BuiltinSecretCipherRow>> {
        self.get_current_cipher_row_impl(org_id, project_id, pipeline_id, path, true)
            .await
    }

    async fn get_current_cipher_row_impl(
        &self,
        org_id: OrganizationId,
        project_id: ProjectId,
        pipeline_id: PipelineId,
        path: &str,
        include_non_propagating_org_secrets: bool,
    ) -> Result<Option<BuiltinSecretCipherRow>> {
        let row = sqlx::query_as::<_, (Uuid, Vec<u8>, Vec<u8>, String, String, i32)>(
            r#"
            SELECT id, encrypted_value, nonce, key_id, kind, version
            FROM builtin_secrets
            WHERE org_id = $1
              AND path = $2
              AND deleted_at IS NULL
              AND (project_id IS NULL OR project_id = $3)
              AND (pipeline_id IS NULL OR pipeline_id = $4)
              AND (
                $5::bool
                OR project_id IS NOT NULL
                OR propagate_to_projects
              )
            ORDER BY
              CASE
                WHEN pipeline_id IS NOT NULL AND pipeline_id = $4 THEN 0
                WHEN pipeline_id IS NULL AND project_id IS NOT NULL AND project_id = $3 THEN 1
                WHEN pipeline_id IS NULL AND project_id IS NULL THEN 2
                ELSE 3
              END ASC,
              version DESC
            LIMIT 1
            "#,
        )
        .bind(org_id.as_uuid())
        .bind(path)
        .bind(project_id.as_uuid())
        .bind(pipeline_id.as_uuid())
        .bind(include_non_propagating_org_secrets)
        .fetch_optional(self.pool)
        .await?;

        Ok(row.map(|(id, encrypted_value, nonce, key_id, kind, version)| BuiltinSecretCipherRow {
            id,
            encrypted_value,
            nonce,
            key_id,
            kind,
            version,
        }))
    }

    /// Whether a resolvable row exists (validation, no decrypt).
    pub async fn exists_resolvable(
        &self,
        org_id: OrganizationId,
        project_id: ProjectId,
        pipeline_id: PipelineId,
        path: &str,
    ) -> Result<bool> {
        Ok(self
            .get_current_cipher_row(org_id, project_id, pipeline_id, path)
            .await?
            .is_some())
    }

    /// Next version for this scope + path.
    pub async fn next_version(
        &self,
        org_id: OrganizationId,
        project_id: Option<ProjectId>,
        pipeline_id: Option<PipelineId>,
        path: &str,
    ) -> Result<i32> {
        let (n,): (Option<i32>,) = sqlx::query_as(
            r#"
            SELECT MAX(version)
            FROM builtin_secrets
            WHERE org_id = $1
              AND path = $2
              AND COALESCE(project_id, '00000000-0000-0000-0000-000000000000'::uuid)
                  = COALESCE($3, '00000000-0000-0000-0000-000000000000'::uuid)
              AND COALESCE(pipeline_id, '00000000-0000-0000-0000-000000000000'::uuid)
                  = COALESCE($4, '00000000-0000-0000-0000-000000000000'::uuid)
            "#,
        )
        .bind(org_id.as_uuid())
        .bind(path)
        .bind(project_id.map(|p| p.as_uuid()))
        .bind(pipeline_id.map(|p| p.as_uuid()))
        .fetch_one(self.pool)
        .await?;

        Ok(n.unwrap_or(0) + 1)
    }

    /// Insert a new secret version (caller supplies ciphertext + nonce).
    pub async fn insert_encrypted(
        &self,
        org_id: OrganizationId,
        project_id: Option<ProjectId>,
        pipeline_id: Option<PipelineId>,
        path: &str,
        kind: StoredSecretKind,
        metadata: &serde_json::Value,
        description: Option<&str>,
        encrypted_value: &[u8],
        nonce: &[u8],
        key_id: &str,
        version: i32,
        created_by: Option<Uuid>,
        propagate_to_projects: bool,
    ) -> Result<BuiltinSecretMetaRow> {
        let id = Uuid::now_v7();
        let row = sqlx::query_as::<_, BuiltinSecretMetaRow>(
            r#"
            INSERT INTO builtin_secrets (
                id, org_id, project_id, pipeline_id, path, kind, metadata, description,
                encrypted_value, nonce, key_id, version, created_by, propagate_to_projects
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
            RETURNING id, org_id, project_id, pipeline_id, path, kind, version, metadata, description,
                      created_at, updated_at, propagate_to_projects
            "#,
        )
        .bind(id)
        .bind(org_id.as_uuid())
        .bind(project_id.map(|p| p.as_uuid()))
        .bind(pipeline_id.map(|p| p.as_uuid()))
        .bind(path)
        .bind(kind.as_str())
        .bind(metadata)
        .bind(description)
        .bind(encrypted_value)
        .bind(nonce)
        .bind(key_id)
        .bind(version)
        .bind(created_by)
        .bind(propagate_to_projects)
        .fetch_one(self.pool)
        .await?;

        Ok(row)
    }

    /// List metadata for secrets visible under a project (propagating org-wide + project-scoped).
    pub async fn list_for_project(&self, org_id: OrganizationId, project_id: ProjectId) -> Result<Vec<BuiltinSecretMetaRow>> {
        let rows = sqlx::query_as::<_, BuiltinSecretMetaRow>(
            r#"
            SELECT id, org_id, project_id, pipeline_id, path, kind, version, metadata, description,
                   created_at, updated_at, propagate_to_projects
            FROM builtin_secrets
            WHERE org_id = $1
              AND deleted_at IS NULL
              AND (project_id IS NULL OR project_id = $2)
              AND NOT (project_id IS NULL AND NOT propagate_to_projects)
            ORDER BY path ASC, version DESC
            "#,
        )
        .bind(org_id.as_uuid())
        .bind(project_id.as_uuid())
        .fetch_all(self.pool)
        .await?;

        Ok(rows)
    }

    /// List metadata for a specific pipeline (includes project + org scoped names that apply).
    pub async fn list_for_pipeline(
        &self,
        org_id: OrganizationId,
        project_id: ProjectId,
        pipeline_id: PipelineId,
    ) -> Result<Vec<BuiltinSecretMetaRow>> {
        let rows = sqlx::query_as::<_, BuiltinSecretMetaRow>(
            r#"
            SELECT id, org_id, project_id, pipeline_id, path, kind, version, metadata, description,
                   created_at, updated_at, propagate_to_projects
            FROM builtin_secrets
            WHERE org_id = $1
              AND deleted_at IS NULL
              AND (project_id IS NULL OR project_id = $2)
              AND (pipeline_id IS NULL OR pipeline_id = $3)
              AND NOT (project_id IS NULL AND NOT propagate_to_projects)
            ORDER BY path ASC, version DESC
            "#,
        )
        .bind(org_id.as_uuid())
        .bind(project_id.as_uuid())
        .bind(pipeline_id.as_uuid())
        .fetch_all(self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn get_meta_by_id(&self, id: Uuid) -> Result<Option<BuiltinSecretMetaRow>> {
        sqlx::query_as::<_, BuiltinSecretMetaRow>(
            r#"
            SELECT id, org_id, project_id, pipeline_id, path, kind, version, metadata, description,
                   created_at, updated_at, propagate_to_projects
            FROM builtin_secrets
            WHERE id = $1 AND deleted_at IS NULL
            "#,
        )
        .bind(id)
        .fetch_optional(self.pool)
        .await
        .map_err(Into::into)
    }

    /// Metadata by primary key, including soft-deleted rows (for admin purge).
    pub async fn get_meta_by_id_including_deleted(&self, id: Uuid) -> Result<Option<BuiltinSecretMetaRow>> {
        sqlx::query_as::<_, BuiltinSecretMetaRow>(
            r#"
            SELECT id, org_id, project_id, pipeline_id, path, kind, version, metadata, description,
                   created_at, updated_at, propagate_to_projects
            FROM builtin_secrets
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn soft_delete(&self, id: Uuid) -> Result<()> {
        let r = sqlx::query(
            r#"
            UPDATE builtin_secrets SET deleted_at = NOW(), updated_at = NOW()
            WHERE id = $1 AND deleted_at IS NULL
            "#,
        )
        .bind(id)
        .execute(self.pool)
        .await?;

        if r.rows_affected() == 0 {
            return Err(StoreError::not_found("builtin_secret", id));
        }
        Ok(())
    }

    /// All non-deleted versions for the same org / project / pipeline scope and logical `path`.
    ///
    /// `project_id` / `pipeline_id` use SQL `IS NOT DISTINCT FROM` so org-wide rows (`NULL`) match.
    pub async fn list_versions_for_scope(
        &self,
        org_id: OrganizationId,
        project_id: Option<ProjectId>,
        pipeline_id: Option<PipelineId>,
        path: &str,
    ) -> Result<Vec<BuiltinSecretMetaRow>> {
        sqlx::query_as::<_, BuiltinSecretMetaRow>(
            r#"
            SELECT id, org_id, project_id, pipeline_id, path, kind, version, metadata, description,
                   created_at, updated_at, propagate_to_projects
            FROM builtin_secrets
            WHERE org_id = $1
              AND path = $2
              AND deleted_at IS NULL
              AND project_id IS NOT DISTINCT FROM $3
              AND pipeline_id IS NOT DISTINCT FROM $4
            ORDER BY version DESC
            "#,
        )
        .bind(org_id.as_uuid())
        .bind(path)
        .bind(project_id.map(|p| p.as_uuid()))
        .bind(pipeline_id.map(|p| p.as_uuid()))
        .fetch_all(self.pool)
        .await
        .map_err(Into::into)
    }

    /// Soft-delete newer versions so resolver picks `anchor` (same scope + path as anchor row).
    pub async fn soft_delete_versions_newer_than(&self, anchor: &BuiltinSecretMetaRow) -> Result<u64> {
        let r = sqlx::query(
            r#"
            UPDATE builtin_secrets
            SET deleted_at = NOW(), updated_at = NOW()
            WHERE org_id = $1
              AND COALESCE(project_id, '00000000-0000-0000-0000-000000000000'::uuid)
                  = COALESCE($2, '00000000-0000-0000-0000-000000000000'::uuid)
              AND path = $3
              AND COALESCE(pipeline_id, '00000000-0000-0000-0000-000000000000'::uuid)
                  = COALESCE($4, '00000000-0000-0000-0000-000000000000'::uuid)
              AND version > $5
              AND deleted_at IS NULL
            "#,
        )
        .bind(anchor.org_id)
        .bind(anchor.project_id)
        .bind(&anchor.path)
        .bind(anchor.pipeline_id)
        .bind(anchor.version)
        .execute(self.pool)
        .await?;

        Ok(r.rows_affected())
    }

    /// Permanently remove one version row (ciphertext). Use only when operators need a hard delete.
    pub async fn hard_delete_by_id(&self, id: Uuid) -> Result<()> {
        let r = sqlx::query(
            r#"
            DELETE FROM builtin_secrets
            WHERE id = $1
            "#,
        )
        .bind(id)
        .execute(self.pool)
        .await?;

        if r.rows_affected() == 0 {
            return Err(StoreError::not_found("builtin_secret", id));
        }
        Ok(())
    }
}
