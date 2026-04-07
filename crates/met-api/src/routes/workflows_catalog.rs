//! Org catalog workflows: Git import, list, version search.

use axum::extract::Query;
use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use met_core::ids::ProjectId;
use met_store::repos::{
    CreateGlobalCatalogGit, WorkflowRepo, WorkflowSubmissionStatus,
};
use serde::{Deserialize, Serialize};
use tracing::instrument;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    error::{ApiError, ApiResult, STORED_SECRETS_UNAVAILABLE},
    extractors::{Auth, PaginatedResponse, Pagination, PaginationMeta},
    github_scm,
    state::AppState,
};

use crate::routes::workflows::WorkflowResponse;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/workflows/catalog", get(list_catalog_workflows))
        .route(
            "/projects/{project_id}/workflows/catalog/import-git",
            post(import_catalog_workflow_git),
        )
        .route(
            "/workflows/{workflow_id}/catalog-versions",
            get(list_catalog_versions),
        )
}

#[derive(Debug, Deserialize)]
pub struct CatalogListQuery {
    #[serde(default)]
    pub status: Option<String>,
}

/// `cursor` for catalog lists is a non-negative SQL `OFFSET` as a decimal string.
fn parse_catalog_list_offset(cursor: Option<&str>) -> i64 {
    let Some(raw) = cursor else {
        return 0;
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return 0;
    }
    trimmed
        .parse::<i64>()
        .ok()
        .filter(|&o| o >= 0)
        .unwrap_or(0)
}

#[utoipa::path(
    get,
    path = "/api/v1/workflows/catalog",
    tag = "workflows",
)]
#[instrument(skip(state))]
async fn list_catalog_workflows(
    State(state): State<AppState>,
    Auth(user): Auth,
    pagination: Pagination,
    Query(q): Query<CatalogListQuery>,
) -> ApiResult<Json<PaginatedResponse<WorkflowResponse>>> {
    let repo = WorkflowRepo::new(state.db());
    let st = match q.status.as_deref() {
        Some("pending") => Some(WorkflowSubmissionStatus::Pending),
        Some("approved") => Some(WorkflowSubmissionStatus::Approved),
        Some("rejected") => Some(WorkflowSubmissionStatus::Rejected),
        Some(s) if !s.is_empty() => {
            return Err(ApiError::bad_request(format!(
                "unknown status filter: {s} (use pending|approved|rejected)"
            )));
        }
        _ => None,
    };

    let offset = parse_catalog_list_offset(pagination.cursor.as_deref());
    let workflows = repo
        .list_global_catalog(user.org_id, pagination.sql_limit(), offset, st)
        .await?;

    let limit_usize = pagination.limit as usize;
    let mut items: Vec<WorkflowResponse> =
        workflows.into_iter().map(WorkflowResponse::from).collect();
    let fetched = items.len();
    let has_more = fetched > limit_usize;
    if has_more {
        items.pop();
    }
    let count = items.len();
    let next_cursor = if has_more {
        Some((offset as usize + count).to_string())
    } else {
        None
    };

    Ok(Json(PaginatedResponse {
        data: items,
        pagination: PaginationMeta {
            next_cursor,
            has_more,
            count,
        },
    }))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ImportCatalogWorkflowGitRequest {
    /// GitHub repository as `owner/name` or URL.
    pub repository: String,
    pub git_ref: String,
    /// Path to workflow YAML in the repo.
    pub workflow_path: String,
    pub credentials_path: String,
}

fn catalog_metadata_json(
    def: &serde_json::Value,
    upstream_url: impl Into<String>,
) -> serde_json::Value {
    let summary = def
        .get("description")
        .and_then(|v| v.as_str())
        .or_else(|| def.get("name").and_then(|v| v.as_str()))
        .unwrap_or("workflow");

    let mut tools = Vec::new();
    if let Some(jobs) = def.get("jobs").and_then(|j| j.as_array()) {
        for job in jobs {
            if let Some(steps) = job.get("steps").and_then(|s| s.as_array()) {
                for step in steps {
                    if let Some(u) = step.get("uses").and_then(|u| u.as_str()) {
                        tools.push(u.to_string());
                    }
                }
            }
        }
    }

    serde_json::json!({
        "summary": summary,
        "tools": tools,
        "target_arch": serde_json::Value::Null,
        "target_os": serde_json::Value::Null,
        "upstream_url": upstream_url.into(),
    })
}

#[utoipa::path(
    post,
    path = "/api/v1/projects/{project_id}/workflows/catalog/import-git",
    request_body = ImportCatalogWorkflowGitRequest,
    responses(
        (status = 200, description = "Catalog workflow version created", body = WorkflowResponse),
        (status = 400, description = "Bad request"),
    ),
    tag = "workflows",
)]
#[instrument(skip(state, req))]
async fn import_catalog_workflow_git(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(project_id): Path<ProjectId>,
    Json(req): Json<ImportCatalogWorkflowGitRequest>,
) -> ApiResult<Json<WorkflowResponse>> {
    if !user.can_access_project(project_id) {
        return Err(ApiError::forbidden("no access to this project"));
    }

    let Some(crypto) = state.stored_secret_crypto.as_ref() else {
        return Err(ApiError::unavailable(STORED_SECRETS_UNAVAILABLE));
    };

    let project = met_store::repos::ProjectRepo::new(state.db())
        .get(project_id)
        .await?;
    let org_id = project.org_id;

    let slug = github_scm::parse_github_repository(&req.repository)?;
    let api_base = github_scm::github_api_base_for_credentials_path(
        state.db(),
        crypto.as_ref(),
        org_id,
        project_id,
        &req.credentials_path,
    )
    .await?;
    let token = github_scm::github_app_installation_token_for_project_secret(
        state.db(),
        crypto.as_ref(),
        org_id,
        project_id,
        &req.credentials_path,
    )
    .await?;
    let commit_sha = github_scm::resolve_github_commit_sha(
        &token,
        &slug.owner,
        &slug.name,
        &req.git_ref,
        &api_base,
    )
    .await?;

    let (yaml_text, _) = github_scm::fetch_github_text_file(
        &token,
        &slug.owner,
        &slug.name,
        req.workflow_path.trim_start_matches('/'),
        &commit_sha,
        &api_base,
    )
    .await?;

    let def = github_scm::yaml_file_to_json_value(&yaml_text)?;

    let name = def
        .get("name")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            ApiError::bad_request("workflow YAML must include a non-empty `name` field")
        })?
        .to_string();

    let version = def
        .get("version")
        .and_then(github_scm::json_workflow_version)
        .unwrap_or_else(|| "0.0.0".to_string());

    let description = def
        .get("description")
        .and_then(|v| v.as_str())
        .map(String::from);

    let repo_slug = format!("{}/{}", slug.owner, slug.name);
    let upstream_url = format!(
        "https://github.com/{}/blob/{}/{}",
        repo_slug,
        commit_sha,
        req.workflow_path.trim_start_matches('/')
    );
    let catalog_metadata = catalog_metadata_json(&def, upstream_url);

    let row = WorkflowRepo::new(state.db())
        .create_global_catalog_git(
            org_id,
            &CreateGlobalCatalogGit {
                name,
                version,
                definition: def,
                description,
                tags: Vec::new(),
                scm_repository: repo_slug,
                scm_ref: req.git_ref,
                scm_path: req.workflow_path,
                scm_revision: commit_sha,
                catalog_metadata,
                submitted_by: user.user_id,
            },
        )
        .await?;

    Ok(Json(WorkflowResponse::from(row)))
}

#[derive(Debug, Deserialize)]
pub struct CatalogVersionsQuery {
    #[serde(default)]
    pub q: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CatalogVersionsPage {
    pub workflow_name: String,
    pub versions: Vec<WorkflowResponse>,
    #[serde(default)]
    pub has_more: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/v1/workflows/{workflow_id}/catalog-versions",
    tag = "workflows",
)]
#[instrument(skip(state))]
async fn list_catalog_versions(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(workflow_id): Path<String>,
    pagination: Pagination,
    Query(query): Query<CatalogVersionsQuery>,
) -> ApiResult<Json<CatalogVersionsPage>> {
    let uuid: Uuid = workflow_id
        .parse()
        .map_err(|_| ApiError::bad_request("invalid workflow ID"))?;

    let anchor = WorkflowRepo::new(state.db())
        .get_by_id(user.org_id, uuid)
        .await
        .map_err(|_| ApiError::not_found("workflow not found"))?;

    if anchor.scope != met_store::repos::WorkflowScope::Global {
        return Err(ApiError::bad_request("not a global catalog workflow row"));
    }

    let offset = parse_catalog_list_offset(pagination.cursor.as_deref());
    let rows = WorkflowRepo::new(state.db())
        .list_global_catalog_versions(
            user.org_id,
            &anchor.name,
            query.q.as_deref(),
            pagination.sql_limit(),
            offset,
        )
        .await?;

    let limit_usize = pagination.limit as usize;
    let mut versions: Vec<WorkflowResponse> =
        rows.into_iter().map(WorkflowResponse::from).collect();
    let fetched = versions.len();
    let has_more = fetched > limit_usize;
    if has_more {
        versions.pop();
    }
    let count = versions.len();
    let next_cursor = if has_more {
        Some((offset as usize + count).to_string())
    } else {
        None
    };

    Ok(Json(CatalogVersionsPage {
        workflow_name: anchor.name,
        versions,
        has_more,
        next_cursor,
    }))
}
