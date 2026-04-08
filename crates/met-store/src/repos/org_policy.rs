//! Per-organization policy (token TTL cap, rate-limit tunables).

use met_core::ids::OrganizationId;
use sqlx::PgPool;

use crate::error::Result;

/// Row in `org_policy`.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct OrgPolicy {
    pub org_id: uuid::Uuid,
    pub max_api_token_ttl_days: i32,
    pub user_rl_primary_period_secs: i32,
    pub user_rl_primary_max: i32,
    pub user_rl_secondary_period_secs: i32,
    pub user_rl_secondary_max: i32,
    pub app_rl_primary_period_secs: i32,
    pub app_rl_primary_max: i32,
    pub app_rl_secondary_period_secs: i32,
    pub app_rl_secondary_max: i32,
}

pub struct OrgPolicyRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> OrgPolicyRepo<'a> {
    #[must_use]
    pub const fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn get(&self, org_id: OrganizationId) -> Result<OrgPolicy> {
        let row = sqlx::query_as::<_, OrgPolicy>(
            r#"
            SELECT org_id, max_api_token_ttl_days,
                   user_rl_primary_period_secs, user_rl_primary_max,
                   user_rl_secondary_period_secs, user_rl_secondary_max,
                   app_rl_primary_period_secs, app_rl_primary_max,
                   app_rl_secondary_period_secs, app_rl_secondary_max
            FROM org_policy
            WHERE org_id = $1
            "#,
        )
        .bind(org_id.as_uuid())
        .fetch_optional(self.pool)
        .await?;

        Ok(row.unwrap_or_else(|| OrgPolicy {
            org_id: org_id.as_uuid(),
            max_api_token_ttl_days: 365,
            user_rl_primary_period_secs: 3600,
            user_rl_primary_max: 15000,
            user_rl_secondary_period_secs: 10,
            user_rl_secondary_max: 60,
            app_rl_primary_period_secs: 3600,
            app_rl_primary_max: 15000,
            app_rl_secondary_period_secs: 10,
            app_rl_secondary_max: 60,
        }))
    }

    pub async fn upsert(&self, org_id: OrganizationId, patch: &OrgPolicyPatch) -> Result<OrgPolicy> {
        let cur = self.get(org_id).await?;
        let max_api_token_ttl_days = patch
            .max_api_token_ttl_days
            .unwrap_or(cur.max_api_token_ttl_days)
            .clamp(1, 3650);
        let user_rl_primary_period_secs = patch
            .user_rl_primary_period_secs
            .unwrap_or(cur.user_rl_primary_period_secs)
            .max(1);
        let user_rl_primary_max = patch.user_rl_primary_max.unwrap_or(cur.user_rl_primary_max).max(1);
        let user_rl_secondary_period_secs = patch
            .user_rl_secondary_period_secs
            .unwrap_or(cur.user_rl_secondary_period_secs)
            .max(1);
        let user_rl_secondary_max = patch
            .user_rl_secondary_max
            .unwrap_or(cur.user_rl_secondary_max)
            .max(1);
        let app_rl_primary_period_secs = patch
            .app_rl_primary_period_secs
            .unwrap_or(cur.app_rl_primary_period_secs)
            .max(1);
        let app_rl_primary_max = patch.app_rl_primary_max.unwrap_or(cur.app_rl_primary_max).max(1);
        let app_rl_secondary_period_secs = patch
            .app_rl_secondary_period_secs
            .unwrap_or(cur.app_rl_secondary_period_secs)
            .max(1);
        let app_rl_secondary_max = patch
            .app_rl_secondary_max
            .unwrap_or(cur.app_rl_secondary_max)
            .max(1);

        let row = sqlx::query_as::<_, OrgPolicy>(
            r#"
            INSERT INTO org_policy (
                org_id, max_api_token_ttl_days,
                user_rl_primary_period_secs, user_rl_primary_max,
                user_rl_secondary_period_secs, user_rl_secondary_max,
                app_rl_primary_period_secs, app_rl_primary_max,
                app_rl_secondary_period_secs, app_rl_secondary_max
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            ON CONFLICT (org_id) DO UPDATE SET
                max_api_token_ttl_days = EXCLUDED.max_api_token_ttl_days,
                user_rl_primary_period_secs = EXCLUDED.user_rl_primary_period_secs,
                user_rl_primary_max = EXCLUDED.user_rl_primary_max,
                user_rl_secondary_period_secs = EXCLUDED.user_rl_secondary_period_secs,
                user_rl_secondary_max = EXCLUDED.user_rl_secondary_max,
                app_rl_primary_period_secs = EXCLUDED.app_rl_primary_period_secs,
                app_rl_primary_max = EXCLUDED.app_rl_primary_max,
                app_rl_secondary_period_secs = EXCLUDED.app_rl_secondary_period_secs,
                app_rl_secondary_max = EXCLUDED.app_rl_secondary_max,
                updated_at = NOW()
            RETURNING org_id, max_api_token_ttl_days,
                   user_rl_primary_period_secs, user_rl_primary_max,
                   user_rl_secondary_period_secs, user_rl_secondary_max,
                   app_rl_primary_period_secs, app_rl_primary_max,
                   app_rl_secondary_period_secs, app_rl_secondary_max
            "#,
        )
        .bind(org_id.as_uuid())
        .bind(max_api_token_ttl_days)
        .bind(user_rl_primary_period_secs)
        .bind(user_rl_primary_max)
        .bind(user_rl_secondary_period_secs)
        .bind(user_rl_secondary_max)
        .bind(app_rl_primary_period_secs)
        .bind(app_rl_primary_max)
        .bind(app_rl_secondary_period_secs)
        .bind(app_rl_secondary_max)
        .fetch_one(self.pool)
        .await?;

        Ok(row)
    }
}

/// Partial update for [`OrgPolicyRepo::upsert`].
#[derive(Debug, Default)]
pub struct OrgPolicyPatch {
    pub max_api_token_ttl_days: Option<i32>,
    pub user_rl_primary_period_secs: Option<i32>,
    pub user_rl_primary_max: Option<i32>,
    pub user_rl_secondary_period_secs: Option<i32>,
    pub user_rl_secondary_max: Option<i32>,
    pub app_rl_primary_period_secs: Option<i32>,
    pub app_rl_primary_max: Option<i32>,
    pub app_rl_secondary_period_secs: Option<i32>,
    pub app_rl_secondary_max: Option<i32>,
}
