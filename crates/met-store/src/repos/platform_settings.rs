//! Repository for `platform_settings` key-value store.

use met_core::ids::UserId;
use met_core::models::PlatformSetting;
use sqlx::PgPool;

use crate::error::Result;

pub struct PlatformSettingsRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> PlatformSettingsRepo<'a> {
    #[must_use]
    pub const fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn get(&self, key: &str) -> Result<Option<PlatformSetting>> {
        let row = sqlx::query_as::<_, PlatformSetting>(
            r#"SELECT key, value, updated_at, updated_by FROM platform_settings WHERE key = $1"#,
        )
        .bind(key)
        .fetch_optional(self.pool)
        .await?;
        Ok(row)
    }

    pub async fn set(
        &self,
        key: &str,
        value: serde_json::Value,
        updated_by: UserId,
    ) -> Result<PlatformSetting> {
        let row = sqlx::query_as::<_, PlatformSetting>(
            r#"
            INSERT INTO platform_settings (key, value, updated_at, updated_by)
            VALUES ($1, $2, NOW(), $3)
            ON CONFLICT (key)
            DO UPDATE SET value = EXCLUDED.value, updated_at = NOW(), updated_by = EXCLUDED.updated_by
            RETURNING key, value, updated_at, updated_by
            "#,
        )
        .bind(key)
        .bind(value)
        .bind(updated_by.as_uuid())
        .fetch_one(self.pool)
        .await?;
        Ok(row)
    }

    /// Whether unauthenticated access to public resources is allowed.
    pub async fn allow_unauthenticated_access(&self) -> Result<bool> {
        let setting = self.get("allow_unauthenticated_access").await?;
        Ok(setting
            .and_then(|s| s.value.as_bool())
            .unwrap_or(false))
    }
}
