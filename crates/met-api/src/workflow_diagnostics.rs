//! Resolve reusable workflow references in a pipeline definition and report catalog/trust status.

use met_core::ids::{OrganizationId, ProjectId};
use met_core::models::Pipeline;
use met_parser::ir::WorkflowScope as IrWorkflowScope;
use met_parser::schema::{RawPipeline, RawWorkflowDef};
use met_parser::semver::{parse_version_constraint, resolve_version};
use met_parser::workflow::parse_workflow_ref;
use met_store::repos::{
    WorkflowRepo, WorkflowScope as DbWorkflowScope, WorkflowVersionListMode,
};
use sqlx::PgPool;

use crate::error::{ApiError, ApiResult, STORED_SECRETS_UNAVAILABLE};
use crate::state::AppState;

/// One workflow reference from the pipeline and how it resolves against the org catalog / DB.
#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]
pub struct WorkflowDiagnosticItem {
    /// Invocation id from the pipeline YAML (`workflows[].id`).
    pub invocation_id: String,
    /// Original reference e.g. `global/build`.
    pub reference: String,
    pub scope: String,
    pub name: String,
    pub version_requested: String,
    pub version_resolved: Option<String>,
    /// `ok` | `missing` | `version_not_found` | `pending_approval` | `rejected` | `deleted` |
    /// `untrusted_blocked` | `project_not_found`
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// When true, pipeline execution should be blocked until resolved.
    pub blocking: bool,
    /// Declared workflow output names from the resolved catalog/project definition (`outputs:` + step `outputs:`), when parseable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub declared_outputs: Option<Vec<String>>,
}

/// Collect declared output names for `${{ workflows.<id>.outputs.<name> }}` validation hints.
pub fn declared_output_names_from_definition(def: &serde_json::Value) -> Option<Vec<String>> {
    let wf: RawWorkflowDef = serde_json::from_value(def.clone()).ok()?;
    let mut names: Vec<String> = wf.outputs.keys().cloned().collect();
    for job in &wf.jobs {
        for step in &job.steps {
            names.extend(step.outputs.keys().cloned());
        }
    }
    names.sort();
    names.dedup();
    if names.is_empty() {
        None
    } else {
        Some(names)
    }
}

fn resolve_version_string(versions: &[String], requested: &str) -> Result<String, ()> {
    if requested == "latest" {
        return versions.first().cloned().ok_or(());
    }
    if let Ok(c) = parse_version_constraint(requested) {
        return resolve_version(&c, versions).map(|s| s.to_string()).ok_or(());
    }
    if versions.iter().any(|v| v == requested) {
        return Ok(requested.to_string());
    }
    Err(())
}

async fn diagnose_global(
    repo: &WorkflowRepo<'_>,
    org_id: OrganizationId,
    name: &str,
    version_requested: &str,
    allow_untrusted: bool,
) -> WorkflowDiagnosticItem {
    let catalog_versions = match repo
        .list_versions(
            org_id,
            None,
            DbWorkflowScope::Global,
            name,
            WorkflowVersionListMode::Catalog,
        )
        .await
    {
        Ok(v) => v,
        Err(e) => {
            return WorkflowDiagnosticItem {
                invocation_id: String::new(),
                reference: String::new(),
                scope: "global".to_string(),
                name: name.to_string(),
                version_requested: version_requested.to_string(),
                version_resolved: None,
                status: "missing".to_string(),
                detail: Some(format!("list versions: {e}")),
                blocking: true,
                declared_outputs: None,
            };
        }
    };

    let resolved = match resolve_version_string(&catalog_versions, version_requested) {
        Ok(v) => v,
        Err(()) => {
            return WorkflowDiagnosticItem {
                invocation_id: String::new(),
                reference: String::new(),
                scope: "global".to_string(),
                name: name.to_string(),
                version_requested: version_requested.to_string(),
                version_resolved: None,
                status: "version_not_found".to_string(),
                detail: Some(format!(
                    "no matching version (have: {})",
                    catalog_versions.join(", ")
                )),
                blocking: true,
                declared_outputs: None,
            };
        }
    };

    let row = match repo
        .get_global_row_any_status(org_id, name, &resolved)
        .await
    {
        Ok(r) => r,
        Err(_) => {
            if repo
                .get_global_deleted_row(org_id, name, &resolved)
                .await
                .ok()
                .flatten()
                .is_some()
            {
                return WorkflowDiagnosticItem {
                    invocation_id: String::new(),
                    reference: String::new(),
                    scope: "global".to_string(),
                    name: name.to_string(),
                    version_requested: version_requested.to_string(),
                    version_resolved: Some(resolved),
                    status: "deleted".to_string(),
                    detail: Some("this catalog version was removed".to_string()),
                    blocking: true,
                    declared_outputs: None,
                };
            }
            return WorkflowDiagnosticItem {
                invocation_id: String::new(),
                reference: String::new(),
                scope: "global".to_string(),
                name: name.to_string(),
                version_requested: version_requested.to_string(),
                version_resolved: Some(resolved),
                status: "missing".to_string(),
                detail: None,
                blocking: true,
                declared_outputs: None,
            };
        }
    };

    use met_store::repos::{WorkflowSubmissionStatus, WorkflowTrustState};

    let mut blocking = false;
    let mut status = "ok";
    let mut detail: Option<String> = None;

    match row.submission_status {
        WorkflowSubmissionStatus::Pending => {
            status = "pending_approval";
            blocking = true;
            detail = Some("catalog workflow is awaiting admin approval".to_string());
        }
        WorkflowSubmissionStatus::Rejected => {
            status = "rejected";
            blocking = true;
            detail = Some("catalog workflow was rejected".to_string());
        }
        WorkflowSubmissionStatus::Approved => {}
    }

    if status == "ok"
        && row.trust_state == WorkflowTrustState::Untrusted
        && !allow_untrusted
    {
        status = "untrusted_blocked";
        blocking = true;
        detail = Some(
            "untrusted catalog workflow; enable org setting allow_untrusted_workflows or mark trusted"
                .to_string(),
        );
    }

    let declared_outputs = declared_output_names_from_definition(&row.definition);

    WorkflowDiagnosticItem {
        invocation_id: String::new(),
        reference: String::new(),
        scope: "global".to_string(),
        name: name.to_string(),
        version_requested: version_requested.to_string(),
        version_resolved: Some(resolved),
        status: status.to_string(),
        detail,
        blocking,
        declared_outputs,
    }
}

async fn diagnose_project(
    repo: &WorkflowRepo<'_>,
    org_id: OrganizationId,
    project_id: ProjectId,
    name: &str,
    version_requested: &str,
) -> WorkflowDiagnosticItem {
    let versions = match repo
        .list_versions(
            org_id,
            Some(project_id),
            DbWorkflowScope::Project,
            name,
            WorkflowVersionListMode::Execution,
        )
        .await
    {
        Ok(v) => v,
        Err(_) => Vec::new(),
    };

    let resolved = match resolve_version_string(&versions, version_requested) {
        Ok(v) => v,
        Err(()) => {
            return WorkflowDiagnosticItem {
                invocation_id: String::new(),
                reference: String::new(),
                scope: "project".to_string(),
                name: name.to_string(),
                version_requested: version_requested.to_string(),
                version_resolved: None,
                status: "project_not_found".to_string(),
                detail: Some(
                    "no matching project-scoped workflow version in the database (sync from Git if this pipeline uses `.stable/workflows`)".to_string(),
                ),
                blocking: true,
                declared_outputs: None,
            };
        }
    };

    match repo
        .get(
            org_id,
            Some(project_id),
            DbWorkflowScope::Project,
            name,
            &resolved,
        )
        .await
    {
        Ok(row) => WorkflowDiagnosticItem {
            invocation_id: String::new(),
            reference: String::new(),
            scope: "project".to_string(),
            name: name.to_string(),
            version_requested: version_requested.to_string(),
            version_resolved: Some(resolved),
            status: "ok".to_string(),
            detail: None,
            blocking: false,
            declared_outputs: declared_output_names_from_definition(&row.definition),
        },
        Err(_) => WorkflowDiagnosticItem {
            invocation_id: String::new(),
            reference: String::new(),
            scope: "project".to_string(),
            name: name.to_string(),
            version_requested: version_requested.to_string(),
            version_resolved: Some(resolved),
            status: "project_not_found".to_string(),
            detail: None,
            blocking: true,
            declared_outputs: None,
        },
    }
}

/// Inspect `workflows:` references in pipeline YAML (no full IR / workflow fetch).
pub async fn collect_workflow_diagnostics(
    pool: &PgPool,
    org_id: OrganizationId,
    project_id: ProjectId,
    allow_untrusted: bool,
    yaml: &str,
) -> ApiResult<Vec<WorkflowDiagnosticItem>> {
    let raw: RawPipeline = serde_yaml::from_str(yaml).map_err(|e| {
        ApiError::bad_request(format!("pipeline YAML: {e}"))
    })?;

    let repo = WorkflowRepo::new(pool);
    let mut out = Vec::new();

    for inv in raw.workflows {
        let (scope, name, version) =
            parse_workflow_ref(&inv.workflow, inv.version.as_deref())
                .map_err(|e| ApiError::bad_request(format!("workflow {}: {}", inv.id, e.message)))?;

        let mut item = match scope {
            IrWorkflowScope::Global => {
                diagnose_global(&repo, org_id, &name, &version, allow_untrusted).await
            }
            IrWorkflowScope::Project => {
                diagnose_project(&repo, org_id, project_id, &name, &version).await
            }
        };
        item.invocation_id = inv.id.clone();
        item.reference = inv.workflow.clone();
        out.push(item);
    }

    Ok(out)
}

/// `true` if any item is blocking.
pub fn diagnostics_has_blocking(items: &[WorkflowDiagnosticItem]) -> bool {
    items.iter().any(|i| i.blocking)
}

/// Human-readable summary for 400 responses.
pub fn diagnostics_trigger_message(items: &[WorkflowDiagnosticItem]) -> String {
    items
        .iter()
        .filter(|i| i.blocking)
        .map(|i| {
            format!(
                "{} ({}): {}{}",
                i.invocation_id,
                i.reference,
                i.status,
                i.detail
                    .as_ref()
                    .map(|d| format!(" — {d}"))
                    .unwrap_or_default()
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// Resolve the pipeline definition as YAML for workflow reference scanning.
pub async fn load_pipeline_yaml_string_for_diagnostics(
    state: &AppState,
    pipeline: &Pipeline,
    org_id: OrganizationId,
    commit_sha: Option<&str>,
    branch: Option<&str>,
) -> ApiResult<String> {
    let effective_ref = commit_sha
        .or(branch)
        .or(pipeline.scm_ref.as_deref())
        .unwrap_or("main");
    if pipeline.scm_provider.as_deref() == Some("github") {
        let Some(crypto) = state.stored_secret_crypto.as_ref() else {
            return Err(ApiError::unavailable(STORED_SECRETS_UNAVAILABLE));
        };
        let repository = pipeline
            .scm_repository
            .as_deref()
            .ok_or_else(|| ApiError::bad_request("Git-backed pipeline missing scm_repository"))?;
        let credentials_path = pipeline
            .scm_credentials_secret_path
            .as_deref()
            .ok_or_else(|| {
                ApiError::bad_request("Git-backed pipeline missing scm_credentials_secret_path")
            })?;
        let scm_path = pipeline
            .scm_path
            .as_deref()
            .ok_or_else(|| ApiError::bad_request("Git-backed pipeline missing scm_path"))?;

        crate::github_scm::fetch_pipeline_yaml_from_github_checkout(
            state.db(),
            crypto.as_ref(),
            org_id,
            pipeline.project_id,
            repository,
            effective_ref,
            scm_path,
            credentials_path,
        )
        .await
    } else {
        serde_yaml::to_string(&pipeline.definition).map_err(|e| {
            ApiError::bad_request(format!(
                "pipeline definition is not representable as YAML: {e}"
            ))
        })
    }
}
