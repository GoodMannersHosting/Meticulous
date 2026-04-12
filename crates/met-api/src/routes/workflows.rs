//! Reusable workflow routes.

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use met_core::ids::ProjectId;
use met_store::repos::{CreateWorkflow, ReusableWorkflow, WorkflowRepo, WorkflowVersionListMode};
use serde::{Deserialize, Serialize};
use tracing::instrument;
use utoipa::ToSchema;

use crate::{
    error::{ApiError, ApiResult},
    extractors::{Auth, PaginatedResponse, Pagination},
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/workflows/global", get(list_global_workflows))
        .route(
            "/projects/{project_id}/workflows/available",
            get(list_project_workflows_available),
        )
        .route(
            "/projects/{project_id}/workflows",
            get(list_project_workflows).post(create_project_workflow),
        )
        .route("/workflows/{workflow_id}", get(get_workflow))
        .route("/workflows/{workflow_id}/versions", get(list_versions))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct WorkflowResponse {
    pub id: String,
    pub scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    pub name: String,
    pub version: String,
    pub definition: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub deprecated: bool,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scm_repository: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scm_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scm_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scm_revision: Option<String>,
    pub submission_status: String,
    pub trust_state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub submitted_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reviewed_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reviewed_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<DateTime<Utc>>,
    pub catalog_metadata: serde_json::Value,
    /// When set, pipelines warn before this date and are hard-blocked on/after it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecated_after: Option<DateTime<Utc>>,
    /// Markdown note explaining the reason for the deprecation period.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecation_note: Option<String>,
}

impl From<ReusableWorkflow> for WorkflowResponse {
    fn from(w: ReusableWorkflow) -> Self {
        use met_store::repos::{
            WorkflowSource as S, WorkflowSubmissionStatus as Sub, WorkflowTrustState as T,
        };
        let source = match w.source {
            S::Git => "git",
            S::Api => "api",
            S::ProjectSync => "project_sync",
        };
        let submission_status = match w.submission_status {
            Sub::Pending => "pending",
            Sub::Approved => "approved",
            Sub::Rejected => "rejected",
        };
        let trust_state = match w.trust_state {
            T::Trusted => "trusted",
            T::Untrusted => "untrusted",
        };
        Self {
            id: w.id.to_string(),
            scope: format!("{:?}", w.scope).to_lowercase(),
            project_id: w.project_id.map(|id| id.to_string()),
            name: w.name,
            version: w.version,
            definition: w.definition,
            description: w.description,
            deprecated: w.deprecated,
            tags: w.tags,
            created_at: w.created_at,
            updated_at: w.updated_at,
            source: source.to_string(),
            scm_repository: w.scm_repository,
            scm_ref: w.scm_ref,
            scm_path: w.scm_path,
            scm_revision: w.scm_revision,
            submission_status: submission_status.to_string(),
            trust_state: trust_state.to_string(),
            submitted_by: w.submitted_by.map(|u| u.to_string()),
            reviewed_by: w.reviewed_by.map(|u| u.to_string()),
            reviewed_at: w.reviewed_at,
            deleted_at: w.deleted_at,
            catalog_metadata: w.catalog_metadata,
            deprecated_after: w.deprecated_after,
            deprecation_note: w.deprecation_note,
        }
    }
}

/// Workflows visible when authoring pipelines in a project: org **global** (execution-gated) plus **project**-scoped rows.
#[derive(Debug, Serialize, ToSchema)]
pub struct ProjectWorkflowsAvailableResponse {
    pub global_workflows: Vec<WorkflowResponse>,
    pub project_workflows: Vec<WorkflowResponse>,
}

#[utoipa::path(
    get,
    path = "/api/v1/projects/{project_id}/workflows/available",
    params(("project_id" = String, Path, description = "Project ID")),
    responses(
        (status = 200, description = "Global + project reusable workflows", body = ProjectWorkflowsAvailableResponse),
        (status = 403, description = "Forbidden"),
    ),
    tag = "workflows",
)]
#[instrument(skip(state))]
async fn list_project_workflows_available(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(project_id): Path<ProjectId>,
) -> ApiResult<Json<ProjectWorkflowsAvailableResponse>> {
    if !user.can_access_project(project_id) {
        return Err(ApiError::forbidden("no access to this project"));
    }

    const LIMIT: i64 = 500;
    let repo = WorkflowRepo::new(state.db());
    let global_workflows = repo
        .list_global(user.org_id, LIMIT, 0)
        .await?
        .into_iter()
        .map(WorkflowResponse::from)
        .collect();
    let project_workflows = repo
        .list_project(project_id, LIMIT, 0)
        .await?
        .into_iter()
        .map(WorkflowResponse::from)
        .collect();

    Ok(Json(ProjectWorkflowsAvailableResponse {
        global_workflows,
        project_workflows,
    }))
}

#[utoipa::path(
    get,
    path = "/api/v1/workflows/global",
    params(
        ("cursor" = Option<String>, Query, description = "Pagination cursor"),
        ("limit" = Option<u32>, Query, description = "Items per page"),
    ),
    responses(
        (status = 200, description = "List of global workflows", body = serde_json::Value),
        (status = 401, description = "Unauthorized"),
    ),
    tag = "workflows",
)]
#[instrument(skip(state))]
async fn list_global_workflows(
    State(state): State<AppState>,
    Auth(user): Auth,
    pagination: Pagination,
) -> ApiResult<Json<PaginatedResponse<WorkflowResponse>>> {
    let repo = WorkflowRepo::new(state.db());
    let workflows = repo
        .list_global(user.org_id, pagination.sql_limit(), 0)
        .await?;

    let response = PaginatedResponse::new(
        workflows.into_iter().map(WorkflowResponse::from).collect(),
        pagination.limit,
        |w| w.id.clone(),
    );

    Ok(Json(response))
}

#[utoipa::path(
    get,
    path = "/api/v1/projects/{project_id}/workflows",
    params(
        ("project_id" = String, Path, description = "Project ID"),
        ("cursor" = Option<String>, Query, description = "Pagination cursor"),
        ("limit" = Option<u32>, Query, description = "Items per page"),
    ),
    responses(
        (status = 200, description = "List of project workflows", body = serde_json::Value),
        (status = 403, description = "Forbidden"),
    ),
    tag = "workflows",
)]
#[instrument(skip(state))]
async fn list_project_workflows(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(project_id): Path<ProjectId>,
    pagination: Pagination,
) -> ApiResult<Json<PaginatedResponse<WorkflowResponse>>> {
    if !user.can_access_project(project_id) {
        return Err(ApiError::forbidden("no access to this project"));
    }

    let repo = WorkflowRepo::new(state.db());
    let workflows = repo
        .list_project(project_id, pagination.sql_limit(), 0)
        .await?;

    let response = PaginatedResponse::new(
        workflows.into_iter().map(WorkflowResponse::from).collect(),
        pagination.limit,
        |w| w.id.clone(),
    );

    Ok(Json(response))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateWorkflowRequest {
    pub name: String,
    pub version: String,
    pub definition: serde_json::Value,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[utoipa::path(
    post,
    path = "/api/v1/projects/{project_id}/workflows",
    params(("project_id" = String, Path, description = "Project ID")),
    request_body = CreateWorkflowRequest,
    responses(
        (status = 200, description = "Workflow created", body = WorkflowResponse),
        (status = 400, description = "Bad request"),
        (status = 403, description = "Forbidden"),
    ),
    tag = "workflows",
)]
#[instrument(skip(state, req))]
async fn create_project_workflow(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(project_id): Path<ProjectId>,
    Json(req): Json<CreateWorkflowRequest>,
) -> ApiResult<Json<WorkflowResponse>> {
    if !user.can_access_project(project_id) {
        return Err(ApiError::forbidden("no access to this project"));
    }

    if req.name.is_empty() {
        return Err(ApiError::bad_request("name is required"));
    }

    if req.version.is_empty() {
        return Err(ApiError::bad_request("version is required"));
    }

    let repo = WorkflowRepo::new(state.db());
    let workflow = repo
        .create_project(
            user.org_id,
            project_id,
            &CreateWorkflow {
                name: req.name.clone(),
                version: req.version,
                definition: req.definition,
                description: req.description,
                tags: req.tags,
            },
        )
        .await?;

    tracing::info!(
        workflow_id = %workflow.id,
        name = %req.name,
        "workflow created"
    );

    Ok(Json(WorkflowResponse::from(workflow)))
}

#[utoipa::path(
    get,
    path = "/api/v1/workflows/{workflow_id}",
    params(("workflow_id" = String, Path, description = "Workflow ID")),
    responses(
        (status = 200, description = "Workflow details", body = WorkflowResponse),
        (status = 404, description = "Workflow not found"),
    ),
    tag = "workflows",
)]
#[instrument(skip(state))]
async fn get_workflow(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(workflow_id): Path<String>,
) -> ApiResult<Json<WorkflowResponse>> {
    let uuid: uuid::Uuid = workflow_id
        .parse()
        .map_err(|_| ApiError::bad_request("invalid workflow ID"))?;

    let row = sqlx::query_as::<_, ReusableWorkflow>(
        r#"
        SELECT id, org_id, project_id, scope, name, version, definition, description, deprecated, tags, created_at, updated_at,
               source, scm_repository, scm_ref, scm_path, scm_revision, submission_status, trust_state,
               submitted_by, reviewed_by, reviewed_at, deleted_at, catalog_metadata,
               deprecated_after, deprecation_note
        FROM reusable_workflows
        WHERE id = $1 AND org_id = $2
        "#,
    )
    .bind(uuid)
    .bind(user.org_id.as_uuid())
    .fetch_optional(state.db())
    .await
    .map_err(met_store::StoreError::from)?
    .ok_or_else(|| ApiError::not_found("workflow not found"))?;

    Ok(Json(WorkflowResponse::from(row)))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct WorkflowVersionsResponse {
    pub workflow_name: String,
    pub versions: Vec<String>,
}

#[utoipa::path(
    get,
    path = "/api/v1/workflows/{workflow_id}/versions",
    params(("workflow_id" = String, Path, description = "Workflow ID")),
    responses(
        (status = 200, description = "Workflow versions", body = WorkflowVersionsResponse),
        (status = 404, description = "Workflow not found"),
    ),
    tag = "workflows",
)]
#[instrument(skip(state))]
async fn list_versions(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(workflow_id): Path<String>,
) -> ApiResult<Json<WorkflowVersionsResponse>> {
    let uuid: uuid::Uuid = workflow_id
        .parse()
        .map_err(|_| ApiError::bad_request("invalid workflow ID"))?;

    let row = sqlx::query_as::<_, ReusableWorkflow>(
        r#"
        SELECT id, org_id, project_id, scope, name, version, definition, description, deprecated, tags, created_at, updated_at,
               source, scm_repository, scm_ref, scm_path, scm_revision, submission_status, trust_state,
               submitted_by, reviewed_by, reviewed_at, deleted_at, catalog_metadata,
               deprecated_after, deprecation_note
        FROM reusable_workflows
        WHERE id = $1 AND org_id = $2
        "#,
    )
    .bind(uuid)
    .bind(user.org_id.as_uuid())
    .fetch_optional(state.db())
    .await
    .map_err(met_store::StoreError::from)?
    .ok_or_else(|| ApiError::not_found("workflow not found"))?;

    let repo = WorkflowRepo::new(state.db());
    let project_id = row.project_id.map(ProjectId::from_uuid);
    let versions = repo
        .list_versions(
            user.org_id,
            project_id,
            row.scope,
            &row.name,
            WorkflowVersionListMode::Catalog,
        )
        .await?;

    Ok(Json(WorkflowVersionsResponse {
        workflow_name: row.name,
        versions,
    }))
}
