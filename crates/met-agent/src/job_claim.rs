//! Local idempotency markers for successfully finished jobs (survives workspace cleanup).
//!
//! When JetStream redelivers a dispatch after the agent already completed the job successfully,
//! we skip re-execution and ACK. Failed jobs do not write a marker so NAK/redelivery can retry.

use std::path::PathBuf;

use crate::config::AgentConfig;
use crate::error::{AgentError, Result};

fn claims_dir(cfg: &AgentConfig) -> PathBuf {
    cfg.workspace_dir
        .parent()
        .map(|p| p.join("claims"))
        .unwrap_or_else(|| cfg.workspace_dir.join("claims"))
}

fn completed_marker(cfg: &AgentConfig, job_run_id: &str) -> PathBuf {
    claims_dir(cfg).join(format!("{job_run_id}.completed"))
}

/// Returns true when this job run was already finished successfully on this agent.
#[must_use]
pub async fn job_successfully_completed(cfg: &AgentConfig, job_run_id: &str) -> bool {
    let path = completed_marker(cfg, job_run_id);
    tokio::fs::try_exists(path).await.unwrap_or(false)
}

/// Record that a job completed successfully (best-effort).
pub async fn record_job_successful_completion(cfg: &AgentConfig, job_run_id: &str) -> Result<()> {
    let path = completed_marker(cfg, job_run_id);
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(|e| {
            AgentError::Workspace(format!("create claims dir {}: {e}", parent.display()))
        })?;
    }
    tokio::fs::write(&path, b"ok\n")
        .await
        .map_err(|e| AgentError::Workspace(format!("write completion marker: {e}")))?;
    Ok(())
}
