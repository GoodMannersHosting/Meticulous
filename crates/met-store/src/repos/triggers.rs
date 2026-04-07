//! Pipeline trigger repository (DB triggers table).

use chrono::Utc;
use met_core::ids::{OrganizationId, PipelineId, TriggerId, UserId};
use met_core::models::{CreateTrigger, Trigger, UpdateTrigger};
use serde_json::Value as JsonValue;
use sqlx::PgPool;

use crate::error::{Result, StoreError};

/// Trigger row with creator username (for list UIs).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PipelineTriggerListEntry {
    #[sqlx(flatten)]
    pub trigger: Trigger,
    pub created_by_username: Option<String>,
}

/// Repository for pipeline triggers (`triggers` table).
pub struct TriggerRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> TriggerRepo<'a> {
    /// Create a new trigger repository.
    #[must_use]
    pub const fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Insert a trigger after verifying `pipeline_id` belongs to `org_id`.
    pub async fn insert(
        &self,
        org_id: OrganizationId,
        pipeline_id: PipelineId,
        input: &CreateTrigger,
        enabled: bool,
        created_by_user_id: Option<UserId>,
    ) -> Result<Trigger> {
        let id = TriggerId::new();
        let now = Utc::now();

        let trigger = sqlx::query_as::<_, Trigger>(
            r#"
            INSERT INTO triggers (id, pipeline_id, kind, config, enabled, description, created_by_user_id, created_at, updated_at)
            SELECT $1, $2, $3, $4, $5, $6, $7, $8, $8
            FROM pipelines p
            INNER JOIN projects pr ON pr.id = p.project_id AND pr.org_id = $9 AND pr.deleted_at IS NULL
            WHERE p.id = $2
            RETURNING id, pipeline_id, kind, config, enabled, description, created_by_user_id, created_at, updated_at
            "#,
        )
        .bind(id.as_uuid())
        .bind(pipeline_id.as_uuid())
        .bind(input.kind)
        .bind(&input.config)
        .bind(enabled)
        .bind(&input.description)
        .bind(created_by_user_id.map(|u| u.as_uuid()))
        .bind(now)
        .bind(org_id.as_uuid())
        .fetch_optional(self.pool)
        .await?
        .ok_or_else(|| StoreError::not_found("pipeline", pipeline_id))?;

        Ok(trigger)
    }

    /// Load a trigger when it belongs to a pipeline in `org_id`.
    pub async fn get_for_org(
        &self,
        org_id: OrganizationId,
        trigger_id: TriggerId,
    ) -> Result<Trigger> {
        sqlx::query_as::<_, Trigger>(
            r#"
            SELECT t.id, t.pipeline_id, t.kind, t.config, t.enabled, t.description, t.created_by_user_id, t.created_at, t.updated_at
            FROM triggers t
            INNER JOIN pipelines p ON p.id = t.pipeline_id
            INNER JOIN projects pr ON pr.id = p.project_id
            WHERE t.id = $1 AND pr.org_id = $2 AND pr.deleted_at IS NULL
            "#,
        )
        .bind(trigger_id.as_uuid())
        .bind(org_id.as_uuid())
        .fetch_optional(self.pool)
        .await?
        .ok_or_else(|| StoreError::not_found("trigger", trigger_id))
    }

    /// List triggers for a pipeline scoped to `org_id`.
    pub async fn list_for_pipeline(
        &self,
        org_id: OrganizationId,
        pipeline_id: PipelineId,
    ) -> Result<Vec<PipelineTriggerListEntry>> {
        let rows = sqlx::query_as::<_, PipelineTriggerListEntry>(
            r#"
            SELECT t.id, t.pipeline_id, t.kind, t.config, t.enabled, t.description, t.created_by_user_id, t.created_at, t.updated_at,
                   u.username AS created_by_username
            FROM triggers t
            INNER JOIN pipelines p ON p.id = t.pipeline_id
            INNER JOIN projects pr ON pr.id = p.project_id
            LEFT JOIN users u ON u.id = t.created_by_user_id AND u.deleted_at IS NULL
            WHERE t.pipeline_id = $1 AND pr.org_id = $2 AND pr.deleted_at IS NULL
            ORDER BY t.created_at ASC
            "#,
        )
        .bind(pipeline_id.as_uuid())
        .bind(org_id.as_uuid())
        .fetch_all(self.pool)
        .await?;

        Ok(rows)
    }

    /// Update a trigger; returns not found if the trigger ID exists but not under `org_id`.
    pub async fn update(
        &self,
        org_id: OrganizationId,
        trigger_id: TriggerId,
        patch: &UpdateTrigger,
    ) -> Result<Trigger> {
        let mut current = self.get_for_org(org_id, trigger_id).await?;

        if let Some(enabled) = patch.enabled {
            current.enabled = enabled;
        }
        if let Some(ref description) = patch.description {
            current.description = if description.is_empty() {
                None
            } else {
                Some(description.clone())
            };
        }
        if let Some(ref p) = patch.config_patch {
            merge_json_config(&mut current.config, p);
        }

        let updated = sqlx::query_as::<_, Trigger>(
            r#"
            UPDATE triggers t SET
              kind = $3,
              config = $4,
              enabled = $5,
              description = $6,
              updated_at = NOW()
            FROM pipelines p
            INNER JOIN projects pr ON pr.id = p.project_id
            WHERE t.id = $1 AND t.pipeline_id = p.id AND pr.org_id = $2 AND pr.deleted_at IS NULL
            RETURNING t.id, t.pipeline_id, t.kind, t.config, t.enabled, t.description, t.created_by_user_id, t.created_at, t.updated_at
            "#,
        )
        .bind(trigger_id.as_uuid())
        .bind(org_id.as_uuid())
        .bind(current.kind)
        .bind(&current.config)
        .bind(current.enabled)
        .bind(&current.description)
        .fetch_optional(self.pool)
        .await?
        .ok_or_else(|| StoreError::not_found("trigger", trigger_id))?;

        Ok(updated)
    }

    /// Replace trigger row (after caller merged config). Verifies org scope.
    pub async fn replace_row(&self, org_id: OrganizationId, trigger: &Trigger) -> Result<Trigger> {
        let updated = sqlx::query_as::<_, Trigger>(
            r#"
            UPDATE triggers t SET
              kind = $3,
              config = $4,
              enabled = $5,
              description = $6,
              updated_at = NOW()
            FROM pipelines p
            INNER JOIN projects pr ON pr.id = p.project_id
            WHERE t.id = $1 AND t.pipeline_id = p.id AND pr.org_id = $2 AND pr.deleted_at IS NULL
            RETURNING t.id, t.pipeline_id, t.kind, t.config, t.enabled, t.description, t.created_by_user_id, t.created_at, t.updated_at
            "#,
        )
        .bind(trigger.id.as_uuid())
        .bind(org_id.as_uuid())
        .bind(trigger.kind)
        .bind(&trigger.config)
        .bind(trigger.enabled)
        .bind(&trigger.description)
        .fetch_optional(self.pool)
        .await?
        .ok_or_else(|| StoreError::not_found("trigger", trigger.id))?;

        Ok(updated)
    }

    /// Delete a trigger under `org_id`.
    pub async fn delete(&self, org_id: OrganizationId, trigger_id: TriggerId) -> Result<()> {
        let res = sqlx::query(
            r#"
            DELETE FROM triggers t
            USING pipelines p, projects pr
            WHERE t.id = $1 AND t.pipeline_id = p.id AND p.project_id = pr.id
              AND pr.org_id = $2 AND pr.deleted_at IS NULL
            "#,
        )
        .bind(trigger_id.as_uuid())
        .bind(org_id.as_uuid())
        .execute(self.pool)
        .await?;

        if res.rows_affected() == 0 {
            return Err(StoreError::not_found("trigger", trigger_id));
        }
        Ok(())
    }

    /// Repo-managed webhook for pipeline with the given `sync_key`, if any.
    pub async fn find_repo_managed_by_sync_key(
        &self,
        org_id: OrganizationId,
        pipeline_id: PipelineId,
        sync_key: &str,
    ) -> Result<Option<Trigger>> {
        let row = sqlx::query_as::<_, Trigger>(
            r#"
            SELECT t.id, t.pipeline_id, t.kind, t.config, t.enabled, t.description, t.created_by_user_id, t.created_at, t.updated_at
            FROM triggers t
            INNER JOIN pipelines p ON p.id = t.pipeline_id
            INNER JOIN projects pr ON pr.id = p.project_id
            WHERE t.pipeline_id = $1 AND pr.org_id = $2 AND pr.deleted_at IS NULL
              AND t.kind = 'webhook'::trigger_kind
              AND t.config->>'managed_by' = 'repo'
              AND t.config->>'sync_key' = $3
            "#,
        )
        .bind(pipeline_id.as_uuid())
        .bind(org_id.as_uuid())
        .bind(sync_key)
        .fetch_optional(self.pool)
        .await?;

        Ok(row)
    }

    /// Delete repo-managed webhooks whose `sync_key` is not in `keep_keys`.
    pub async fn delete_repo_managed_not_in_sync_keys(
        &self,
        org_id: OrganizationId,
        pipeline_id: PipelineId,
        keep_keys: &[String],
    ) -> Result<u64> {
        let res = sqlx::query(
            r#"
            DELETE FROM triggers t
            USING pipelines p, projects pr
            WHERE t.pipeline_id = p.id AND p.project_id = pr.id
              AND pr.org_id = $1 AND pr.deleted_at IS NULL
              AND t.pipeline_id = $2
              AND t.kind = 'webhook'::trigger_kind
              AND t.config->>'managed_by' = 'repo'
              AND NOT (t.config->>'sync_key' = ANY($3))
            "#,
        )
        .bind(org_id.as_uuid())
        .bind(pipeline_id.as_uuid())
        .bind(keep_keys)
        .execute(self.pool)
        .await?;

        Ok(res.rows_affected())
    }
}

/// Load a trigger for the public webhook path: org must match pipeline's project org.
pub async fn get_trigger_for_webhook_dispatch(
    pool: &PgPool,
    org_id: OrganizationId,
    trigger_id: TriggerId,
) -> Result<Trigger> {
    TriggerRepo::new(pool).get_for_org(org_id, trigger_id).await
}

fn merge_json_config(base: &mut JsonValue, patch: &JsonValue) {
    match (base, patch) {
        (JsonValue::Object(a), JsonValue::Object(b)) => {
            for (k, v) in b {
                match a.get_mut(k) {
                    Some(existing) if existing.is_object() && v.is_object() => {
                        merge_json_config(existing, v);
                    }
                    Some(_) | None => {
                        a.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        (base, patch) => *base = patch.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn merge_json_config_deep() {
        let mut base = json!({"a": 1, "nested": {"x": 1}, "secret": "keep"});
        let patch = json!({"nested": {"y": 2}, "b": 2});
        merge_json_config(&mut base, &patch);
        assert_eq!(base["a"], 1);
        assert_eq!(base["b"], 2);
        assert_eq!(base["nested"]["x"], 1);
        assert_eq!(base["nested"]["y"], 2);
        assert_eq!(base["secret"], "keep");
    }
}
