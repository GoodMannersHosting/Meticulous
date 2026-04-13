//! Validation for pipeline-level agent affinity and shared workspace.

use crate::error::{ErrorCode, ParseDiagnostics, ParseError};
use crate::ir::PipelineIR;
use met_core::ids::JobId;
use std::collections::{HashMap, HashSet};

/// Reject pipelines where `share-workspace` is true but two jobs in the same affinity group can run concurrently.
pub fn validate_share_workspace_affinity(ir: &PipelineIR, diagnostics: &mut ParseDiagnostics) {
    if ir.allow_parallel_shared_workspace_jobs {
        return;
    }
    if !ir
        .jobs
        .iter()
        .any(|j| j.share_workspace && j.affinity_group.is_some())
    {
        return;
    }

    let mut adj: HashMap<JobId, Vec<JobId>> = HashMap::new();
    for j in &ir.jobs {
        for dep in &j.depends_on {
            adj.entry(*dep).or_default().push(j.id);
        }
    }

    fn reachable(adj: &HashMap<JobId, Vec<JobId>>, from: JobId, to: JobId) -> bool {
        let mut stack = vec![from];
        let mut seen = HashSet::new();
        while let Some(n) = stack.pop() {
            if n == to {
                return true;
            }
            if !seen.insert(n) {
                continue;
            }
            if let Some(nexts) = adj.get(&n) {
                for &x in nexts {
                    stack.push(x);
                }
            }
        }
        false
    }

    let mut groups: HashMap<String, Vec<JobId>> = HashMap::new();
    for j in &ir.jobs {
        if j.share_workspace
            && let Some(ref g) = j.affinity_group
        {
            groups.entry(g.clone()).or_default().push(j.id);
        }
    }

    let id_to_name: HashMap<JobId, &str> =
        ir.jobs.iter().map(|j| (j.id, j.name.as_str())).collect();

    for (g, job_ids) in groups {
        if job_ids.len() < 2 {
            continue;
        }
        for i in 0..job_ids.len() {
            for j in (i + 1)..job_ids.len() {
                let a = job_ids[i];
                let b = job_ids[j];
                if !reachable(&adj, a, b) && !reachable(&adj, b, a) {
                    let na = id_to_name.get(&a).unwrap_or(&"?");
                    let nb = id_to_name.get(&b).unwrap_or(&"?");
                    diagnostics.push(
                        ParseError::new(
                            ErrorCode::E5005,
                            format!(
                                "agent affinity group '{g}': jobs '{na}' and '{nb}' can run concurrently; share-workspace requires a total order via depends-on"
                            ),
                        )
                        .with_hint(
                            "Chain dependencies so only one job in the group runs at a time, or set agent-affinity.share-workspace: false",
                        ),
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{JobIR, PipelineIR};
    use met_core::ids::{JobId, PipelineId};
    use std::time::Duration;

    fn jid(s: &str) -> JobId {
        JobId::from_uuid(uuid::Uuid::parse_str(s).unwrap())
    }

    fn minimal_job(
        id: JobId,
        name: &str,
        depends_on: Vec<JobId>,
        group: Option<&str>,
        share_ws: bool,
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
            share_workspace: share_ws,
            workflow_invocation_id: None,
            workflow_invocation_name: None,
            workspace_transfer: None,
        }
    }

    #[test]
    fn share_workspace_serial_ok() {
        let a = jid("00000000-0000-0000-0000-000000000001");
        let b = jid("00000000-0000-0000-0000-000000000002");
        let ir = PipelineIR {
            id: PipelineId::new(),
            name: "p".to_string(),
            source_file: None,
            project_id: None,
            triggers: vec![],
            variables: Default::default(),
            secret_refs: Default::default(),
            jobs: vec![
                minimal_job(a, "j1", vec![], Some("g"), true),
                minimal_job(b, "j2", vec![a], Some("g"), true),
            ],
            default_pool_selector: None,
            expose_workflow_secret_outputs: false,
            allow_parallel_shared_workspace_jobs: false,
        };
        let mut diag = ParseDiagnostics::new();
        validate_share_workspace_affinity(&ir, &mut diag);
        assert!(!diag.has_errors());
    }

    #[test]
    fn share_workspace_parallel_rejected() {
        let a = jid("00000000-0000-0000-0000-000000000001");
        let b = jid("00000000-0000-0000-0000-000000000002");
        let ir = PipelineIR {
            id: PipelineId::new(),
            name: "p".to_string(),
            source_file: None,
            project_id: None,
            triggers: vec![],
            variables: Default::default(),
            secret_refs: Default::default(),
            jobs: vec![
                minimal_job(a, "j1", vec![], Some("g"), true),
                minimal_job(b, "j2", vec![], Some("g"), true),
            ],
            default_pool_selector: None,
            expose_workflow_secret_outputs: false,
            allow_parallel_shared_workspace_jobs: false,
        };
        let mut diag = ParseDiagnostics::new();
        validate_share_workspace_affinity(&ir, &mut diag);
        assert!(diag.has_errors());
    }

    #[test]
    fn share_workspace_parallel_allowed_when_opt_in() {
        let a = jid("00000000-0000-0000-0000-000000000001");
        let b = jid("00000000-0000-0000-0000-000000000002");
        let ir = PipelineIR {
            id: PipelineId::new(),
            name: "p".to_string(),
            source_file: None,
            project_id: None,
            triggers: vec![],
            variables: Default::default(),
            secret_refs: Default::default(),
            jobs: vec![
                minimal_job(a, "j1", vec![], Some("g"), true),
                minimal_job(b, "j2", vec![], Some("g"), true),
            ],
            default_pool_selector: None,
            expose_workflow_secret_outputs: false,
            allow_parallel_shared_workspace_jobs: true,
        };
        let mut diag = ParseDiagnostics::new();
        validate_share_workspace_affinity(&ir, &mut diag);
        assert!(!diag.has_errors());
    }
}
