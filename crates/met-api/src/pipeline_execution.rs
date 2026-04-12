//! Resolve [`PipelineIR`](met_parser::ir::PipelineIR) and start the pipeline engine for an existing `runs` row.
//! Manual trigger and run retry both use this so a retried run is actually scheduled.

use std::collections::HashMap;
use std::sync::Arc;

use indexmap::IndexMap;
use uuid::Uuid;
use met_core::ids::{OrganizationId, PipelineId, ProjectId, RunId, TriggerId};
use met_core::models::{Pipeline, Run};
use met_parser::ir::PipelineIR;
use met_parser::{DatabaseWorkflowProvider, PipelineParser};
use sqlx::PgPool;

use crate::error::{ApiError, ApiResult, STORED_SECRETS_UNAVAILABLE};
use crate::github_scm;
use crate::state::AppState;
use crate::workflow_diagnostics;

/// Merge variables with precedence: project base, then project env, then pipeline base, then pipeline env
/// (for a run targeting environment `run_environment_id`; omit env-only rows when it is `None`).
async fn load_platform_variables_merged(
    pool: &PgPool,
    org_id: OrganizationId,
    project_id: ProjectId,
    pipeline_id: PipelineId,
    run_environment_id: Option<Uuid>,
) -> Result<IndexMap<String, String>, sqlx::Error> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        r#"
        SELECT name, value FROM variables
        WHERE org_id = $1 AND project_id = $2
          AND (pipeline_id IS NULL OR pipeline_id = $3)
          AND (
            environment_id IS NULL
            OR ($4::uuid IS NOT NULL AND environment_id = $4)
          )
        ORDER BY
          CASE WHEN pipeline_id IS NULL THEN 0 ELSE 1 END,
          CASE WHEN environment_id IS NULL THEN 0 ELSE 1 END,
          name ASC
        "#,
    )
    .bind(org_id.as_uuid())
    .bind(project_id.as_uuid())
    .bind(pipeline_id.as_uuid())
    .bind(run_environment_id)
    .fetch_all(pool)
    .await?;

    let mut map = IndexMap::new();
    for (name, value) in rows {
        map.insert(name, value);
    }
    Ok(map)
}

/// Precedence: DB (project/pipeline × environment, narrowest wins) < YAML `variables:` < one-off trigger payload.
pub(crate) fn merge_runtime_variables_into_ir(
    pipeline_ir: &mut PipelineIR,
    platform: IndexMap<String, String>,
    trigger: Option<HashMap<String, String>>,
) {
    let from_yaml = std::mem::take(&mut pipeline_ir.variables);
    let mut merged = platform;
    for (k, v) in from_yaml {
        merged.insert(k, v);
    }
    if let Some(tv) = trigger {
        for (k, v) in tv {
            merged.insert(k, v);
        }
    }
    pipeline_ir.variables = merged;
}

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
    trigger_variables: Option<HashMap<String, String>>,
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

    let run_environment_id: Option<Uuid> = sqlx::query_scalar(
        r#"SELECT environment_id FROM runs WHERE id = $1"#,
    )
    .bind(run_id.as_uuid())
    .fetch_one(state.db())
    .await
    .map_err(|e| ApiError::internal(format!("load run environment: {e}")))?;

    let platform_vars = load_platform_variables_merged(
        state.db(),
        org_id,
        project_id,
        pipeline_id,
        run_environment_id,
    )
    .await
    .map_err(|e| ApiError::internal(format!("load variables: {e}")))?;
    merge_runtime_variables_into_ir(&mut pipeline_ir, platform_vars, trigger_variables);

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

/// Workflow diagnostics, persist a run, and enqueue engine work — shared by manual trigger and webhooks.
pub async fn dispatch_pipeline_run(
    state: &AppState,
    pipeline: &Pipeline,
    org_id: OrganizationId,
    commit_sha: Option<&str>,
    branch: Option<&str>,
    trigger_id: Option<TriggerId>,
    run_triggered_by: &str,
    engine_triggered_by: &'static str,
    trigger_variables: Option<HashMap<String, String>>,
    webhook_remote_addr: Option<String>,
) -> ApiResult<Run> {
    use met_store::repos::{OrganizationRepo, RunRepo};

    let org = OrganizationRepo::new(state.db()).get(org_id).await?;
    let yaml = workflow_diagnostics::load_pipeline_yaml_string_for_diagnostics(
        state, pipeline, org_id, commit_sha, branch,
    )
    .await?;
    let wf_diag = workflow_diagnostics::collect_workflow_diagnostics(
        state.db(),
        org_id,
        pipeline.project_id,
        org.allow_untrusted_workflows,
        &yaml,
    )
    .await?;
    if workflow_diagnostics::diagnostics_has_blocking(&wf_diag) {
        return Err(ApiError::bad_request(format!(
            "workflow catalog policy: {}",
            workflow_diagnostics::diagnostics_trigger_message(&wf_diag)
        )));
    }

    let pipeline_ir =
        load_pipeline_ir_for_execution(state, pipeline, org_id, commit_sha, branch).await?;

    let run_repo = RunRepo::new(state.db());
    let run = run_repo
        .create_full(
            pipeline.id,
            org_id,
            trigger_id,
            run_triggered_by,
            None,
            commit_sha,
            branch,
            None,
            None,
            webhook_remote_addr.as_deref(),
        )
        .await?;
    let run_id = run.id;

    start_engine_for_existing_run_from_state(
        state,
        org_id,
        run_id,
        pipeline_ir,
        pipeline.id,
        pipeline.project_id,
        engine_triggered_by,
        trigger_variables,
    )
    .await?;

    Ok(run)
}
