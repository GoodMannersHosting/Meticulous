//! Background task: periodically purge old data according to retention policy.
//!
//! Two independent cleanup loops share a single tick interval (every 5 minutes):
//!
//! 1. **Heartbeat GC** — deletes `agent_heartbeats` rows older than
//!    `platform_settings.heartbeat_retention_hours` (default 48 h).  Skipped when the
//!    setting is 0.
//!
//! 2. **Run GC** — for each active project, resolves its effective retention window
//!    (project-level `run_retention_days` override, or the global platform default).
//!    Deletes terminal runs (succeeded / failed / cancelled) older than the window in
//!    batches of 500 rows per tick.  Only terminal runs are touched so that in-flight
//!    executions are never disrupted.
//!
//! Both phases are best-effort: failures are logged as warnings and the task continues
//! on the next tick.

use chrono::{Duration, Utc};
use met_store::repos::{AgentHeartbeatRepo, PlatformSettingsRepo, ProjectRepo, RunRepo};
use tracing::{info, instrument, warn};

use crate::state::AppState;

const POLL_INTERVAL_SECS: u64 = 300; // 5 minutes
const RUN_DELETE_BATCH_SIZE: i64 = 500;

/// Spawn the data-retention loop.  Call once from `main` after the app state is ready.
pub fn spawn(state: AppState) {
    tokio::spawn(async move {
        run_loop(&state).await;
    });
}

async fn run_loop(state: &AppState) {
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(POLL_INTERVAL_SECS));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        interval.tick().await;
        gc_heartbeats(state).await;
        gc_runs(state).await;
    }
}

#[instrument(skip(state))]
async fn gc_heartbeats(state: &AppState) {
    let repo = PlatformSettingsRepo::new(state.db());
    let retention_hours = match repo.heartbeat_retention_hours().await {
        Ok(h) => h,
        Err(e) => {
            warn!(error = %e, "data_retention: failed to read heartbeat_retention_hours");
            return;
        }
    };

    if retention_hours == 0 {
        return;
    }

    let cutoff = Utc::now() - Duration::hours(retention_hours);
    let heartbeat_repo = AgentHeartbeatRepo::new(state.db());
    match heartbeat_repo.delete_older_than(cutoff).await {
        Ok(0) => {}
        Ok(n) => info!(deleted = n, "data_retention: purged old agent heartbeats"),
        Err(e) => warn!(error = %e, "data_retention: heartbeat GC failed"),
    }
}

#[instrument(skip(state))]
async fn gc_runs(state: &AppState) {
    let settings_repo = PlatformSettingsRepo::new(state.db());
    let global_days = match settings_repo.run_retention_days().await {
        Ok(d) => d,
        Err(e) => {
            warn!(error = %e, "data_retention: failed to read run_retention_days");
            return;
        }
    };

    let project_repo = ProjectRepo::new(state.db());
    let projects = match project_repo.list_with_retention(global_days).await {
        Ok(p) => p,
        Err(e) => {
            warn!(error = %e, "data_retention: failed to list projects for run GC");
            return;
        }
    };

    if projects.is_empty() {
        return;
    }

    let run_repo = RunRepo::new(state.db());
    for project in &projects {
        let cutoff = Utc::now() - Duration::days(project.effective_retention_days);
        match run_repo
            .delete_old_runs_for_project(project.id, cutoff, RUN_DELETE_BATCH_SIZE)
            .await
        {
            Ok(0) => {}
            Ok(n) => info!(
                project_id = %project.id,
                deleted = n,
                "data_retention: purged old runs"
            ),
            Err(e) => warn!(
                project_id = %project.id,
                error = %e,
                "data_retention: run GC failed"
            ),
        }
    }
}
