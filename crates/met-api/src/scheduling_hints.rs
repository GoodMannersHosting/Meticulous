//! Best-effort scheduling explanations for job runs (pending / queued), derived from
//! the resolved pipeline IR and current job run statuses.

use async_trait::async_trait;
use met_core::{
    ids::{JobId, OrganizationId, ProjectId},
    models::{JobRun, JobStatus},
};
use met_parser::{
    PipelineParser, WorkflowFetchError, WorkflowProvider,
    ir::{JobIR, PipelineIR, PoolSelector, TagValue, WorkflowScope},
    schema::RawWorkflowDef,
    semver::{parse_version_constraint, resolve_version},
};
use met_store::{
    PgPool,
    repos::{WorkflowRepo, WorkflowScope as DbWorkflowScope, WorkflowVersionListMode},
};
use tracing::debug;

/// Database-backed workflow resolution for both global and project workflows.
struct StoreWorkflowProvider {
    pool: PgPool,
    org_id: OrganizationId,
    project_id: ProjectId,
}

impl StoreWorkflowProvider {
    fn new(pool: PgPool, org_id: OrganizationId, project_id: ProjectId) -> Self {
        Self {
            pool,
            org_id,
            project_id,
        }
    }

    fn map_scope(scope: WorkflowScope) -> DbWorkflowScope {
        match scope {
            WorkflowScope::Global => DbWorkflowScope::Global,
            WorkflowScope::Project => DbWorkflowScope::Project,
        }
    }

    fn project_for_scope(&self, scope: WorkflowScope) -> Option<ProjectId> {
        match scope {
            WorkflowScope::Global => None,
            WorkflowScope::Project => Some(self.project_id),
        }
    }
}

#[async_trait]
impl WorkflowProvider for StoreWorkflowProvider {
    async fn fetch(
        &self,
        scope: WorkflowScope,
        name: &str,
        version: &str,
    ) -> Result<RawWorkflowDef, WorkflowFetchError> {
        let repo = WorkflowRepo::new(&self.pool);
        let db_scope = Self::map_scope(scope);
        let project_ref = self.project_for_scope(scope);

        let resolved_version = if version == "latest" {
            repo.get_latest(self.org_id, project_ref, db_scope, name)
                .await
                .map_err(|e| WorkflowFetchError::Network(e.to_string()))?
                .version
        } else if let Ok(constraint) = parse_version_constraint(version) {
            let versions = repo
                .list_versions(
                    self.org_id,
                    project_ref,
                    db_scope,
                    name,
                    WorkflowVersionListMode::Execution,
                )
                .await
                .map_err(|e| WorkflowFetchError::Network(e.to_string()))?;
            resolve_version(&constraint, &versions).ok_or_else(|| {
                WorkflowFetchError::VersionNotFound {
                    scope: match scope {
                        WorkflowScope::Global => "global".to_string(),
                        WorkflowScope::Project => "project".to_string(),
                    },
                    name: name.to_string(),
                    version: version.to_string(),
                }
            })?
        } else {
            version.to_string()
        };

        let wf = repo
            .get(self.org_id, project_ref, db_scope, name, &resolved_version)
            .await
            .map_err(|_| WorkflowFetchError::NotFound {
                scope: match scope {
                    WorkflowScope::Global => "global".to_string(),
                    WorkflowScope::Project => "project".to_string(),
                },
                name: name.to_string(),
            })?;

        let mut workflow: RawWorkflowDef = serde_json::from_value(wf.definition)
            .map_err(|e| WorkflowFetchError::Parse(e.to_string()))?;
        workflow.version = Some(resolved_version);
        if workflow.description.is_none() {
            workflow.description = wf.description;
        }
        Ok(workflow)
    }

    async fn list_versions(
        &self,
        scope: WorkflowScope,
        name: &str,
    ) -> Result<Vec<String>, WorkflowFetchError> {
        let repo = WorkflowRepo::new(&self.pool);
        let db_scope = Self::map_scope(scope);
        let project_ref = self.project_for_scope(scope);
        repo.list_versions(
            self.org_id,
            project_ref,
            db_scope,
            name,
            WorkflowVersionListMode::Execution,
        )
            .await
            .map_err(|e| WorkflowFetchError::Network(e.to_string()))
    }
}

/// Parse pipeline definition into IR using global + project workflows in the database.
///
/// Returns `None` if YAML conversion or parsing fails (e.g. missing workflow, Git-only composite).
pub async fn try_parse_pipeline_ir(
    pool: &PgPool,
    org_id: OrganizationId,
    project_id: ProjectId,
    definition: &serde_json::Value,
) -> Option<PipelineIR> {
    let yaml = serde_yaml::to_string(definition).ok()?;
    let provider = StoreWorkflowProvider::new(pool.clone(), org_id, project_id);
    let mut parser = PipelineParser::new(&provider);
    match parser.parse(&yaml).await {
        Ok(ir) => Some(ir),
        Err(diags) => {
            debug!(
                "scheduling_hints: pipeline IR parse failed: {}",
                diags
                    .iter()
                    .map(|d| d.message.as_str())
                    .collect::<Vec<_>>()
                    .join("; ")
            );
            None
        }
    }
}

fn job_display_name<'a>(ir: &'a PipelineIR, id: JobId) -> &'a str {
    ir.jobs
        .iter()
        .find(|j| j.id == id)
        .map(|j| j.name.as_str())
        .unwrap_or("dependency")
}

fn format_pool_hint(pool: &PoolSelector) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(p) = &pool.pool_name {
        if p != "_default" && !p.is_empty() {
            parts.push(format!("pool '{p}'"));
        }
    }
    for (k, v) in &pool.required_tags {
        let s = match v {
            TagValue::Bool(b) => format!("{k}={b}"),
            TagValue::String(s) => format!("{k}={s}"),
            TagValue::Present => k.clone(),
        };
        parts.push(s);
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(", "))
    }
}

/// Whether a dependency has finished (success, failure, or skip) per engine semantics.
fn dep_is_done(status: JobStatus) -> bool {
    matches!(
        status,
        JobStatus::Succeeded
            | JobStatus::Failed
            | JobStatus::Cancelled
            | JobStatus::TimedOut
            | JobStatus::Skipped
    )
}

fn find_job_ir<'a>(ir: &'a PipelineIR, jr: &JobRun) -> Option<&'a JobIR> {
    ir.jobs.iter().find(|j| j.id == jr.job_id)
}

/// Produce a short human-readable explanation for stalled-looking job states.
pub fn scheduling_hint(ir: &PipelineIR, job_runs: &[JobRun], jr: &JobRun) -> Option<String> {
    if jr.status == JobStatus::Queued {
        return Some(
            "Dispatched to an agent queue; waiting for the agent to start this job.".to_string(),
        );
    }
    if jr.status != JobStatus::Pending {
        return None;
    }

    let job_ir = find_job_ir(ir, jr)?;

    let id_to_status: std::collections::HashMap<JobId, JobStatus> =
        job_runs.iter().map(|j| (j.job_id, j.status)).collect();

    let mut blockers: Vec<String> = Vec::new();
    for dep_id in &job_ir.depends_on {
        let st = id_to_status.get(dep_id).copied();
        let Some(st) = st else {
            continue;
        };
        if dep_is_done(st) {
            continue;
        }
        let label = job_display_name(ir, *dep_id);
        let st_label = format!("{:?}", st).to_lowercase();
        blockers.push(format!("{label} ({st_label})"));
    }

    if !blockers.is_empty() {
        return Some(format!(
            "Waiting for upstream jobs to finish: {}.",
            blockers.join(", ")
        ));
    }

    let pool = format_pool_hint(&job_ir.pool_selector);
    let tail = if let Some(p) = pool {
        format!(
            "Waiting for the scheduler to assign an available agent (requires {p}). Other jobs may be using concurrency slots."
        )
    } else {
        "Waiting for the scheduler — no upstream blockers, but an agent must be available (and under the concurrency limit) before this job starts.".to_string()
    };

    Some(tail)
}
