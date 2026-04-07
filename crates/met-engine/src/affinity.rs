//! Same-agent affinity: shared workspace directory name and dispatch flags.

use met_core::ids::RunId;
use met_parser::{JobIR, PipelineIR};
use sha2::{Digest, Sha256};

use crate::state::RunState;

/// Stable subdirectory name under the agent workspace root for a shared affinity workspace.
#[must_use]
pub fn workspace_root_dir_name(run_id: RunId, affinity_group: &str) -> String {
    let mut h = Sha256::new();
    h.update(run_id.to_string().as_bytes());
    h.update(b":");
    h.update(affinity_group.as_bytes());
    let out = h.finalize();
    format!("w{}", hex::encode(&out[..16]))
}

/// When true, a successful completion must not count toward `MET_AGENT_EXIT_AFTER_JOBS`.
pub async fn suppress_exit_after_jobs_increment(
    pipeline: &PipelineIR,
    job: &JobIR,
    run_state: &RunState,
) -> bool {
    let Some(ref g) = job.affinity_group else {
        return false;
    };
    for other in &pipeline.jobs {
        if other.id == job.id {
            continue;
        }
        if other.affinity_group.as_ref() == Some(g) && !run_state.is_job_complete(&other.id).await {
            return true;
        }
    }
    false
}

/// When false, the agent retains the workspace directory after the job (shared root until last job).
pub async fn workspace_delete_after_job(
    pipeline: &PipelineIR,
    job: &JobIR,
    run_state: &RunState,
) -> bool {
    if !job.share_workspace {
        return true;
    }
    let Some(ref g) = job.affinity_group else {
        return true;
    };
    for other in &pipeline.jobs {
        if other.id == job.id {
            continue;
        }
        if other.share_workspace && other.affinity_group.as_ref() == Some(g) {
            if !run_state.is_job_complete(&other.id).await {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::JobState;
    use met_core::ids::{JobId, JobRunId, PipelineId};
    use std::time::Duration;

    fn job(
        id: JobId,
        name: &str,
        depends_on: Vec<JobId>,
        group: Option<&str>,
        share: bool,
    ) -> JobIR {
        JobIR {
            id,
            name: name.to_string(),
            depends_on,
            pool_selector: Default::default(),
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
        }
    }

    fn pipeline(jobs: Vec<JobIR>) -> PipelineIR {
        PipelineIR {
            id: PipelineId::new(),
            name: "t".into(),
            source_file: None,
            project_id: None,
            triggers: vec![],
            variables: Default::default(),
            secret_refs: Default::default(),
            jobs,
            default_pool_selector: None,
            expose_workflow_secret_outputs: false,
        }
    }

    #[tokio::test]
    async fn suppress_until_last_in_group() {
        let run = RunState::new(RunId::new());
        let a = JobId::new();
        let b = JobId::new();
        run.register_job(JobState::new(a, JobRunId::new(), "ja"))
            .await;
        run.register_job(JobState::new(b, JobRunId::new(), "jb"))
            .await;

        let p = pipeline(vec![
            job(a, "ja", vec![], Some("g"), false),
            job(b, "jb", vec![a], Some("g"), false),
        ]);

        assert!(
            suppress_exit_after_jobs_increment(&p, &p.jobs[0], &run).await,
            "other job in group not complete"
        );
        run.mark_job_completed(&a, true, None, None).await;
        assert!(
            !suppress_exit_after_jobs_increment(&p, &p.jobs[1], &run).await,
            "last job should count toward exit limit"
        );
    }

    #[tokio::test]
    async fn delete_shared_workspace_only_after_last() {
        let run = RunState::new(RunId::new());
        let a = JobId::new();
        let b = JobId::new();
        run.register_job(JobState::new(a, JobRunId::new(), "ja"))
            .await;
        run.register_job(JobState::new(b, JobRunId::new(), "jb"))
            .await;

        let p = pipeline(vec![
            job(a, "ja", vec![], Some("g"), true),
            job(b, "jb", vec![a], Some("g"), true),
        ]);

        assert!(!workspace_delete_after_job(&p, &p.jobs[0], &run).await);
        run.mark_job_completed(&a, true, None, None).await;
        assert!(workspace_delete_after_job(&p, &p.jobs[1], &run).await);
    }
}
