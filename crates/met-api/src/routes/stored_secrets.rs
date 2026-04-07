//! CRUD for platform-stored secrets (`builtin_secrets`). Plaintext is accepted only on create/rotate; never returned.

use std::collections::HashMap;

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::{delete, get, post},
};
use chrono::{DateTime, Utc};
use met_core::ids::{OrganizationId, PipelineId, ProjectId};
use met_secrets::parse_github_app_credentials;
use met_store::repos::{
    BuiltinSecretMetaRow, BuiltinSecretsRepo, PipelineRepo, ProjectRepo, StoredSecretKind,
};
use serde::{Deserialize, Serialize};
use tracing::instrument;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    error::{ApiError, ApiResult, STORED_SECRETS_UNAVAILABLE},
    extractors::{Auth, CurrentUser},
    state::AppState,
};

#[must_use]
fn user_may_manage_org_stored_secrets(user: &CurrentUser) -> bool {
    user.has_any_permission(&["*", "org:admin"])
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/projects/{project_id}/stored-secret-versions",
            get(list_stored_secret_versions),
        )
        .route(
            "/projects/{project_id}/stored-secrets",
            get(list_stored_secrets).post(create_stored_secret),
        )
        .route(
            "/stored-secrets/{id}/activate",
            post(activate_stored_secret_version),
        )
        .route(
            "/stored-secrets/{id}/permanent",
            delete(purge_stored_secret_version),
        )
        .route("/stored-secrets/{id}/rotate", post(rotate_stored_secret))
        .route("/stored-secrets/{id}", delete(delete_stored_secret))
}

#[derive(Debug, Deserialize)]
struct ListStoredQuery {
    pipeline_id: Option<PipelineId>,
}

#[derive(Debug, Deserialize)]
struct ListStoredSecretVersionsQuery {
    path: String,
    pipeline_id: Option<PipelineId>,
    /// When true, list versions for org-wide secrets (`project_id` NULL) at this path.
    #[serde(default)]
    organization_wide: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct StoredSecretResponse {
    pub id: Uuid,
    #[schema(value_type = Option<String>)]
    pub project_id: Option<ProjectId>,
    #[schema(value_type = Option<String>)]
    pub pipeline_id: Option<PipelineId>,
    pub path: String,
    pub kind: String,
    pub version: i32,
    pub metadata: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Org-wide only: when `false`, not exposed to pipelines or project secret UIs (catalog SCM may still use).
    pub propagate_to_projects: bool,
}

impl From<BuiltinSecretMetaRow> for StoredSecretResponse {
    fn from(r: BuiltinSecretMetaRow) -> Self {
        Self {
            id: r.id,
            project_id: r.project_id.map(ProjectId::from_uuid),
            pipeline_id: r.pipeline_id.map(PipelineId::from_uuid),
            path: r.path,
            kind: r.kind,
            version: r.version,
            metadata: r.metadata,
            description: r.description,
            created_at: r.created_at,
            updated_at: r.updated_at,
            propagate_to_projects: r.propagate_to_projects,
        }
    }
}

fn dedupe_latest(rows: Vec<BuiltinSecretMetaRow>) -> Vec<BuiltinSecretMetaRow> {
    let mut best: HashMap<String, BuiltinSecretMetaRow> = HashMap::new();
    for r in rows {
        let k = format!("{}|{:?}|{:?}", r.path, r.project_id, r.pipeline_id);
        match best.get_mut(&k) {
            Some(e) => {
                if r.version > e.version {
                    *e = r;
                }
            }
            None => {
                best.insert(k, r);
            }
        }
    }
    let mut v: Vec<_> = best.into_values().collect();
    v.sort_by(|a, b| a.path.cmp(&b.path));
    v
}

#[utoipa::path(
    get,
    path = "/api/v1/projects/{project_id}/stored-secrets",
    params(
        ("project_id" = String, Path, description = "Project ID"),
        ("pipeline_id" = Option<String>, Query, description = "When set, list secrets scoped to this pipeline only"),
    ),
    responses(
        (status = 200, description = "Metadata only (no secret values)", body = Vec<StoredSecretResponse>),
        (status = 403, description = "Forbidden"),
    ),
    tag = "stored_secrets",
)]
#[instrument(skip(state))]
async fn list_stored_secrets(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(project_id): Path<ProjectId>,
    Query(q): Query<ListStoredQuery>,
) -> ApiResult<Json<Vec<StoredSecretResponse>>> {
    if !user.can_access_project(project_id) {
        return Err(ApiError::forbidden("no access to this project"));
    }

    let org_id = ProjectRepo::new(state.db())
        .get(project_id)
        .await
        .map_err(met_store::StoreError::from)?
        .org_id;

    let repo = BuiltinSecretsRepo::new(state.db());
    let rows = if let Some(pid) = q.pipeline_id {
        let pl = PipelineRepo::new(state.db())
            .get(pid)
            .await
            .map_err(met_store::StoreError::from)?;
        if pl.project_id.as_uuid() != project_id.as_uuid() {
            return Err(ApiError::bad_request("pipeline does not belong to project"));
        }
        repo.list_for_pipeline(org_id, project_id, pid).await?
    } else {
        repo.list_for_project(org_id, project_id).await?
    };

    let out = dedupe_latest(rows)
        .into_iter()
        .map(StoredSecretResponse::from)
        .collect();
    Ok(Json(out))
}

#[utoipa::path(
    get,
    path = "/api/v1/projects/{project_id}/stored-secret-versions",
    params(
        ("project_id" = String, Path, description = "Project ID"),
        ("path" = String, Query, description = "Secret logical name"),
        ("pipeline_id" = Option<String>, Query, description = "Omit for project-wide secret scope; set for pipeline-scoped secrets"),
    ),
    responses(
        (status = 200, description = "All versions (metadata only)", body = Vec<StoredSecretResponse>),
        (status = 403, description = "Forbidden"),
    ),
    tag = "stored_secrets",
)]
#[instrument(skip(state))]
async fn list_stored_secret_versions(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(project_id): Path<ProjectId>,
    Query(q): Query<ListStoredSecretVersionsQuery>,
) -> ApiResult<Json<Vec<StoredSecretResponse>>> {
    if !user.can_access_project(project_id) {
        return Err(ApiError::forbidden("no access to this project"));
    }

    let path = q.path.trim();
    if path.is_empty() {
        return Err(ApiError::bad_request("path is required"));
    }

    let org_id = ProjectRepo::new(state.db())
        .get(project_id)
        .await
        .map_err(met_store::StoreError::from)?
        .org_id;

    if q.organization_wide {
        if q.pipeline_id.is_some() {
            return Err(ApiError::bad_request(
                "organization_wide conflicts with pipeline_id",
            ));
        }
    } else if let Some(pid) = q.pipeline_id {
        let pl = PipelineRepo::new(state.db())
            .get(pid)
            .await
            .map_err(met_store::StoreError::from)?;
        if pl.project_id.as_uuid() != project_id.as_uuid() {
            return Err(ApiError::bad_request("pipeline does not belong to project"));
        }
    }

    let repo = BuiltinSecretsRepo::new(state.db());
    let (scope_project, scope_pipeline) = if q.organization_wide {
        (None, None)
    } else {
        (Some(project_id), q.pipeline_id)
    };
    let rows = repo
        .list_versions_for_scope(org_id, scope_project, scope_pipeline, path)
        .await
        .map_err(met_store::StoreError::from)?;
    Ok(Json(
        rows.into_iter().map(StoredSecretResponse::from).collect(),
    ))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateStoredSecretRequest {
    /// Logical name (`builtin_secrets.path`); becomes YAML `stored.name`.
    pub path: String,
    pub kind: String,
    /// One-time plaintext (never stored or returned after this call).
    pub value: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    #[schema(value_type = Option<String>)]
    pub pipeline_id: Option<PipelineId>,
    /// Set to `"organization"` for org-wide secrets (`project_id` NULL). Requires `org:admin` or `*`.
    #[serde(default)]
    pub scope: Option<String>,
    /// When `scope` is organization: if `false`, secret is **not** visible to pipelines, jobs, or project secret lists
    /// (use for platform SCM such as global workflow catalog import). Default `true`.
    #[serde(default)]
    pub propagate_to_projects: Option<bool>,
}

#[utoipa::path(
    post,
    path = "/api/v1/projects/{project_id}/stored-secrets",
    request_body = CreateStoredSecretRequest,
    params(
        ("project_id" = String, Path, description = "Project ID"),
    ),
    responses(
        (status = 200, description = "Created", body = StoredSecretResponse),
        (status = 400, description = "Bad request"),
        (status = 403, description = "Forbidden"),
        (status = 503, description = "Stored secrets not configured"),
    ),
    tag = "stored_secrets",
)]
#[instrument(skip(state, req))]
async fn create_stored_secret(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(project_id): Path<ProjectId>,
    Json(req): Json<CreateStoredSecretRequest>,
) -> ApiResult<Json<StoredSecretResponse>> {
    if !user.can_access_project(project_id) {
        return Err(ApiError::forbidden("no access to this project"));
    }

    let Some(ref crypto) = state.stored_secret_crypto else {
        return Err(ApiError::unavailable(STORED_SECRETS_UNAVAILABLE));
    };

    if req.path.trim().is_empty() {
        return Err(ApiError::bad_request("path is required"));
    }
    if req.value.is_empty() {
        return Err(ApiError::bad_request("value is required"));
    }

    let kind =
        StoredSecretKind::parse(&req.kind).map_err(|e| ApiError::bad_request(e.to_string()))?;

    if kind == StoredSecretKind::GithubApp {
        parse_github_app_credentials(req.value.trim()).map_err(|e| {
            ApiError::bad_request(format!(
                "github_app value must be JSON with app_id, installation_id, private_key_pem, and optional extra fields: {e}"
            ))
        })?;
    }

    let project = ProjectRepo::new(state.db())
        .get(project_id)
        .await
        .map_err(met_store::StoreError::from)?;
    let org_id = project.org_id;

    let org_wide = req
        .scope
        .as_deref()
        .is_some_and(|s| s.eq_ignore_ascii_case("organization"));

    if org_wide {
        if !user_may_manage_org_stored_secrets(&user) {
            return Err(ApiError::forbidden(
                "org-wide stored secrets require org:admin (or *) permission",
            ));
        }
        if req.pipeline_id.is_some() {
            return Err(ApiError::bad_request(
                "organization-scoped secrets cannot be pipeline-scoped",
            ));
        }
    } else {
        if req.propagate_to_projects == Some(false) {
            return Err(ApiError::bad_request(
                "propagate_to_projects may only be set when scope is organization",
            ));
        }
    }
    if let Some(pid) = req.pipeline_id {
        let pl = PipelineRepo::new(state.db())
            .get(pid)
            .await
            .map_err(met_store::StoreError::from)?;
        if pl.project_id.as_uuid() != project_id.as_uuid() {
            return Err(ApiError::bad_request("pipeline does not belong to project"));
        }
    }

    let repo = BuiltinSecretsRepo::new(state.db());
    let (store_project, store_pipeline) = if org_wide {
        (None, None)
    } else {
        (Some(project_id), req.pipeline_id)
    };
    let version = repo
        .next_version(org_id, store_project, store_pipeline, &req.path)
        .await
        .map_err(met_store::StoreError::from)?;

    let (ct, nonce, key_id) = crypto
        .encrypt(req.value.as_bytes())
        .map_err(|e| ApiError::internal(e.to_string()))?;

    let meta = serde_json::json!({});
    let propagate_to_projects = if org_wide {
        req.propagate_to_projects.unwrap_or(true)
    } else {
        true
    };
    let row = repo
        .insert_encrypted(
            org_id,
            store_project,
            store_pipeline,
            &req.path,
            kind,
            &meta,
            req.description.as_deref(),
            &ct,
            &nonce,
            &key_id,
            version,
            Some(user.user_id.as_uuid()),
            propagate_to_projects,
        )
        .await
        .map_err(met_store::StoreError::from)?;

    tracing::info!(stored_secret_id = %row.id, path = %req.path, "stored secret created");
    Ok(Json(StoredSecretResponse::from(row)))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RotateStoredSecretRequest {
    pub value: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/stored-secrets/{id}/rotate",
    request_body = RotateStoredSecretRequest,
    params(
        ("id" = String, Path, description = "Stored secret row ID (UUID)"),
    ),
    responses(
        (status = 200, description = "Rotated", body = StoredSecretResponse),
        (status = 400, description = "Bad request"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Not found"),
        (status = 503, description = "Stored secrets not configured"),
    ),
    tag = "stored_secrets",
)]
#[instrument(skip(state, req))]
async fn rotate_stored_secret(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(id): Path<Uuid>,
    Json(req): Json<RotateStoredSecretRequest>,
) -> ApiResult<Json<StoredSecretResponse>> {
    let Some(ref crypto) = state.stored_secret_crypto else {
        return Err(ApiError::unavailable(STORED_SECRETS_UNAVAILABLE));
    };

    if req.value.is_empty() {
        return Err(ApiError::bad_request("value is required"));
    }

    let repo = BuiltinSecretsRepo::new(state.db());
    let existing = repo
        .get_meta_by_id(id)
        .await
        .map_err(met_store::StoreError::from)?
        .ok_or_else(|| ApiError::not_found("stored secret not found"))?;

    let project_id = existing.project_id.map(ProjectId::from_uuid);
    match project_id {
        Some(pid) => {
            if !user.can_access_project(pid) {
                return Err(ApiError::forbidden("no access to this project"));
            }
        }
        None => {
            if !user_may_manage_org_stored_secrets(&user) {
                return Err(ApiError::forbidden(
                    "rotating org-wide secrets requires org:admin (or *) permission",
                ));
            }
        }
    }

    if existing.org_id != user.org_id.as_uuid() {
        return Err(ApiError::forbidden("wrong organization"));
    }

    if existing.kind == "github_app" {
        parse_github_app_credentials(req.value.trim()).map_err(|e| {
            ApiError::bad_request(format!(
                "github_app value must be JSON with app_id, installation_id, private_key_pem, and optional extra fields: {e}"
            ))
        })?;
    }

    let org_id = OrganizationId::from_uuid(existing.org_id);
    let kind = StoredSecretKind::parse(&existing.kind)
        .map_err(|e| ApiError::bad_request(e.to_string()))?;

    let version = repo
        .next_version(
            org_id,
            existing.project_id.map(ProjectId::from_uuid),
            existing.pipeline_id.map(PipelineId::from_uuid),
            &existing.path,
        )
        .await
        .map_err(met_store::StoreError::from)?;

    let (ct, nonce, key_id) = crypto
        .encrypt(req.value.as_bytes())
        .map_err(|e| ApiError::internal(e.to_string()))?;

    let row = repo
        .insert_encrypted(
            org_id,
            existing.project_id.map(ProjectId::from_uuid),
            existing.pipeline_id.map(PipelineId::from_uuid),
            &existing.path,
            kind,
            &existing.metadata,
            existing.description.as_deref(),
            &ct,
            &nonce,
            &key_id,
            version,
            Some(user.user_id.as_uuid()),
            existing.propagate_to_projects,
        )
        .await
        .map_err(met_store::StoreError::from)?;

    tracing::info!(stored_secret_id = %row.id, path = %existing.path, "stored secret rotated");
    Ok(Json(StoredSecretResponse::from(row)))
}

#[utoipa::path(
    delete,
    path = "/api/v1/stored-secrets/{id}",
    params(
        ("id" = String, Path, description = "Stored secret row ID (UUID)"),
    ),
    responses(
        (status = 200, description = "Soft-deleted", body = serde_json::Value),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Not found"),
    ),
    tag = "stored_secrets",
)]
#[instrument(skip(state))]
async fn delete_stored_secret(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let repo = BuiltinSecretsRepo::new(state.db());
    let existing = repo
        .get_meta_by_id(id)
        .await
        .map_err(met_store::StoreError::from)?
        .ok_or_else(|| ApiError::not_found("stored secret not found"))?;

    let project_id = existing.project_id.map(ProjectId::from_uuid);
    match project_id {
        Some(pid) => {
            if !user.can_access_project(pid) {
                return Err(ApiError::forbidden("no access to this project"));
            }
        }
        None => {
            if !user_may_manage_org_stored_secrets(&user) {
                return Err(ApiError::forbidden(
                    "deleting org-wide secrets requires org:admin (or *) permission",
                ));
            }
        }
    }
    if existing.org_id != user.org_id.as_uuid() {
        return Err(ApiError::forbidden("wrong organization"));
    }

    repo.soft_delete(id)
        .await
        .map_err(met_store::StoreError::from)?;
    Ok(Json(
        serde_json::json!({ "message": "stored secret deleted" }),
    ))
}

#[utoipa::path(
    post,
    path = "/api/v1/stored-secrets/{id}/activate",
    params(
        ("id" = String, Path, description = "Row id of the version to make current"),
    ),
    responses(
        (status = 200, description = "Newer versions soft-deleted", body = serde_json::Value),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Not found"),
    ),
    tag = "stored_secrets",
)]
#[instrument(skip(state))]
async fn activate_stored_secret_version(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let repo = BuiltinSecretsRepo::new(state.db());
    let anchor = repo
        .get_meta_by_id(id)
        .await
        .map_err(met_store::StoreError::from)?
        .ok_or_else(|| ApiError::not_found("stored secret not found"))?;

    let project_id = anchor.project_id.map(ProjectId::from_uuid);
    match project_id {
        Some(pid) => {
            if !user.can_access_project(pid) {
                return Err(ApiError::forbidden("no access to this project"));
            }
        }
        None => {
            if !user_may_manage_org_stored_secrets(&user) {
                return Err(ApiError::forbidden(
                    "org-wide secret rollback requires org:admin (or *) permission",
                ));
            }
        }
    }
    if anchor.org_id != user.org_id.as_uuid() {
        return Err(ApiError::forbidden("wrong organization"));
    }

    let activated = StoredSecretResponse::from(anchor.clone());
    let n = repo
        .soft_delete_versions_newer_than(&anchor)
        .await
        .map_err(met_store::StoreError::from)?;

    tracing::info!(
        stored_secret_id = %id,
        path = %anchor.path,
        invalidated_newer = n,
        "stored secret version activated (rollback)"
    );

    Ok(Json(serde_json::json!({
        "message": "newer versions removed from resolution",
        "invalidated_newer_versions": n,
        "activated": activated,
    })))
}

#[utoipa::path(
    delete,
    path = "/api/v1/stored-secrets/{id}/permanent",
    params(
        ("id" = String, Path, description = "Row id of the version to purge"),
    ),
    responses(
        (status = 200, description = "Row deleted from database", body = serde_json::Value),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Not found"),
    ),
    tag = "stored_secrets",
)]
#[instrument(skip(state))]
async fn purge_stored_secret_version(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let repo = BuiltinSecretsRepo::new(state.db());
    let existing = repo
        .get_meta_by_id_including_deleted(id)
        .await
        .map_err(met_store::StoreError::from)?
        .ok_or_else(|| ApiError::not_found("stored secret not found"))?;

    let project_id = existing.project_id.map(ProjectId::from_uuid);
    match project_id {
        Some(pid) => {
            if !user.can_access_project(pid) {
                return Err(ApiError::forbidden("no access to this project"));
            }
        }
        None => {
            if !user_may_manage_org_stored_secrets(&user) {
                return Err(ApiError::forbidden(
                    "purging org-wide secrets requires org:admin (or *) permission",
                ));
            }
        }
    }
    if existing.org_id != user.org_id.as_uuid() {
        return Err(ApiError::forbidden("wrong organization"));
    }

    repo.hard_delete_by_id(id)
        .await
        .map_err(met_store::StoreError::from)?;

    tracing::warn!(
        stored_secret_id = %id,
        path = %existing.path,
        version = existing.version,
        "stored secret version permanently purged"
    );

    Ok(Json(
        serde_json::json!({ "message": "stored secret version permanently deleted" }),
    ))
}
