//! Passive workspace snapshots for `share_workspace` affinity groups (ADR-014 extension).

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use async_trait::async_trait;
use met_core::ids::{JobId, OrganizationId};
use met_parser::{JobIR, PipelineIR};

use crate::error::Result;

/// Presign workspace snapshot uploads/downloads against object storage.
#[async_trait]
pub trait WorkspaceSnapshotPresigner: Send + Sync {
    async fn presign_put(
        &self,
        org_id: OrganizationId,
        object_key: &str,
        expires_in: Duration,
    ) -> Result<String>;

    async fn presign_get(
        &self,
        org_id: OrganizationId,
        object_key: &str,
        expires_in: Duration,
    ) -> Result<String>;
}

/// Registered snapshot after a producer job completes successfully.
#[derive(Debug, Clone)]
pub struct WorkspaceSnapshotRecord {
    pub object_key: String,
    pub sha256: String,
    pub size_bytes: i64,
    pub producer_job_run_id: met_core::ids::JobRunId,
    pub workflow_invocation_id: String,
    pub generation: i32,
}

/// Configuration for passive workspace snapshots.
#[derive(Debug, Clone)]
pub struct WorkspaceSnapshotConfig {
    pub enabled: bool,
    pub presign_put_ttl: Duration,
    pub presign_get_ttl: Duration,
    pub max_archive_bytes: i64,
    /// Operator hint for S3 lifecycle on `workspace-snapshots/` (hours). Enforced in object store, not the engine.
    pub object_ttl_hours: u32,
}

impl Default for WorkspaceSnapshotConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            presign_put_ttl: Duration::from_secs(3600),
            presign_get_ttl: Duration::from_secs(3600),
            max_archive_bytes: 10_i64 * 1024 * 1024 * 1024,
            object_ttl_hours: 24,
        }
    }
}

/// Returns whether `to` is reachable from `from` following dependency edges (successor depends on predecessor).
fn dependency_reachable(pipeline: &PipelineIR, from: JobId, to: JobId) -> bool {
    let mut stack = vec![from];
    let mut seen = HashSet::new();
    while let Some(n) = stack.pop() {
        if n == to {
            return true;
        }
        if !seen.insert(n) {
            continue;
        }
        for j in &pipeline.jobs {
            if j.depends_on.contains(&n) {
                stack.push(j.id);
            }
        }
    }
    false
}

/// Maximal in-group predecessor for passive snapshot restore (deepest dependency in the same affinity group).
#[must_use]
pub fn workspace_snapshot_predecessor(pipeline: &PipelineIR, job: &JobIR) -> Option<JobId> {
    if !job.share_workspace {
        return None;
    }
    let group = job.affinity_group.as_deref()?;
    let jobs_by_id: HashMap<JobId, &JobIR> = pipeline.jobs.iter().map(|j| (j.id, j)).collect();

    let candidates: Vec<JobId> = job
        .depends_on
        .iter()
        .copied()
        .filter(|d| {
            jobs_by_id
                .get(d)
                .is_some_and(|j| j.share_workspace && j.affinity_group.as_deref() == Some(group))
        })
        .collect();

    if candidates.is_empty() {
        return None;
    }

    // Pick `c` such that every other candidate `o` can reach `c` via dependency edges (o runs before c).
    for &c in &candidates {
        let mut ok = true;
        for &o in &candidates {
            if o == c {
                continue;
            }
            if !dependency_reachable(pipeline, o, c) {
                ok = false;
                break;
            }
        }
        if ok {
            return Some(c);
        }
    }

    None
}

fn object_base_prefix(
    org_id: OrganizationId,
    project_id: Option<met_core::ids::ProjectId>,
) -> String {
    let org = org_id.as_uuid().to_string();
    match project_id {
        Some(p) => format!("orgs/{org}/projects/{}", p.as_uuid()),
        None => format!("orgs/{org}"),
    }
}

/// Build the object key for a producer job's snapshot (matches `ObjectKeyBuilder::workspace_snapshot_job_run`).
#[must_use]
pub fn snapshot_object_key_for_job_run(
    org_id: OrganizationId,
    project_id: Option<met_core::ids::ProjectId>,
    run_id: met_core::ids::RunId,
    producer_job_run_id: met_core::ids::JobRunId,
) -> String {
    let base = object_base_prefix(org_id, project_id);
    format!(
        "{base}/workspace-snapshots/{}/{}.tar.zst",
        run_id, producer_job_run_id
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use met_core::ids::{JobId, PipelineId};
    use met_parser::ir::{JobIR, PipelineIR, PoolSelector};
    use std::time::Duration;

    fn jid(s: &str) -> JobId {
        JobId::from_uuid(uuid::Uuid::parse_str(s).unwrap())
    }

    fn minimal_job(
        id: JobId,
        name: &str,
        depends_on: Vec<JobId>,
        share: bool,
        group: Option<&str>,
    ) -> JobIR {
        JobIR {
            id,
            name: name.to_string(),
            depends_on,
            pool_selector: PoolSelector::default(),
            steps: vec![],
            services: vec![],
            timeout: Duration::from_secs(60),
            retry_policy: None,
            cache_config: None,
            condition: None,
            source_workflow: None,
            env: Default::default(),
            affinity_group: group.map(String::from),
            share_workspace: share,
            workflow_invocation_id: None,
            workflow_invocation_name: None,
        }
    }

    #[test]
    fn predecessor_picks_deepest_in_group_dependency() {
        let a = jid("00000000-0000-0000-0000-0000000000a1");
        let b = jid("00000000-0000-0000-0000-0000000000a2");
        let c = jid("00000000-0000-0000-0000-0000000000a3");
        let ir = PipelineIR {
            id: PipelineId::new(),
            name: "p".to_string(),
            source_file: None,
            project_id: None,
            triggers: vec![],
            variables: Default::default(),
            secret_refs: Default::default(),
            jobs: vec![
                minimal_job(a, "checkout", vec![], true, Some("ci")),
                minimal_job(b, "lint", vec![a], true, Some("ci")),
                minimal_job(c, "test", vec![a, b], true, Some("ci")),
            ],
            default_pool_selector: None,
            expose_workflow_secret_outputs: false,
        };
        let job_c = ir.jobs.iter().find(|j| j.id == c).unwrap();
        assert_eq!(workspace_snapshot_predecessor(&ir, job_c), Some(b));
    }
}
