//! Background task: auto-re-import catalog workflows on their configured schedule.
//!
//! Spawned once at API startup. Polls every minute for schedule rows where
//! `next_sync_at <= now() AND enabled = true AND interval_minutes > 0`.
//! For each due row it re-imports the latest YAML from the stored SCM coordinates,
//! then advances `last_synced_at` / `next_sync_at`.

use chrono::Duration;
use met_core::ids::{OrganizationId, ProjectId};
use met_store::PgPool;
use met_store::repos::WorkflowRepo;
use tracing::{error, info, instrument, warn};
use uuid::Uuid;

use crate::github_scm;
use crate::routes::workflows_catalog::ImportCatalogWorkflowGitRequest;
use crate::state::AppState;

/// Row returned by the scheduler query.
#[derive(sqlx::FromRow, Debug)]
struct DueSchedule {
    org_id: Uuid,
    workflow_name: String,
    interval_minutes: i32,
}

/// Spawn the background sync loop. Call once from `main`.
pub fn spawn(state: AppState) {
    tokio::spawn(async move {
        run_loop(&state).await;
    });
}

async fn run_loop(state: &AppState) {
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        interval.tick().await;
        if let Err(e) = process_due_schedules(state).await {
            warn!(error = %e, "workflow sync task error");
        }
    }
}

async fn process_due_schedules(state: &AppState) -> Result<(), Box<dyn std::error::Error>> {
    let due: Vec<DueSchedule> = sqlx::query_as(
        r#"
        SELECT org_id, workflow_name, interval_minutes
        FROM workflow_sync_schedules
        WHERE enabled = true
          AND interval_minutes > 0
          AND next_sync_at <= NOW()
        FOR UPDATE SKIP LOCKED
        LIMIT 20
        "#,
    )
    .fetch_all(state.db())
    .await?;

    if due.is_empty() {
        return Ok(());
    }

    for schedule in due {
        let org_id = OrganizationId::from_uuid(schedule.org_id);
        let workflow_name = schedule.workflow_name.clone();

        match sync_one(state, org_id, &workflow_name).await {
            Ok(count) => {
                info!(
                    org_id = %org_id,
                    workflow_name,
                    versions_imported = count,
                    "auto-synced workflow"
                );
            }
            Err(e) => {
                error!(
                    org_id = %org_id,
                    workflow_name,
                    error = %e,
                    "auto-sync failed for workflow"
                );
            }
        }

        // Advance the schedule regardless of success so a bad workflow doesn't thrash.
        let _ = advance_schedule(
            state.db(),
            schedule.org_id,
            &workflow_name,
            schedule.interval_minutes,
        )
        .await;
    }

    Ok(())
}

#[instrument(skip(state))]
async fn sync_one(
    state: &AppState,
    org_id: OrganizationId,
    workflow_name: &str,
) -> Result<usize, String> {
    let Some(crypto) = state.stored_secret_crypto.as_ref() else {
        return Err("stored-secrets crypto not configured".into());
    };

    // Find all live versions of this workflow that have SCM coordinates.
    let versions = WorkflowRepo::new(state.db())
        .list_global_catalog_versions_with_scm(org_id, workflow_name)
        .await
        .map_err(|e| e.to_string())?;

    if versions.is_empty() {
        return Err(format!(
            "no SCM-sourced versions of '{}' found in the catalog",
            workflow_name
        ));
    }

    let mut imported = 0usize;
    for row in versions {
        let credentials_path = row
            .catalog_metadata
            .get("catalog_scm_credentials_path")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        if credentials_path.is_empty()
            || row.scm_repository.is_none()
            || row.scm_ref.is_none()
            || row.scm_path.is_none()
        {
            continue;
        }

        // Retrieve the user who originally submitted this workflow as the actor.
        let submitter = row
            .submitted_by
            .map(met_core::ids::UserId::from_uuid)
            .unwrap_or_else(|| met_core::ids::UserId::from_uuid(Uuid::nil()));

        let req = ImportCatalogWorkflowGitRequest {
            repository: row.scm_repository.unwrap(),
            git_ref: row.scm_ref.unwrap(),
            workflow_path: row.scm_path.unwrap(),
            credentials_path,
        };

        match crate::routes::workflows_catalog::import_catalog_workflow_git_execute(
            state,
            submitter,
            org_id,
            ProjectId::from_uuid(Uuid::nil()),
            req,
        )
        .await
        {
            Ok(_) => imported += 1,
            Err(e) => {
                warn!(
                    org_id = %org_id,
                    workflow_name,
                    error = %e,
                    "auto-sync: failed to re-import one version"
                );
            }
        }
    }

    Ok(imported)
}

async fn advance_schedule(
    pool: &PgPool,
    org_id: Uuid,
    workflow_name: &str,
    interval_minutes: i32,
) -> Result<(), sqlx::Error> {
    let next = chrono::Utc::now() + Duration::minutes(i64::from(interval_minutes));
    sqlx::query(
        r#"
        UPDATE workflow_sync_schedules
        SET last_synced_at = NOW(),
            next_sync_at   = $1,
            updated_at     = NOW()
        WHERE org_id = $2 AND workflow_name = $3
        "#,
    )
    .bind(next)
    .bind(org_id)
    .bind(workflow_name)
    .execute(pool)
    .await?;
    Ok(())
}
