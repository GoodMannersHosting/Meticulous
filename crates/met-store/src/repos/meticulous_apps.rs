//! Meticulous App repository (integrations + installations).

use chrono::Utc;
use met_core::ids::{AppInstallationId, AppKeyId, MeticulousAppId, ProjectId, UserId};
use met_core::models::{MeticulousApp, MeticulousAppInstallation, MeticulousAppKey};
use sqlx::PgPool;

use crate::error::{Result, StoreError};

const APP_ROW: &str = "id, application_id, name, description, created_by, created_at, updated_at";
const KEY_ROW: &str = "id, app_id, key_id, public_key_pem, created_at, revoked_at";
const INSTALL_ROW: &str = "id, app_id, project_id, permissions, revoked_at, created_at";

/// Repository for Meticulous Apps.
pub struct MeticulousAppRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> MeticulousAppRepo<'a> {
    #[must_use]
    pub const fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn create_app_with_initial_key(
        &self,
        name: &str,
        description: Option<&str>,
        created_by: UserId,
        key_id: &str,
        public_key_pem: &str,
    ) -> Result<(MeticulousApp, MeticulousAppKey)> {
        let mut tx = self.pool.begin().await?;
        let app_id = MeticulousAppId::new();
        let application_id = app_id.to_string();
        let now = Utc::now();

        let app = sqlx::query_as::<_, MeticulousApp>(&format!(
            r#"
            INSERT INTO meticulous_apps (id, application_id, name, description, created_by, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $6)
            RETURNING {APP_ROW}
            "#
        ))
        .bind(app_id.as_uuid())
        .bind(&application_id)
        .bind(name)
        .bind(description)
        .bind(created_by.as_uuid())
        .bind(now)
        .fetch_one(&mut *tx)
        .await?;

        let key_row_id = AppKeyId::new();
        let key = sqlx::query_as::<_, MeticulousAppKey>(&format!(
            r#"
            INSERT INTO meticulous_app_keys (id, app_id, key_id, public_key_pem, created_at)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING {KEY_ROW}
            "#
        ))
        .bind(key_row_id.as_uuid())
        .bind(app_id.as_uuid())
        .bind(key_id)
        .bind(public_key_pem)
        .bind(now)
        .fetch_one(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok((app, key))
    }

    pub async fn list_apps(&self) -> Result<Vec<MeticulousApp>> {
        sqlx::query_as::<_, MeticulousApp>(&format!(
            "SELECT {APP_ROW} FROM meticulous_apps ORDER BY created_at DESC"
        ))
        .fetch_all(self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn get_by_application_id(&self, application_id: &str) -> Result<MeticulousApp> {
        sqlx::query_as::<_, MeticulousApp>(&format!(
            "SELECT {APP_ROW} FROM meticulous_apps WHERE application_id = $1"
        ))
        .bind(application_id)
        .fetch_optional(self.pool)
        .await?
        .ok_or_else(|| StoreError::not_found("meticulous_app", application_id))
    }

    pub async fn get_by_id(&self, id: MeticulousAppId) -> Result<MeticulousApp> {
        sqlx::query_as::<_, MeticulousApp>(&format!(
            "SELECT {APP_ROW} FROM meticulous_apps WHERE id = $1"
        ))
        .bind(id.as_uuid())
        .fetch_optional(self.pool)
        .await?
        .ok_or_else(|| StoreError::not_found("meticulous_app", id))
    }

    pub async fn add_key(
        &self,
        app_id: MeticulousAppId,
        key_id: &str,
        public_key_pem: &str,
    ) -> Result<MeticulousAppKey> {
        let key_row_id = AppKeyId::new();
        let now = Utc::now();
        sqlx::query_as::<_, MeticulousAppKey>(&format!(
            r#"
            INSERT INTO meticulous_app_keys (id, app_id, key_id, public_key_pem, created_at)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING {KEY_ROW}
            "#
        ))
        .bind(key_row_id.as_uuid())
        .bind(app_id.as_uuid())
        .bind(key_id)
        .bind(public_key_pem)
        .bind(now)
        .fetch_one(self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn revoke_key(&self, app_id: MeticulousAppId, key_id: &str) -> Result<u64> {
        let res = sqlx::query(
            r#"
            UPDATE meticulous_app_keys
            SET revoked_at = NOW()
            WHERE app_id = $1 AND key_id = $2 AND revoked_at IS NULL
            "#,
        )
        .bind(app_id.as_uuid())
        .bind(key_id)
        .execute(self.pool)
        .await?;
        Ok(res.rows_affected())
    }

    pub async fn list_active_keys(&self, app_id: MeticulousAppId) -> Result<Vec<MeticulousAppKey>> {
        sqlx::query_as::<_, MeticulousAppKey>(&format!(
            "SELECT {KEY_ROW} FROM meticulous_app_keys WHERE app_id = $1 AND revoked_at IS NULL ORDER BY created_at ASC"
        ))
        .bind(app_id.as_uuid())
        .fetch_all(self.pool)
        .await
        .map_err(Into::into)
    }

    /// Lookup public key material for JWT verification (`iss` = application_id, `kid` = key_id).
    pub async fn get_active_public_key_pem(
        &self,
        application_id: &str,
        jwt_key_id: &str,
    ) -> Result<(MeticulousAppId, String)> {
        let row: Option<(uuid::Uuid, String)> = sqlx::query_as(
            r#"
            SELECT a.id, k.public_key_pem
            FROM meticulous_apps a
            JOIN meticulous_app_keys k ON k.app_id = a.id AND k.revoked_at IS NULL
            WHERE a.application_id = $1 AND k.key_id = $2
            "#,
        )
        .bind(application_id)
        .bind(jwt_key_id)
        .fetch_optional(self.pool)
        .await?;

        let Some((app_uuid, pem)) = row else {
            return Err(StoreError::not_found("meticulous_app_key", jwt_key_id));
        };

        Ok((MeticulousAppId::from_uuid(app_uuid), pem))
    }

    pub async fn create_installation(
        &self,
        app_id: MeticulousAppId,
        project_id: ProjectId,
        permissions: &[String],
    ) -> Result<MeticulousAppInstallation> {
        let id = AppInstallationId::new();
        let now = Utc::now();
        sqlx::query_as::<_, MeticulousAppInstallation>(&format!(
            r#"
            INSERT INTO meticulous_app_installations (id, app_id, project_id, permissions, created_at)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING {INSTALL_ROW}
            "#
        ))
        .bind(id.as_uuid())
        .bind(app_id.as_uuid())
        .bind(project_id.as_uuid())
        .bind(permissions)
        .bind(now)
        .fetch_one(self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn list_installations_for_app(
        &self,
        app_id: MeticulousAppId,
    ) -> Result<Vec<MeticulousAppInstallation>> {
        sqlx::query_as::<_, MeticulousAppInstallation>(&format!(
            "SELECT {INSTALL_ROW} FROM meticulous_app_installations WHERE app_id = $1 ORDER BY created_at DESC"
        ))
        .bind(app_id.as_uuid())
        .fetch_all(self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn get_installation(
        &self,
        id: AppInstallationId,
    ) -> Result<MeticulousAppInstallation> {
        sqlx::query_as::<_, MeticulousAppInstallation>(&format!(
            "SELECT {INSTALL_ROW} FROM meticulous_app_installations WHERE id = $1"
        ))
        .bind(id.as_uuid())
        .fetch_optional(self.pool)
        .await?
        .ok_or_else(|| StoreError::not_found("app_installation", id))
    }

    pub async fn revoke_installation(&self, id: AppInstallationId) -> Result<()> {
        let res = sqlx::query(
            r#"
            UPDATE meticulous_app_installations
            SET revoked_at = NOW()
            WHERE id = $1 AND revoked_at IS NULL
            "#,
        )
        .bind(id.as_uuid())
        .execute(self.pool)
        .await?;
        if res.rows_affected() == 0 {
            return Err(StoreError::not_found("app_installation", id));
        }
        Ok(())
    }

    pub async fn get_active_installation_for_app_project(
        &self,
        app_id: MeticulousAppId,
        project_id: ProjectId,
    ) -> Result<MeticulousAppInstallation> {
        sqlx::query_as::<_, MeticulousAppInstallation>(&format!(
            r#"
            SELECT {INSTALL_ROW}
            FROM meticulous_app_installations
            WHERE app_id = $1 AND project_id = $2 AND revoked_at IS NULL
            "#
        ))
        .bind(app_id.as_uuid())
        .bind(project_id.as_uuid())
        .fetch_optional(self.pool)
        .await?
        .ok_or_else(|| {
            StoreError::not_found(
                "app_installation",
                format!("app={app_id} project={project_id}"),
            )
        })
    }
}
