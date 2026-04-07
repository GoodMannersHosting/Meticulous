//! Reconcile declarative webhook triggers from parsed pipeline IR into `triggers` rows.

use met_core::ids::{OrganizationId, PipelineId};
use met_core::models::{CreateTrigger, TriggerKind, WebhookConfig};
use met_parser::ir::{PipelineIR, Trigger as IrTrigger, WebhookEvent, WebhookTrigger};
use met_store::error::Result;
use met_store::repos::TriggerRepo;
use sqlx::PgPool;

/// Upsert repo-managed webhook triggers from IR and delete YAML-removed rows.
pub async fn reconcile_repo_webhook_triggers(
    pool: &PgPool,
    org_id: OrganizationId,
    pipeline_id: PipelineId,
    ir: &PipelineIR,
) -> Result<()> {
    let repo = TriggerRepo::new(pool);
    let mut keep = Vec::new();
    for t in &ir.triggers {
        if let IrTrigger::Webhook(w) = t {
            if let Some(sk) = w.sync_key.as_ref().filter(|s| !s.is_empty()) {
                keep.push(sk.clone());
                upsert_repo_webhook(pool, org_id, pipeline_id, w, sk).await?;
            }
        }
    }
    repo.delete_repo_managed_not_in_sync_keys(org_id, pipeline_id, &keep)
        .await?;
    Ok(())
}

async fn upsert_repo_webhook(
    pool: &PgPool,
    org_id: OrganizationId,
    pipeline_id: PipelineId,
    w: &WebhookTrigger,
    sync_key: &str,
) -> Result<()> {
    let repo = TriggerRepo::new(pool);
    let mut config = build_repo_webhook_config(w, sync_key);

    if let Some(mut existing) = repo
        .find_repo_managed_by_sync_key(org_id, pipeline_id, sync_key)
        .await?
    {
        let prev: WebhookConfig =
            serde_json::from_value(existing.config.clone()).unwrap_or_default();
        config.secret = prev.secret;
        existing.config = serde_json::to_value(&config).map_err(|e| {
            met_store::StoreError::Validation(format!("webhook config serialization: {e}"))
        })?;
        repo.replace_row(org_id, &existing).await?;
    } else {
        let input = CreateTrigger {
            kind: TriggerKind::Webhook,
            config: serde_json::to_value(&config).map_err(|e| {
                met_store::StoreError::Validation(format!("webhook config serialization: {e}"))
            })?,
            description: None,
        };
        repo.insert(org_id, pipeline_id, &input, true, None).await?;
    }
    Ok(())
}

fn build_repo_webhook_config(w: &WebhookTrigger, sync_key: &str) -> WebhookConfig {
    let mut config = WebhookConfig::default();
    config.branches = w.branches.clone();
    config.paths = w.paths.clone();
    config.paths_ignore = w.paths_ignore.clone();
    config.events = w
        .events
        .iter()
        .map(|e| event_as_str(e).to_string())
        .collect();
    config.flatten_top_level = true;
    config.include_raw_body_variable = None;
    config.sync_key = Some(sync_key.to_string());
    config.managed_by = Some("repo".to_string());
    config.secret = None;
    config
}

fn event_as_str(e: &WebhookEvent) -> &'static str {
    match e {
        WebhookEvent::Push => "push",
        WebhookEvent::PullRequest => "pull_request",
        WebhookEvent::PullRequestReview => "pull_request_review",
        WebhookEvent::PullRequestComment => "pull_request_comment",
        WebhookEvent::Release => "release",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use met_parser::ir::WebhookEvent;

    #[test]
    fn build_config_from_ir() {
        let w = WebhookTrigger {
            events: vec![WebhookEvent::Push],
            branches: vec!["main".into()],
            paths: vec!["src/**".into()],
            paths_ignore: vec!["*.md".into()],
            sync_key: Some("k1".into()),
        };
        let c = build_repo_webhook_config(&w, "k1");
        assert_eq!(c.sync_key.as_deref(), Some("k1"));
        assert_eq!(c.managed_by.as_deref(), Some("repo"));
        assert!(c.secret.is_none());
        assert_eq!(c.branches, vec!["main"]);
        assert_eq!(c.events, vec!["push"]);
        assert_eq!(c.paths_ignore, vec!["*.md"]);
    }
}
