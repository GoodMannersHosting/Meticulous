//! ADR-014 explicit `workspace:` validation and producer job resolution.

use std::collections::{HashMap, HashSet};

use met_core::ids::JobId;

use crate::error::{ErrorCode, ParseDiagnostics, ParseError};
use crate::ir::{JobIR, PipelineIR};

/// Resolve `restore_from_job_id` for each job with `workspace_transfer`, and validate edges.
pub fn resolve_and_validate_workspace_transfers(ir: &mut PipelineIR, diagnostics: &mut ParseDiagnostics) {
    let invocation_ids: HashSet<String> = ir
        .jobs
        .iter()
        .filter_map(|j| j.workflow_invocation_id.clone())
        .collect();

    let mut terminal_by_invocation: HashMap<String, JobId> = HashMap::new();
    for inv in &invocation_ids {
        if let Some(t) = terminal_job_for_invocation(ir, inv) {
            terminal_by_invocation.insert(inv.clone(), t);
        }
    }

    for job in &mut ir.jobs {
        let Some(wt) = job.workspace_transfer.as_mut() else {
            continue;
        };
        let Some(ref inv_raw) = wt.restore_from_invocation_id else {
            continue;
        };
        let inv = inv_raw.trim();
        if inv.is_empty() {
            wt.restore_from_invocation_id = None;
            continue;
        }
        let inv_owned = inv.to_string();
        if !invocation_ids.contains(inv) {
            diagnostics.push(ParseError::new(
                ErrorCode::E5006,
                format!(
                    "workspace.from: unknown workflow invocation id '{inv}' (job '{}')",
                    job.name
                ),
            ));
            continue;
        }

        let Some(producer) = terminal_by_invocation.get(&inv_owned).copied() else {
            diagnostics.push(ParseError::new(
                ErrorCode::E5006,
                format!(
                    "workspace.from: could not resolve producer job for invocation '{inv}' (job '{}'); invocation must have a unique terminal job",
                    job.name
                ),
            ));
            continue;
        };

        if !job.depends_on.contains(&producer) {
            diagnostics.push(
                ParseError::new(
                    ErrorCode::E5006,
                    format!(
                        "workspace.from: job '{}' must list the producer invocation's terminal job in depends-on (missing dependency on snapshot source)",
                        job.name
                    ),
                )
                .with_hint(format!(
                    "Add the workflow that ends with the '{inv}' snapshot to depends-on (or depend on that workflow's terminal job)."
                )),
            );
        }

        wt.restore_from_invocation_id = Some(inv_owned);
        wt.restore_from_job_id = Some(producer);
    }
}

/// Jobs in `invocation_id` form an induced subgraph; return the unique "sink" job id, or None if ambiguous.
fn terminal_job_for_invocation(ir: &PipelineIR, invocation_id: &str) -> Option<JobId> {
    let group: Vec<&JobIR> = ir
        .jobs
        .iter()
        .filter(|j| j.workflow_invocation_id.as_deref() == Some(invocation_id))
        .collect();
    if group.is_empty() {
        return None;
    }
    if group.len() == 1 {
        return Some(group[0].id);
    }
    let mut sinks = Vec::new();
    for j in &group {
        let mut depended_on_by_other = false;
        for k in &group {
            if k.id != j.id && k.depends_on.contains(&j.id) {
                depended_on_by_other = true;
                break;
            }
        }
        if !depended_on_by_other {
            sinks.push(j.id);
        }
    }
    if sinks.len() == 1 {
        return Some(sinks[0]);
    }
    None
}
