//! Pipeline CRUD routes.

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{delete, get, post, put},
};
use met_core::{
    ids::{PipelineId, ProjectId},
    models::{CreatePipeline, Pipeline, UpdatePipeline},
};
use met_store::repos::{OrganizationRepo, PipelineRepo, ProjectRepo};
use serde::{Deserialize, Serialize};
use tracing::instrument;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    error::{ApiError, ApiResult, STORED_SECRETS_UNAVAILABLE},
    extractors::{Auth, PaginatedResponse, Pagination, SessionOrAppAuth},
    github_scm, pipeline_execution,
    project_access::{
        SessionOrApp, effective_project_role_in_user_org,
        effective_project_role_session_or_app_in_user_org, ensure_session_or_app_pipeline_scope,
    },
    state::AppState,
    workflow_diagnostics::{self, WorkflowDiagnosticItem},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/pipelines", get(list_pipelines).post(create_pipeline))
        .route(
            "/pipelines/{id}",
            get(get_pipeline)
                .put(update_pipeline)
                .delete(delete_pipeline),
        )
        .route("/pipelines/{id}/archive", post(archive_pipeline))
        .route("/pipelines/{id}/unarchive", post(unarchive_pipeline))
        .route(
            "/pipelines/by-slug/{project_id}/{slug}",
            get(get_pipeline_by_slug),
        )
        .route(
            "/projects/{project_id}/pipelines/import-git",
            post(import_pipeline_git),
        )
        .route(
            "/pipelines/{id}/sync-from-git",
            post(sync_pipeline_from_git),
        )
        .route("/pipelines/{id}/trigger", post(trigger_pipeline))
        .route("/pipelines/{id}/validate", post(validate_pipeline))
        .route(
            "/pipelines/{id}/workflow-diagnostics",
            get(pipeline_workflow_diagnostics),
        )
}

#[derive(Debug, Deserialize)]
pub struct ListPipelinesQuery {
    project_id: Option<ProjectId>,
}

#[derive(Debug, Deserialize, Default)]
pub struct WorkflowDiagnosticsQuery {
    pub commit_sha: Option<String>,
    pub branch: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PipelineResponse {
    #[serde(flatten)]
    #[schema(value_type = Object)]
    pub pipeline: Pipeline,
}

#[utoipa::path(
    get,
    path = "/api/v1/pipelines",
    params(
        ("project_id" = Option<String>, Query, description = "Filter by project ID"),
        ("cursor" = Option<String>, Query, description = "Pagination cursor"),
        ("limit" = Option<u32>, Query, description = "Items per page"),
    ),
    responses(
        (status = 200, description = "List of pipelines", body = serde_json::Value),
        (status = 401, description = "Unauthorized"),
    ),
    tag = "pipelines",
)]
#[instrument(skip(state))]
async fn list_pipelines(
    State(state): State<AppState>,
    SessionOrAppAuth(caller): SessionOrAppAuth,
    pagination: Pagination,
    axum::extract::Query(query): axum::extract::Query<ListPipelinesQuery>,
) -> ApiResult<Json<PaginatedResponse<PipelineResponse>>> {
    let repo = PipelineRepo::new(state.db());

    let project_id = query
        .project_id
        .ok_or_else(|| ApiError::bad_request("project_id query parameter is required"))?;

    effective_project_role_session_or_app_in_user_org(state.db(), &caller, project_id).await?;

    let mut pipelines = repo
        .list_by_project(project_id, pagination.sql_limit(), 0)
        .await?;

    if let SessionOrApp::User(u) = &caller {
        if u.is_api_token {
            if let Some(ref allow) = u.pipeline_ids {
                let set: std::collections::HashSet<_> = allow.iter().copied().collect();
                pipelines.retain(|p| set.contains(&p.id));
            }
        }
    }

    let response = PaginatedResponse::new(
        pipelines
            .into_iter()
            .map(|p| PipelineResponse { pipeline: p })
            .collect(),
        pagination.limit,
        |p| p.pipeline.id.to_string(),
    );

    Ok(Json(response))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreatePipelineRequest {
    #[schema(value_type = String)]
    pub project_id: ProjectId,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub definition: serde_json::Value,
    pub definition_path: Option<String>,
}

#[utoipa::path(
    post,
    path = "/api/v1/pipelines",
    request_body = CreatePipelineRequest,
    responses(
        (status = 200, description = "Pipeline created", body = PipelineResponse),
        (status = 400, description = "Bad request"),
        (status = 401, description = "Unauthorized"),
    ),
    tag = "pipelines",
)]
#[instrument(skip(state, req))]
async fn create_pipeline(
    State(state): State<AppState>,
    Auth(user): Auth,
    Json(req): Json<CreatePipelineRequest>,
) -> ApiResult<Json<PipelineResponse>> {
    let role = effective_project_role_in_user_org(state.db(), &user, req.project_id).await?;
    if !role.can_manage_pipelines() {
        return Err(ApiError::forbidden(
            "project administrator role is required to create pipelines",
        ));
    }

    let repo = PipelineRepo::new(state.db());

    let create = CreatePipeline {
        name: req.name,
        slug: req.slug,
        description: req.description,
        definition: req.definition,
        definition_path: req.definition_path,
        scm_provider: None,
        scm_repository: None,
        scm_ref: None,
        scm_path: None,
        scm_credentials_secret_path: None,
        scm_revision: None,
        visibility: Default::default(),
    };

    let pipeline = repo.create(req.project_id, &create).await?;

    Ok(Json(PipelineResponse { pipeline }))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ImportPipelineGitRequest {
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    /// GitHub repository as `owner/name` or `https://github.com/owner/repo`.
    pub repository: String,
    /// Branch, tag, or commit SHA.
    pub git_ref: String,
    /// Path to the pipeline YAML in the repo (e.g. `.stable/demo.yaml`).
    pub scm_path: String,
    /// `builtin_secrets.path` for a project-scoped `github_app` secret.
    pub credentials_path: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/projects/{project_id}/pipelines/import-git",
    request_body = ImportPipelineGitRequest,
    params(("project_id" = String, Path, description = "Project ID")),
    responses(
        (status = 200, description = "Pipeline created from Git", body = PipelineResponse),
        (status = 400, description = "Bad request"),
    ),
    tag = "pipelines",
)]
#[instrument(skip(state, req))]
async fn import_pipeline_git(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(project_id): Path<ProjectId>,
    Json(req): Json<ImportPipelineGitRequest>,
) -> ApiResult<Json<PipelineResponse>> {
    let role = effective_project_role_in_user_org(state.db(), &user, project_id).await?;
    if !role.can_manage_pipelines() {
        return Err(ApiError::forbidden(
            "project administrator role is required to import pipelines",
        ));
    }

    let Some(crypto) = state.stored_secret_crypto.as_ref() else {
        return Err(ApiError::unavailable(STORED_SECRETS_UNAVAILABLE));
    };

    let project = ProjectRepo::new(state.db()).get(project_id).await?;
    let org_id = project.org_id;

    let (ir, commit_sha, def) = github_scm::parse_pipeline_from_github_checkout(
        state.db(),
        crypto.as_ref(),
        org_id,
        project_id,
        &req.repository,
        &req.git_ref,
        &req.scm_path,
        &req.credentials_path,
    )
    .await?;

    let nil_pipe = PipelineId::from_uuid(Uuid::nil());
    met_secret_resolve::validate_secret_refs(
        state.db(),
        org_id,
        Some(project_id),
        nil_pipe,
        &ir.secret_refs,
    )
    .await
    .map_err(|e| ApiError::bad_request(e.to_string()))?;

    let repo = PipelineRepo::new(state.db());
    let create = CreatePipeline {
        name: req.name,
        slug: req.slug,
        description: req.description,
        definition: def,
        definition_path: Some(req.scm_path.clone()),
        scm_provider: Some("github".to_string()),
        scm_repository: Some(req.repository.trim().to_string()),
        scm_ref: Some(req.git_ref.clone()),
        scm_path: Some(req.scm_path),
        scm_credentials_secret_path: Some(req.credentials_path),
        scm_revision: Some(commit_sha),
        visibility: Default::default(),
    };

    let pipeline = repo.create(project_id, &create).await?;

    crate::trigger_sync::reconcile_repo_webhook_triggers(state.db(), org_id, pipeline.id, &ir)
        .await?;

    Ok(Json(PipelineResponse { pipeline }))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SyncPipelineGitRequest {
    /// When set, sync from this ref instead of the pipeline's stored `scm_ref`.
    pub git_ref: Option<String>,
}

#[utoipa::path(
    post,
    path = "/api/v1/pipelines/{id}/sync-from-git",
    request_body = SyncPipelineGitRequest,
    params(("id" = String, Path, description = "Pipeline ID")),
    responses(
        (status = 200, description = "Pipeline definition refreshed", body = PipelineResponse),
        (status = 400, description = "Bad request"),
    ),
    tag = "pipelines",
)]
#[instrument(skip(state, req))]
async fn sync_pipeline_from_git(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(id): Path<PipelineId>,
    Json(req): Json<SyncPipelineGitRequest>,
) -> ApiResult<Json<PipelineResponse>> {
    let Some(crypto) = state.stored_secret_crypto.as_ref() else {
        return Err(ApiError::unavailable(STORED_SECRETS_UNAVAILABLE));
    };

    let pipeline_repo = PipelineRepo::new(state.db());
    let pipeline = pipeline_repo.get(id).await?;

    let role =
        effective_project_role_in_user_org(state.db(), &user, pipeline.project_id).await?;
    if !role.can_manage_pipelines() {
        return Err(ApiError::forbidden(
            "project administrator role is required to sync pipelines from Git",
        ));
    }

    if pipeline.scm_provider.as_deref() != Some("github") {
        return Err(ApiError::bad_request(
            "pipeline is not linked to GitHub (scm_provider is not github)",
        ));
    }

    let repository = pipeline
        .scm_repository
        .as_deref()
        .ok_or_else(|| ApiError::bad_request("pipeline is missing scm_repository"))?;
    let credentials_path = pipeline
        .scm_credentials_secret_path
        .as_deref()
        .ok_or_else(|| ApiError::bad_request("pipeline is missing scm_credentials_secret_path"))?;
    let scm_path = pipeline
        .scm_path
        .as_deref()
        .ok_or_else(|| ApiError::bad_request("pipeline is missing scm_path"))?;
    let default_ref = pipeline
        .scm_ref
        .as_deref()
        .ok_or_else(|| ApiError::bad_request("pipeline is missing scm_ref"))?;
    let git_ref = req.git_ref.as_deref().unwrap_or(default_ref);

    let project = ProjectRepo::new(state.db())
        .get(pipeline.project_id)
        .await?;
    let org_id = project.org_id;

    let (ir, commit_sha, def) = github_scm::parse_pipeline_from_github_checkout(
        state.db(),
        crypto.as_ref(),
        org_id,
        pipeline.project_id,
        repository,
        git_ref,
        scm_path,
        credentials_path,
    )
    .await?;

    met_secret_resolve::validate_secret_refs(
        state.db(),
        org_id,
        Some(pipeline.project_id),
        id,
        &ir.secret_refs,
    )
    .await
    .map_err(|e| ApiError::bad_request(e.to_string()))?;

    let update = UpdatePipeline {
        name: None,
        description: None,
        definition: Some(def),
        enabled: None,
        scm_provider: None,
        scm_repository: None,
        scm_ref: Some(git_ref.to_string()),
        scm_path: None,
        scm_credentials_secret_path: None,
        scm_revision: Some(commit_sha),
        visibility: None,
    };

    let pipeline = pipeline_repo.update(id, &update).await?;
    crate::trigger_sync::reconcile_repo_webhook_triggers(state.db(), org_id, id, &ir).await?;
    Ok(Json(PipelineResponse { pipeline }))
}

#[utoipa::path(
    get,
    path = "/api/v1/pipelines/{id}",
    params(("id" = String, Path, description = "Pipeline ID")),
    responses(
        (status = 200, description = "Pipeline details", body = PipelineResponse),
        (status = 404, description = "Pipeline not found"),
    ),
    tag = "pipelines",
)]
#[instrument(skip(state))]
async fn get_pipeline(
    State(state): State<AppState>,
    SessionOrAppAuth(caller): SessionOrAppAuth,
    Path(id): Path<PipelineId>,
) -> ApiResult<Json<PipelineResponse>> {
    let repo = PipelineRepo::new(state.db());
    let pipeline = repo.get(id).await?;
    effective_project_role_session_or_app_in_user_org(state.db(), &caller, pipeline.project_id)
        .await?;
    ensure_session_or_app_pipeline_scope(&caller, pipeline.id, pipeline.project_id)?;
    Ok(Json(PipelineResponse { pipeline }))
}

#[instrument(skip(state))]
async fn get_pipeline_by_slug(
    State(state): State<AppState>,
    SessionOrAppAuth(caller): SessionOrAppAuth,
    Path((project_id, slug)): Path<(ProjectId, String)>,
) -> ApiResult<Json<PipelineResponse>> {
    let repo = PipelineRepo::new(state.db());
    effective_project_role_session_or_app_in_user_org(state.db(), &caller, project_id).await?;
    let pipeline = repo.get_by_slug(project_id, &slug).await?;
    ensure_session_or_app_pipeline_scope(&caller, pipeline.id, pipeline.project_id)?;
    Ok(Json(PipelineResponse { pipeline }))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdatePipelineRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub definition: Option<serde_json::Value>,
    pub enabled: Option<bool>,
    pub scm_provider: Option<String>,
    pub scm_repository: Option<String>,
    pub scm_ref: Option<String>,
    pub scm_path: Option<String>,
    pub scm_credentials_secret_path: Option<String>,
    pub scm_revision: Option<String>,
}

#[utoipa::path(
    put,
    path = "/api/v1/pipelines/{id}",
    params(("id" = String, Path, description = "Pipeline ID")),
    request_body = UpdatePipelineRequest,
    responses(
        (status = 200, description = "Pipeline updated", body = PipelineResponse),
        (status = 400, description = "Bad request"),
        (status = 404, description = "Pipeline not found"),
    ),
    tag = "pipelines",
)]
#[instrument(skip(state, req))]
async fn update_pipeline(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(id): Path<PipelineId>,
    Json(req): Json<UpdatePipelineRequest>,
) -> ApiResult<Json<PipelineResponse>> {
    let repo = PipelineRepo::new(state.db());
    let existing = repo.get(id).await?;
    let role =
        effective_project_role_in_user_org(state.db(), &user, existing.project_id).await?;
    if !role.can_manage_pipelines() {
        return Err(ApiError::forbidden(
            "project administrator role is required to update pipelines",
        ));
    }

    let update = UpdatePipeline {
        name: req.name,
        description: req.description,
        definition: req.definition,
        enabled: req.enabled,
        scm_provider: req.scm_provider,
        scm_repository: req.scm_repository,
        scm_ref: req.scm_ref,
        scm_path: req.scm_path,
        scm_credentials_secret_path: req.scm_credentials_secret_path,
        scm_revision: req.scm_revision,
        visibility: None,
    };

    let pipeline = repo.update(id, &update).await?;

    Ok(Json(PipelineResponse { pipeline }))
}

#[utoipa::path(
    delete,
    path = "/api/v1/pipelines/{id}",
    params(("id" = String, Path, description = "Pipeline ID")),
    responses(
        (status = 200, description = "Pipeline deleted"),
        (status = 404, description = "Pipeline not found"),
    ),
    tag = "pipelines",
)]
#[instrument(skip(state))]
async fn delete_pipeline(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(id): Path<PipelineId>,
) -> ApiResult<()> {
    let repo = PipelineRepo::new(state.db());
    let pipeline = repo.get(id).await?;
    let role =
        effective_project_role_in_user_org(state.db(), &user, pipeline.project_id).await?;
    if !role.can_manage_pipelines() {
        return Err(ApiError::forbidden(
            "project administrator role is required to delete pipelines",
        ));
    }
    if pipeline.archived_at.is_none() {
        return Err(ApiError::bad_request(
            "only archived pipelines can be permanently deleted; archive first or purge from Admin → Archive",
        ));
    }
    repo.delete(id).await?;
    Ok(())
}

#[instrument(skip(state))]
async fn archive_pipeline(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(id): Path<PipelineId>,
) -> ApiResult<Json<PipelineResponse>> {
    let repo = PipelineRepo::new(state.db());
    let p = repo.get(id).await?;
    let role = effective_project_role_in_user_org(state.db(), &user, p.project_id).await?;
    if !role.can_manage_pipelines() {
        return Err(ApiError::forbidden(
            "project administrator role is required to archive pipelines",
        ));
    }
    let p = repo.archive(id).await?;
    Ok(Json(PipelineResponse { pipeline: p }))
}

#[instrument(skip(state))]
async fn unarchive_pipeline(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(id): Path<PipelineId>,
) -> ApiResult<Json<PipelineResponse>> {
    if !user.has_permission("*") {
        return Err(ApiError::forbidden(
            "only organization administrators may unarchive pipelines (Admin → Archive)",
        ));
    }
    let repo = PipelineRepo::new(state.db());
    let p = repo.get(id).await?;
    let proj = ProjectRepo::new(state.db()).get(p.project_id).await?;
    if proj.org_id != user.org_id {
        return Err(ApiError::not_found("pipeline not found"));
    }
    let p = repo.unarchive(id).await?;
    Ok(Json(PipelineResponse { pipeline: p }))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct TriggerPipelineRequest {
    pub branch: Option<String>,
    pub commit_sha: Option<String>,
    pub variables: Option<std::collections::HashMap<String, String>>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TriggerPipelineResponse {
    #[schema(value_type = String)]
    pub run_id: met_core::ids::RunId,
    pub run_number: i64,
    pub status: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/pipelines/{id}/trigger",
    params(("id" = String, Path, description = "Pipeline ID")),
    request_body = TriggerPipelineRequest,
    responses(
        (status = 200, description = "Pipeline triggered", body = TriggerPipelineResponse),
        (status = 404, description = "Pipeline not found"),
    ),
    tag = "pipelines",
)]
#[instrument(skip(state, req))]
async fn trigger_pipeline(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(id): Path<PipelineId>,
    Json(req): Json<TriggerPipelineRequest>,
) -> ApiResult<Json<TriggerPipelineResponse>> {
    let pipeline_repo = PipelineRepo::new(state.db());
    let pipeline = pipeline_repo.get(id).await?;

    let role =
        effective_project_role_in_user_org(state.db(), &user, pipeline.project_id).await?;
    if !role.can_trigger_pipelines() {
        return Err(ApiError::forbidden(
            "developer or administrator project role is required to trigger pipelines",
        ));
    }

    let project = ProjectRepo::new(state.db())
        .get(pipeline.project_id)
        .await?;
    let org_id = project.org_id;

    let run = pipeline_execution::dispatch_pipeline_run(
        &state,
        &pipeline,
        org_id,
        req.commit_sha.as_deref(),
        req.branch.as_deref(),
        None,
        &user.email,
        "api",
        req.variables,
        None,
    )
    .await?;

    Ok(Json(TriggerPipelineResponse {
        run_id: run.id,
        run_number: run.run_number,
        status: format!("{:?}", run.status).to_lowercase(),
    }))
}

#[utoipa::path(
    get,
    path = "/api/v1/pipelines/{id}/workflow-diagnostics",
    params(
        ("id" = String, Path, description = "Pipeline ID"),
        ("commit_sha" = Option<String>, Query, description = "Commit SHA override"),
        ("branch" = Option<String>, Query, description = "Branch override"),
    ),
    responses(
        (status = 200, description = "Per-invocation workflow resolution", body = Vec<WorkflowDiagnosticItem>),
        (status = 404, description = "Pipeline not found"),
    ),
    tag = "pipelines",
)]
#[instrument(skip(state))]
async fn pipeline_workflow_diagnostics(
    State(state): State<AppState>,
    SessionOrAppAuth(caller): SessionOrAppAuth,
    Path(id): Path<PipelineId>,
    axum::extract::Query(q): axum::extract::Query<WorkflowDiagnosticsQuery>,
) -> ApiResult<Json<Vec<WorkflowDiagnosticItem>>> {
    let pipeline_repo = PipelineRepo::new(state.db());
    let pipeline = pipeline_repo.get(id).await?;

    effective_project_role_session_or_app_in_user_org(state.db(), &caller, pipeline.project_id)
        .await?;

    let project = ProjectRepo::new(state.db())
        .get(pipeline.project_id)
        .await?;
    let org_id = project.org_id;
    let org = OrganizationRepo::new(state.db()).get(org_id).await?;

    let yaml = workflow_diagnostics::load_pipeline_yaml_string_for_diagnostics(
        &state,
        &pipeline,
        org_id,
        q.commit_sha.as_deref(),
        q.branch.as_deref(),
    )
    .await?;

    let items = workflow_diagnostics::collect_workflow_diagnostics(
        state.db(),
        org_id,
        pipeline.project_id,
        org.allow_untrusted_workflows,
        &yaml,
    )
    .await?;

    Ok(Json(items))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ValidatePipelineRequest {
    pub definition: serde_json::Value,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ValidatePipelineResponse {
    pub valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

#[utoipa::path(
    post,
    path = "/api/v1/pipelines/{id}/validate",
    params(("id" = String, Path, description = "Pipeline ID")),
    request_body = ValidatePipelineRequest,
    responses(
        (status = 200, description = "Validation result", body = ValidatePipelineResponse),
        (status = 404, description = "Pipeline not found"),
    ),
    tag = "pipelines",
)]
#[instrument(skip(state, req))]
async fn validate_pipeline(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(id): Path<PipelineId>,
    Json(req): Json<ValidatePipelineRequest>,
) -> ApiResult<Json<ValidatePipelineResponse>> {
    let pipeline = PipelineRepo::new(state.db()).get(id).await?;
    effective_project_role_in_user_org(state.db(), &user, pipeline.project_id).await?;

    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    if req.definition.is_null() {
        errors.push("definition cannot be null".to_string());
    }

    if let Some(obj) = req.definition.as_object() {
        if !obj.contains_key("jobs") && !obj.contains_key("steps") {
            warnings.push("definition should contain 'jobs' or 'steps'".to_string());
        }
    } else {
        errors.push("definition must be a JSON object".to_string());
    }

    Ok(Json(ValidatePipelineResponse {
        valid: errors.is_empty(),
        errors,
        warnings,
    }))
}
