//! Resolve [`PipelineIR`](met_parser::ir::PipelineIR) and start the pipeline engine for an existing `runs` row.
//! Manual trigger and run retry both use this so a retried run is actually scheduled.

use std::sync::Arc;

use met_core::ids::{OrganizationId, PipelineId, ProjectId, RunId};
use met_core::models::Pipeline;
use met_parser::ir::PipelineIR;
use met_parser::{DatabaseWorkflowProvider, PipelineParser};

use crate::error::{ApiError, ApiResult, STORED_SECRETS_UNAVAILABLE};
use crate::github_scm;
use crate::state::AppState;

/// Build pipeline IR the same way as POST `/pipelines/{id}/trigger`.
pub async fn load_pipeline_ir_for_execution(
    state: &AppState,
    pipeline: &Pipeline,
    org_id: OrganizationId,
    commit_sha: Option<&str>,
    branch: Option<&str>,
) -> ApiResult<PipelineIR> {
    let effective_ref = commit_sha
        .or(branch)
        .or(pipeline.scm_ref.as_deref())
        .unwrap_or("main");

    let pipeline_ir = if pipeline.scm_provider.as_deref() == Some("github") {
        let Some(crypto) = state.stored_secret_crypto.as_ref() else {
            return Err(ApiError::unavailable(STORED_SECRETS_UNAVAILABLE));
        };
        let repository = pipeline
            .scm_repository
            .as_deref()
            .ok_or_else(|| ApiError::bad_request("Git-backed pipeline missing scm_repository"))?;
        let credentials_path =
            pipeline
                .scm_credentials_secret_path
                .as_deref()
                .ok_or_else(|| {
                    ApiError::bad_request("Git-backed pipeline missing scm_credentials_secret_path")
                })?;
        let scm_path = pipeline
            .scm_path
            .as_deref()
            .ok_or_else(|| ApiError::bad_request("Git-backed pipeline missing scm_path"))?;

        let (ir, _, _) = github_scm::parse_pipeline_from_github_checkout(
            state.db(),
            crypto.as_ref(),
            org_id,
            pipeline.project_id,
            repository,
            effective_ref,
            scm_path,
            credentials_path,
        )
        .await?;
        ir
    } else {
        let yaml = serde_yaml::to_string(&pipeline.definition).map_err(|e| {
            ApiError::bad_request(format!(
                "pipeline definition is not representable as YAML: {e}"
            ))
        })?;

        let wf_provider = DatabaseWorkflowProvider::new(state.db().clone(), org_id.as_uuid());
        let mut parser = PipelineParser::new(&wf_provider);
        parser.parse(&yaml).await.map_err(|diags| {
            ApiError::bad_request(format!(
                "invalid pipeline definition: {}",
                diags
                    .iter()
                    .map(|d| d.message.clone())
                    .collect::<Vec<_>>()
                    .join("; ")
            ))
        })?
    };

    Ok(pipeline_ir)
}

/// Spawn [`Engine::execute_with_existing_run`](Engine::execute_with_existing_run) after acquiring capacity.
pub async fn start_engine_for_existing_run_from_state(
    state: &AppState,
    org_id: OrganizationId,
    run_id: RunId,
    mut pipeline_ir: PipelineIR,
    pipeline_id: PipelineId,
    project_id: ProjectId,
    triggered_by: &'static str,
) -> ApiResult<()> {
    let Some(engine) = state.engine.as_ref().map(Arc::clone) else {
        let detail = state
            .engine_init_error
            .as_deref()
            .unwrap_or("NATS or JetStream initialization failed (see API logs)");
        return Err(ApiError::unavailable(format!(
            "pipeline engine is not available: {detail}"
        )));
    };

    pipeline_ir.id = pipeline_id;
    pipeline_ir.project_id = Some(project_id);

    let permit = state
        .engine_run_semaphore
        .clone()
        .acquire_owned()
        .await
        .map_err(|_| ApiError::unavailable("engine shutdown"))?;

    tokio::spawn(async move {
        let _run_capacity = permit;
        let res = engine
            .execute_with_existing_run(run_id, org_id, pipeline_ir, triggered_by)
            .await;
        if let Err(e) = res {
            tracing::error!(%run_id, error = %e, "pipeline engine run failed");
        }
    });

    Ok(())
}
