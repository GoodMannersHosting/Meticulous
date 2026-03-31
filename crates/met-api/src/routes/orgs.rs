//! Organization CRUD routes.

use axum::{
    extract::{Path, State},
    routing::{delete, get, patch, post},
    Json, Router,
};
use met_core::{
    ids::OrganizationId,
    models::{CreateOrganization, Organization, UpdateOrganization},
};
use met_store::repos::OrganizationRepo;
use serde::{Deserialize, Serialize};
use tracing::instrument;
use utoipa::ToSchema;

use crate::{
    error::{ApiError, ApiResult},
    extractors::{Auth, Pagination, PaginatedResponse},
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/orgs", get(list_orgs).post(create_org))
        .route(
            "/orgs/{id}",
            get(get_org).patch(update_org).delete(delete_org),
        )
}

#[derive(Debug, Serialize, ToSchema)]
pub struct OrgResponse {
    #[serde(flatten)]
    #[schema(value_type = Object)]
    pub org: Organization,
}

#[utoipa::path(
    get,
    path = "/api/v1/orgs",
    params(
        ("cursor" = Option<String>, Query, description = "Pagination cursor"),
        ("limit" = Option<u32>, Query, description = "Items per page"),
    ),
    responses(
        (status = 200, description = "List of organizations", body = serde_json::Value),
        (status = 401, description = "Unauthorized"),
    ),
    tag = "organizations",
)]
#[instrument(skip(state))]
async fn list_orgs(
    State(state): State<AppState>,
    Auth(user): Auth,
    pagination: Pagination,
) -> ApiResult<Json<PaginatedResponse<OrgResponse>>> {
    let repo = OrganizationRepo::new(state.db());

    let orgs = if user.has_permission("*") {
        repo.list(pagination.sql_limit(), 0).await?
    } else {
        let org = repo.get(user.org_id).await?;
        vec![org]
    };

    let response = PaginatedResponse::new(
        orgs.into_iter().map(|org| OrgResponse { org }).collect(),
        pagination.limit,
        |o| o.org.id.to_string(),
    );

    Ok(Json(response))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateOrgRequest {
    pub name: String,
    pub slug: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/orgs",
    request_body = CreateOrgRequest,
    responses(
        (status = 200, description = "Organization created", body = OrgResponse),
        (status = 400, description = "Bad request"),
        (status = 403, description = "Forbidden"),
    ),
    tag = "organizations",
)]
#[instrument(skip(state, req))]
async fn create_org(
    State(state): State<AppState>,
    Auth(user): Auth,
    Json(req): Json<CreateOrgRequest>,
) -> ApiResult<Json<OrgResponse>> {
    if !user.has_permission("*") {
        return Err(ApiError::forbidden("platform admin access required"));
    }

    if req.name.is_empty() {
        return Err(ApiError::bad_request("name is required"));
    }

    if req.slug.is_empty() {
        return Err(ApiError::bad_request("slug is required"));
    }

    if !req.slug.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
        return Err(ApiError::bad_request(
            "slug must contain only alphanumeric characters, hyphens, and underscores",
        ));
    }

    let repo = OrganizationRepo::new(state.db());
    let org = repo
        .create(&CreateOrganization {
            name: req.name,
            slug: req.slug,
        })
        .await?;

    tracing::info!(org_id = %org.id, org_slug = %org.slug, "organization created");

    Ok(Json(OrgResponse { org }))
}

#[utoipa::path(
    get,
    path = "/api/v1/orgs/{id}",
    params(("id" = String, Path, description = "Organization ID")),
    responses(
        (status = 200, description = "Organization details", body = OrgResponse),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Organization not found"),
    ),
    tag = "organizations",
)]
#[instrument(skip(state))]
async fn get_org(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(id): Path<OrganizationId>,
) -> ApiResult<Json<OrgResponse>> {
    if id != user.org_id && !user.has_permission("*") {
        return Err(ApiError::forbidden("cannot access other organizations"));
    }

    let repo = OrganizationRepo::new(state.db());
    let org = repo.get(id).await?;
    Ok(Json(OrgResponse { org }))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateOrgRequest {
    pub name: Option<String>,
}

#[utoipa::path(
    patch,
    path = "/api/v1/orgs/{id}",
    params(("id" = String, Path, description = "Organization ID")),
    request_body = UpdateOrgRequest,
    responses(
        (status = 200, description = "Organization updated", body = OrgResponse),
        (status = 400, description = "Bad request"),
        (status = 403, description = "Forbidden"),
    ),
    tag = "organizations",
)]
#[instrument(skip(state, req))]
async fn update_org(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(id): Path<OrganizationId>,
    Json(req): Json<UpdateOrgRequest>,
) -> ApiResult<Json<OrgResponse>> {
    if id != user.org_id && !user.has_permission("*") {
        return Err(ApiError::forbidden("cannot modify other organizations"));
    }

    if !user.has_permission("*") && !user.has_permission("org:admin") {
        return Err(ApiError::forbidden("org admin access required"));
    }

    if let Some(ref name) = req.name {
        if name.is_empty() {
            return Err(ApiError::bad_request("name cannot be empty"));
        }
    }

    let repo = OrganizationRepo::new(state.db());
    let org = repo
        .update(id, &UpdateOrganization { name: req.name })
        .await?;

    tracing::info!(org_id = %id, "organization updated");

    Ok(Json(OrgResponse { org }))
}

#[utoipa::path(
    delete,
    path = "/api/v1/orgs/{id}",
    params(("id" = String, Path, description = "Organization ID")),
    responses(
        (status = 200, description = "Organization deleted"),
        (status = 400, description = "Cannot delete own organization"),
        (status = 403, description = "Forbidden"),
    ),
    tag = "organizations",
)]
#[instrument(skip(state))]
async fn delete_org(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(id): Path<OrganizationId>,
) -> ApiResult<Json<serde_json::Value>> {
    if !user.has_permission("*") {
        return Err(ApiError::forbidden("platform admin access required"));
    }

    if id == user.org_id {
        return Err(ApiError::bad_request("cannot delete your own organization"));
    }

    let repo = OrganizationRepo::new(state.db());
    repo.delete(id).await?;

    tracing::warn!(admin_id = %user.user_id, org_id = %id, "organization deleted");

    Ok(Json(serde_json::json!({ "message": "organization deleted" })))
}
