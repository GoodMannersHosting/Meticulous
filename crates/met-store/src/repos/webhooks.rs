//! Webhook registrations, deduplicated deliveries (ADR-005), and pipeline targets (ADR-013).
//!
//! Run creation for SCM webhooks is performed in `met-api` via [`crate::pipeline_execution`] so
//! the engine is scheduled consistently with other triggers.

use chrono::{DateTime, Utc};
use met_core::ids::{OrganizationId, PipelineId, ProjectId, TriggerId, UserId};
use sqlx::PgPool;
use std::collections::HashSet;
use uuid::Uuid;

use crate::error::{Result, StoreError};

/// Row from `webhook_reg joined projects` for inbound dispatch.
#[derive(Debug, Clone)]
pub struct WebhookRegistrationContext {
    pub registration_id: Uuid,
    pub project_id: ProjectId,
    pub org_id: OrganizationId,
    pub provider: String,
    pub events: Vec<String>,
    pub active: bool,
    pub secret_verifier: String,
    /// For `provider = generic`: maps JSON body to variables ([`met_core::models::WebhookConfig`] subset, no `secret`).
    pub payload_mapping: serde_json::Value,
    /// For `provider = generic`: `none` (open), `hmac`, or `query` (secret in URL).
    pub generic_inbound_auth: String,
    /// When `generic_inbound_auth == query`: required query parameter name (e.g. `token`).
    pub generic_query_param_name: Option<String>,
}

/// Summary row for listing project webhook registrations (admin UI).
#[derive(Debug, Clone)]
pub struct WebhookRegistrationSummary {
    pub id: Uuid,
    pub provider: String,
    pub events: Vec<String>,
    pub active: bool,
    pub payload_mapping: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub generic_inbound_auth: String,
    pub generic_query_param_name: Option<String>,
    /// Whether `secret_hash` is non-empty (caller-visible secret exists for signing).
    pub secret_configured: bool,
    pub description: Option<String>,
    pub created_by_user_id: Option<UserId>,
    pub created_by_username: Option<String>,
}

/// A routing target row.
#[derive(Debug, Clone)]
pub struct WebhookRegistrationTarget {
    pub id: Uuid,
    pub webhook_registration_id: Uuid,
    pub pipeline_id: PipelineId,
    pub enabled: bool,
    pub filter_config: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct CreateWebhookTarget {
    pub pipeline_id: PipelineId,
    pub enabled: bool,
    pub filter_config: serde_json::Value,
}

#[derive(Debug, Clone, Default)]
pub struct UpdateWebhookTarget {
    pub enabled: Option<bool>,
    pub filter_config: Option<serde_json::Value>,
}

/// ADR-005: insert `webhook_deliveries` row or detect duplicate `(provider, delivery_id)`.
#[derive(Debug)]
pub enum WebhookDeliveryClaim {
    /// New delivery; caller runs pipelines then [`WebhookRepo::set_delivery_run_ids`].
    New,
    Duplicate {
        run_ids: Vec<Uuid>,
    },
}

/// Repository for SCM webhooks and targets.
pub struct WebhookRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> WebhookRepo<'a> {
    #[must_use]
    pub const fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Load registration and owning org for an active webhook id (path segment).
    pub async fn get_registration_context(
        &self,
        registration_id: TriggerId,
    ) -> Result<Option<WebhookRegistrationContext>> {
        let row = sqlx::query_as::<_, (
            Uuid,
            Uuid,
            Uuid,
            String,
            Vec<String>,
            bool,
            String,
            serde_json::Value,
            String,
            Option<String>,
        )>(
            r#"
            SELECT wr.id, wr.project_id, p.org_id, wr.provider, wr.events, wr.active, wr.secret_hash,
                   wr.payload_mapping, wr.generic_inbound_auth, wr.generic_query_param_name
            FROM webhook_registrations wr
            JOIN projects p ON p.id = wr.project_id AND p.deleted_at IS NULL
            WHERE wr.id = $1 AND wr.active = true
            "#,
        )
        .bind(registration_id.as_uuid())
        .fetch_optional(self.pool)
        .await?;

        Ok(row.map(
            |(
                id,
                project_id,
                org_id,
                provider,
                events,
                active,
                secret_verifier,
                payload_mapping,
                gia,
                gqpn,
            )| {
                WebhookRegistrationContext {
                    registration_id: id,
                    project_id: ProjectId::from_uuid(project_id),
                    org_id: OrganizationId::from_uuid(org_id),
                    provider,
                    events,
                    active,
                    secret_verifier,
                    payload_mapping,
                    generic_inbound_auth: gia,
                    generic_query_param_name: gqpn,
                }
            },
        ))
    }

    /// List webhook registrations for a project (any `active`; UI may filter).
    pub async fn list_registrations_for_project(
        &self,
        project_id: ProjectId,
    ) -> Result<Vec<WebhookRegistrationSummary>> {
        let rows = sqlx::query_as::<
            _,
            (
                Uuid,
                String,
                Vec<String>,
                bool,
                serde_json::Value,
                DateTime<Utc>,
                String,
                Option<String>,
                bool,
                Option<String>,
                Option<Uuid>,
                Option<String>,
            ),
        >(
            r#"
            SELECT wr.id, wr.provider, wr.events, wr.active, wr.payload_mapping, wr.created_at,
                   wr.generic_inbound_auth, wr.generic_query_param_name,
                   (wr.secret_hash IS NOT NULL AND wr.secret_hash <> '') AS secret_configured,
                   wr.description, wr.created_by_user_id, u.username AS created_by_username
            FROM webhook_registrations wr
            LEFT JOIN users u ON u.id = wr.created_by_user_id AND u.deleted_at IS NULL
            WHERE wr.project_id = $1
            ORDER BY wr.created_at ASC
            "#,
        )
        .bind(project_id.as_uuid())
        .fetch_all(self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(
                |(
                    id,
                    provider,
                    events,
                    active,
                    payload_mapping,
                    created_at,
                    gia,
                    gqpn,
                    secret_configured,
                    description,
                    created_by_user_id,
                    created_by_username,
                )| {
                    WebhookRegistrationSummary {
                        id,
                        provider,
                        events,
                        active,
                        payload_mapping,
                        created_at,
                        generic_inbound_auth: gia,
                        generic_query_param_name: gqpn,
                        secret_configured,
                        description,
                        created_by_user_id: created_by_user_id.map(UserId::from_uuid),
                        created_by_username,
                    }
                },
            )
            .collect())
    }

    /// One registration summary for `project_id` (admin UI / patch response).
    pub async fn get_registration_summary_for_project(
        &self,
        project_id: ProjectId,
        registration_id: TriggerId,
    ) -> Result<WebhookRegistrationSummary> {
        let row = sqlx::query_as::<
            _,
            (
                Uuid,
                String,
                Vec<String>,
                bool,
                serde_json::Value,
                DateTime<Utc>,
                String,
                Option<String>,
                bool,
                Option<String>,
                Option<Uuid>,
                Option<String>,
            ),
        >(
            r#"
            SELECT wr.id, wr.provider, wr.events, wr.active, wr.payload_mapping, wr.created_at,
                   wr.generic_inbound_auth, wr.generic_query_param_name,
                   (wr.secret_hash IS NOT NULL AND wr.secret_hash <> '') AS secret_configured,
                   wr.description, wr.created_by_user_id, u.username AS created_by_username
            FROM webhook_registrations wr
            LEFT JOIN users u ON u.id = wr.created_by_user_id AND u.deleted_at IS NULL
            WHERE wr.project_id = $1 AND wr.id = $2
            "#,
        )
        .bind(project_id.as_uuid())
        .bind(registration_id.as_uuid())
        .fetch_optional(self.pool)
        .await?
        .ok_or_else(|| StoreError::not_found("webhook_registration", registration_id))?;

        let (
            id,
            provider,
            events,
            active,
            payload_mapping,
            created_at,
            gia,
            gqpn,
            secret_configured,
            description,
            created_by_user_id,
            created_by_username,
        ) = row;

        Ok(WebhookRegistrationSummary {
            id,
            provider,
            events,
            active,
            payload_mapping,
            created_at,
            generic_inbound_auth: gia,
            generic_query_param_name: gqpn,
            secret_configured,
            description,
            created_by_user_id: created_by_user_id.map(UserId::from_uuid),
            created_by_username,
        })
    }

    /// Inbound signing material (stored verifier string), server-side only.
    pub async fn get_secret_hash_for_project_registration(
        &self,
        project_id: ProjectId,
        registration_id: TriggerId,
    ) -> Result<String> {
        let row: Option<(String,)> = sqlx::query_as(
            r#"
            SELECT COALESCE(secret_hash, '') FROM webhook_registrations
            WHERE project_id = $1 AND id = $2
            "#,
        )
        .bind(project_id.as_uuid())
        .bind(registration_id.as_uuid())
        .fetch_optional(self.pool)
        .await?;

        let Some((h,)) = row else {
            return Err(StoreError::not_found(
                "webhook_registration",
                registration_id,
            ));
        };
        Ok(h)
    }

    pub async fn update_generic_inbound_for_project(
        &self,
        project_id: ProjectId,
        registration_id: TriggerId,
        generic_inbound_auth: &str,
        generic_query_param_name: Option<&str>,
        secret_hash: &str,
    ) -> Result<()> {
        let r = sqlx::query(
            r#"
            UPDATE webhook_registrations
            SET generic_inbound_auth = $1,
                generic_query_param_name = $2,
                secret_hash = $3,
                updated_at = NOW()
            WHERE id = $4 AND project_id = $5 AND provider = 'generic'
            "#,
        )
        .bind(generic_inbound_auth)
        .bind(generic_query_param_name)
        .bind(secret_hash)
        .bind(registration_id.as_uuid())
        .bind(project_id.as_uuid())
        .execute(self.pool)
        .await?;

        if r.rows_affected() == 0 {
            return Err(StoreError::not_found(
                "webhook_registration",
                registration_id,
            ));
        }
        Ok(())
    }

    pub async fn update_registration_description(
        &self,
        project_id: ProjectId,
        registration_id: TriggerId,
        description: Option<String>,
    ) -> Result<()> {
        let r = sqlx::query(
            r#"
            UPDATE webhook_registrations
            SET description = $1, updated_at = NOW()
            WHERE id = $2 AND project_id = $3
            "#,
        )
        .bind(&description)
        .bind(registration_id.as_uuid())
        .bind(project_id.as_uuid())
        .execute(self.pool)
        .await?;

        if r.rows_affected() == 0 {
            return Err(StoreError::not_found(
                "webhook_registration",
                registration_id,
            ));
        }
        Ok(())
    }

    /// Replace inbound secret material (hex form stored in `secret_hash`, per setup/verify).
    pub async fn update_registration_secret_hash(
        &self,
        project_id: ProjectId,
        registration_id: TriggerId,
        secret_hash: &str,
    ) -> Result<()> {
        let r = sqlx::query(
            r#"
            UPDATE webhook_registrations
            SET secret_hash = $1, updated_at = NOW()
            WHERE id = $2 AND project_id = $3
            "#,
        )
        .bind(secret_hash)
        .bind(registration_id.as_uuid())
        .bind(project_id.as_uuid())
        .execute(self.pool)
        .await?;

        if r.rows_affected() == 0 {
            return Err(StoreError::not_found(
                "webhook_registration",
                registration_id,
            ));
        }
        Ok(())
    }

    /// Generic webhooks: disable verification (open URL). SCM rows: only clear `secret_hash` (handlers skip verify when empty).
    pub async fn clear_registration_inbound_secret(
        &self,
        project_id: ProjectId,
        registration_id: TriggerId,
        provider: &str,
    ) -> Result<()> {
        let r = if provider.eq_ignore_ascii_case("generic") {
            sqlx::query(
                r#"
                UPDATE webhook_registrations
                SET secret_hash = '',
                    generic_inbound_auth = 'none',
                    generic_query_param_name = NULL,
                    updated_at = NOW()
                WHERE id = $1 AND project_id = $2 AND provider = 'generic'
                "#,
            )
            .bind(registration_id.as_uuid())
            .bind(project_id.as_uuid())
            .execute(self.pool)
            .await?
        } else {
            sqlx::query(
                r#"
                UPDATE webhook_registrations
                SET secret_hash = '', updated_at = NOW()
                WHERE id = $1 AND project_id = $2
                "#,
            )
            .bind(registration_id.as_uuid())
            .bind(project_id.as_uuid())
            .execute(self.pool)
            .await?
        };

        if r.rows_affected() == 0 {
            return Err(StoreError::not_found(
                "webhook_registration",
                registration_id,
            ));
        }
        Ok(())
    }

    /// Replace targets with exactly `pipeline_ids` (must belong to `project_id`). Idempotent inserts use empty `filter_config`.
    pub async fn sync_registration_targets(
        &self,
        project_id: ProjectId,
        registration_id: TriggerId,
        pipeline_ids: &[PipelineId],
    ) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        let reg_ok: Option<(bool,)> = sqlx::query_as(
            r#"
            SELECT TRUE FROM webhook_registrations WHERE id = $1 AND project_id = $2
            "#,
        )
        .bind(registration_id.as_uuid())
        .bind(project_id.as_uuid())
        .fetch_optional(&mut *tx)
        .await?;
        if reg_ok.is_none() {
            return Err(StoreError::not_found(
                "webhook_registration",
                registration_id,
            ));
        }

        let want: HashSet<Uuid> = pipeline_ids.iter().map(|p| p.as_uuid()).collect();

        for pid in pipeline_ids {
            let ok: Option<(Uuid,)> = sqlx::query_as(
                r#"
                SELECT p.id
                FROM pipelines p
                INNER JOIN projects pr ON pr.id = p.project_id AND pr.deleted_at IS NULL
                WHERE p.id = $1 AND p.project_id = $2
                "#,
            )
            .bind(pid.as_uuid())
            .bind(project_id.as_uuid())
            .fetch_optional(&mut *tx)
            .await?;
            if ok.is_none() {
                return Err(StoreError::Validation(format!(
                    "pipeline {} is not in this project",
                    pid
                )));
            }
        }

        let current: Vec<(Uuid, Uuid)> = sqlx::query_as(
            r#"
            SELECT id, pipeline_id
            FROM webhook_registration_targets
            WHERE webhook_registration_id = $1
            "#,
        )
        .bind(registration_id.as_uuid())
        .fetch_all(&mut *tx)
        .await?;

        let have_pipelines: HashSet<Uuid> = current.iter().map(|(_, p)| *p).collect();

        for (target_id, puuid) in current {
            if !want.contains(&puuid) {
                sqlx::query(r#"DELETE FROM webhook_registration_targets WHERE id = $1"#)
                    .bind(target_id)
                    .execute(&mut *tx)
                    .await?;
            }
        }

        for pid in pipeline_ids {
            if !have_pipelines.contains(&pid.as_uuid()) {
                sqlx::query(
                    r#"
                    INSERT INTO webhook_registration_targets
                        (webhook_registration_id, pipeline_id, enabled, filter_config)
                    VALUES ($1, $2, true, '{}'::jsonb)
                    "#,
                )
                .bind(registration_id.as_uuid())
                .bind(pid.as_uuid())
                .execute(&mut *tx)
                .await
                .map_err(|e| {
                    if let sqlx::Error::Database(ref db) = e {
                        if db.code().as_deref() == Some("23505") {
                            return StoreError::Validation(
                                "target for this pipeline already exists for this webhook"
                                    .to_string(),
                            );
                        }
                    }
                    e.into()
                })?;
            }
        }

        tx.commit().await?;
        Ok(())
    }

    /// Ensure the registration belongs to `project_id` (for admin routes).
    pub async fn assert_registration_in_project(
        &self,
        project_id: ProjectId,
        registration_id: TriggerId,
    ) -> Result<()> {
        let ok: Option<(bool,)> = sqlx::query_as(
            r#"
            SELECT TRUE
            FROM webhook_registrations
            WHERE id = $1 AND project_id = $2
            "#,
        )
        .bind(registration_id.as_uuid())
        .bind(project_id.as_uuid())
        .fetch_optional(self.pool)
        .await?;

        if ok.is_none() {
            return Err(StoreError::not_found(
                "webhook_registration",
                registration_id,
            ));
        }
        Ok(())
    }

    /// Delete a webhook registration and cascaded targets/deliveries.
    pub async fn delete_registration(
        &self,
        project_id: ProjectId,
        registration_id: TriggerId,
    ) -> Result<()> {
        let r = sqlx::query(
            r#"
            DELETE FROM webhook_registrations
            WHERE id = $1 AND project_id = $2
            "#,
        )
        .bind(registration_id.as_uuid())
        .bind(project_id.as_uuid())
        .execute(self.pool)
        .await?;

        if r.rows_affected() == 0 {
            return Err(StoreError::not_found(
                "webhook_registration",
                registration_id,
            ));
        }
        Ok(())
    }

    pub async fn list_targets(
        &self,
        registration_id: TriggerId,
    ) -> Result<Vec<WebhookRegistrationTarget>> {
        let rows = sqlx::query_as::<_, (Uuid, Uuid, Uuid, bool, serde_json::Value)>(
            r#"
            SELECT id, webhook_registration_id, pipeline_id, enabled, filter_config
            FROM webhook_registration_targets
            WHERE webhook_registration_id = $1
            ORDER BY created_at ASC
            "#,
        )
        .bind(registration_id.as_uuid())
        .fetch_all(self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(id, wrid, pid, enabled, fc)| WebhookRegistrationTarget {
                id,
                webhook_registration_id: wrid,
                pipeline_id: PipelineId::from_uuid(pid),
                enabled,
                filter_config: fc,
            })
            .collect())
    }

    /// Insert target; `pipeline` must belong to the registration's project (caller validates).
    pub async fn insert_target(
        &self,
        registration_id: TriggerId,
        input: &CreateWebhookTarget,
    ) -> Result<WebhookRegistrationTarget> {
        let row = sqlx::query_as::<_, (Uuid, Uuid, Uuid, bool, serde_json::Value)>(
            r#"
            INSERT INTO webhook_registration_targets
                (webhook_registration_id, pipeline_id, enabled, filter_config)
            VALUES ($1, $2, $3, $4)
            RETURNING id, webhook_registration_id, pipeline_id, enabled, filter_config
            "#,
        )
        .bind(registration_id.as_uuid())
        .bind(input.pipeline_id.as_uuid())
        .bind(input.enabled)
        .bind(&input.filter_config)
        .fetch_one(self.pool)
        .await
        .map_err(|e| {
            if let sqlx::Error::Database(ref db) = e {
                if db.code().as_deref() == Some("23505") {
                    return StoreError::Validation(
                        "target for this pipeline already exists for this webhook".to_string(),
                    );
                }
            }
            e.into()
        })?;

        let (id, wrid, pid, enabled, filter_config) = row;
        Ok(WebhookRegistrationTarget {
            id,
            webhook_registration_id: wrid,
            pipeline_id: PipelineId::from_uuid(pid),
            enabled,
            filter_config,
        })
    }

    pub async fn update_target(
        &self,
        target_id: Uuid,
        registration_id: TriggerId,
        input: &UpdateWebhookTarget,
    ) -> Result<WebhookRegistrationTarget> {
        let existing = sqlx::query_as::<_, (Uuid, Uuid, Uuid, bool, serde_json::Value)>(
            r#"
            SELECT id, webhook_registration_id, pipeline_id, enabled, filter_config
            FROM webhook_registration_targets
            WHERE id = $1 AND webhook_registration_id = $2
            "#,
        )
        .bind(target_id)
        .bind(registration_id.as_uuid())
        .fetch_optional(self.pool)
        .await?
        .ok_or_else(|| StoreError::not_found("webhook_registration_target", target_id))?;

        let enabled = input.enabled.unwrap_or(existing.3);
        let filter_config = input
            .filter_config
            .clone()
            .unwrap_or_else(|| existing.4.clone());

        let row = sqlx::query_as::<_, (Uuid, Uuid, Uuid, bool, serde_json::Value)>(
            r#"
            UPDATE webhook_registration_targets
            SET enabled = $2, filter_config = $3, updated_at = NOW()
            WHERE id = $1
            RETURNING id, webhook_registration_id, pipeline_id, enabled, filter_config
            "#,
        )
        .bind(target_id)
        .bind(enabled)
        .bind(&filter_config)
        .fetch_one(self.pool)
        .await?;

        let (id, wrid, pid, en, fc) = row;
        Ok(WebhookRegistrationTarget {
            id,
            webhook_registration_id: wrid,
            pipeline_id: PipelineId::from_uuid(pid),
            enabled: en,
            filter_config: fc,
        })
    }

    pub async fn delete_target(&self, target_id: Uuid, registration_id: TriggerId) -> Result<()> {
        let r = sqlx::query(
            r#"
            DELETE FROM webhook_registration_targets
            WHERE id = $1 AND webhook_registration_id = $2
            "#,
        )
        .bind(target_id)
        .bind(registration_id.as_uuid())
        .execute(self.pool)
        .await?;

        if r.rows_affected() == 0 {
            return Err(StoreError::not_found(
                "webhook_registration_target",
                target_id,
            ));
        }
        Ok(())
    }

    /// [`WebhookDeliveryClaim::New`] if this delivery was claimed; duplicate otherwise.
    pub async fn claim_webhook_delivery(
        &self,
        provider: &str,
        delivery_id: &str,
        registration_id: TriggerId,
    ) -> Result<WebhookDeliveryClaim> {
        let inserted = sqlx::query_scalar::<_, Uuid>(
            r#"
            INSERT INTO webhook_deliveries (provider, delivery_id, registration_id)
            VALUES ($1, $2, $3)
            ON CONFLICT (provider, delivery_id) DO NOTHING
            RETURNING id
            "#,
        )
        .bind(provider)
        .bind(delivery_id)
        .bind(registration_id.as_uuid())
        .fetch_optional(self.pool)
        .await?;

        if inserted.is_some() {
            return Ok(WebhookDeliveryClaim::New);
        }

        let run_ids = self.get_delivery_run_ids(provider, delivery_id).await?;
        Ok(WebhookDeliveryClaim::Duplicate { run_ids })
    }

    pub async fn get_delivery_run_ids(
        &self,
        provider: &str,
        delivery_id: &str,
    ) -> Result<Vec<Uuid>> {
        let run_ids: Vec<Uuid> = sqlx::query_scalar(
            r#"
            SELECT run_ids FROM webhook_deliveries
            WHERE provider = $1 AND delivery_id = $2
            "#,
        )
        .bind(provider)
        .bind(delivery_id)
        .fetch_one(self.pool)
        .await?;

        Ok(run_ids)
    }

    pub async fn set_delivery_run_ids(
        &self,
        provider: &str,
        delivery_id: &str,
        run_ids: &[Uuid],
    ) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE webhook_deliveries
            SET run_ids = $1
            WHERE provider = $2 AND delivery_id = $3
            "#,
        )
        .bind(run_ids)
        .bind(provider)
        .bind(delivery_id)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    pub fn target_requires_branch(filter_config: &serde_json::Value) -> bool {
        let Some(arr) = filter_config.get("branches").and_then(|v| v.as_array()) else {
            return false;
        };
        !arr.is_empty()
    }

    pub fn target_branch_allows(filter_config: &serde_json::Value, branch: &str) -> bool {
        let Some(arr) = filter_config.get("branches").and_then(|v| v.as_array()) else {
            return true;
        };
        if arr.is_empty() {
            return true;
        }
        for v in arr {
            let Some(pat) = v.as_str() else { continue };
            if Self::branch_pattern_matches(pat, branch) {
                return true;
            }
        }
        false
    }

    pub fn branch_pattern_matches(pattern: &str, branch: &str) -> bool {
        if pattern == "*" {
            return true;
        }
        if pattern.ends_with('*') && pattern.len() > 1 {
            let prefix = &pattern[..pattern.len() - 1];
            branch.starts_with(prefix)
        } else {
            pattern == branch
        }
    }

    pub fn target_event_allows(filter_config: &serde_json::Value, event_type: &str) -> bool {
        let Some(arr) = filter_config.get("events").and_then(|v| v.as_array()) else {
            return true;
        };
        if arr.is_empty() {
            return true;
        }
        arr.iter()
            .filter_map(|v| v.as_str())
            .any(|e| e == event_type)
    }
}

#[cfg(test)]
mod tests {
    use super::WebhookRepo;
    use serde_json::json;

    #[test]
    fn branch_filter_glob_and_exact() {
        let cfg = json!({ "branches": ["main", "feat/*"] });
        assert!(WebhookRepo::target_branch_allows(&cfg, "main"));
        assert!(WebhookRepo::target_branch_allows(&cfg, "feat/foo"));
        assert!(!WebhookRepo::target_branch_allows(&cfg, "release"));
        assert!(WebhookRepo::branch_pattern_matches("*", "anything"));
    }

    #[test]
    fn event_override_in_filter_config() {
        let cfg = json!({ "events": ["push"] });
        assert!(WebhookRepo::target_event_allows(&cfg, "push"));
        assert!(!WebhookRepo::target_event_allows(&cfg, "pull_request"));
        let cfg_empty = json!({ "events": [] });
        assert!(WebhookRepo::target_event_allows(&cfg_empty, "pull_request"));
    }
}
