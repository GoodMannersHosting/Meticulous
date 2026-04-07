//! Admin actions for org workflow catalog (approve, trust, soft-delete).

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::post,
};
use met_store::repos::{WorkflowRepo, WorkflowTrustState};
use serde::Serialize;
use tracing::instrument;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::routes::admin::require_admin;
use crate::routes::workflows::WorkflowResponse;
use crate::{
    error::{ApiError, ApiResult},
    extractors::Auth,
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/admin/workflows/{workflow_id}/approve",
            post(approve_catalog_workflow),
        )
        .route(
            "/admin/workflows/{workflow_id}/reject",
            post(reject_catalog_workflow),
        )
        .route(
            "/admin/workflows/{workflow_id}/trust",
            post(trust_catalog_workflow),
        )
        .route(
            "/admin/workflows/{workflow_id}/untrust",
            post(untrust_catalog_workflow),
        )
        .route(
            "/admin/workflows/{workflow_id}/delete",
            post(soft_delete_catalog_workflow),
        )
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminWorkflowOpResponse {
    pub workflow: WorkflowResponse,
}

#[utoipa::path(post, path = "/admin/workflows/{workflow_id}/approve", tag = "admin")]
#[instrument(skip(state))]
async fn approve_catalog_workflow(
    State(state): State<AppState>,
    Auth(admin): Auth,
    Path(workflow_id): Path<String>,
) -> ApiResult<Json<AdminWorkflowOpResponse>> {
    require_admin(&admin)?;
    let id: Uuid = workflow_id
        .parse()
        .map_err(|_| ApiError::bad_request("invalid workflow ID"))?;
    let row = WorkflowRepo::new(state.db())
        .approve_global(admin.org_id, id, admin.user_id)
        .await
        .map_err(|_| ApiError::not_found("workflow not found"))?;
    Ok(Json(AdminWorkflowOpResponse {
        workflow: WorkflowResponse::from(row),
    }))
}

#[utoipa::path(post, path = "/admin/workflows/{workflow_id}/reject", tag = "admin")]
#[instrument(skip(state))]
async fn reject_catalog_workflow(
    State(state): State<AppState>,
    Auth(admin): Auth,
    Path(workflow_id): Path<String>,
) -> ApiResult<Json<AdminWorkflowOpResponse>> {
    require_admin(&admin)?;
    let id: Uuid = workflow_id
        .parse()
        .map_err(|_| ApiError::bad_request("invalid workflow ID"))?;
    let row = WorkflowRepo::new(state.db())
        .reject_global(admin.org_id, id, admin.user_id)
        .await
        .map_err(|_| ApiError::not_found("workflow not found"))?;
    Ok(Json(AdminWorkflowOpResponse {
        workflow: WorkflowResponse::from(row),
    }))
}

#[utoipa::path(post, path = "/admin/workflows/{workflow_id}/trust", tag = "admin")]
#[instrument(skip(state))]
async fn trust_catalog_workflow(
    State(state): State<AppState>,
    Auth(admin): Auth,
    Path(workflow_id): Path<String>,
) -> ApiResult<Json<AdminWorkflowOpResponse>> {
    require_admin(&admin)?;
    let id: Uuid = workflow_id
        .parse()
        .map_err(|_| ApiError::bad_request("invalid workflow ID"))?;
    let row = WorkflowRepo::new(state.db())
        .set_global_trust(admin.org_id, id, WorkflowTrustState::Trusted, admin.user_id)
        .await
        .map_err(|_| ApiError::not_found("workflow not found"))?;
    Ok(Json(AdminWorkflowOpResponse {
        workflow: WorkflowResponse::from(row),
    }))
}

#[utoipa::path(post, path = "/admin/workflows/{workflow_id}/untrust", tag = "admin")]
#[instrument(skip(state))]
async fn untrust_catalog_workflow(
    State(state): State<AppState>,
    Auth(admin): Auth,
    Path(workflow_id): Path<String>,
) -> ApiResult<Json<AdminWorkflowOpResponse>> {
    require_admin(&admin)?;
    let id: Uuid = workflow_id
        .parse()
        .map_err(|_| ApiError::bad_request("invalid workflow ID"))?;
    let row = WorkflowRepo::new(state.db())
        .set_global_trust(
            admin.org_id,
            id,
            WorkflowTrustState::Untrusted,
            admin.user_id,
        )
        .await
        .map_err(|_| ApiError::not_found("workflow not found"))?;
    Ok(Json(AdminWorkflowOpResponse {
        workflow: WorkflowResponse::from(row),
    }))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminWorkflowDeleteResponse {
    pub ok: bool,
}

#[utoipa::path(post, path = "/admin/workflows/{workflow_id}/delete", tag = "admin")]
#[instrument(skip(state))]
async fn soft_delete_catalog_workflow(
    State(state): State<AppState>,
    Auth(admin): Auth,
    Path(workflow_id): Path<String>,
) -> ApiResult<Json<AdminWorkflowDeleteResponse>> {
    require_admin(&admin)?;
    let id: Uuid = workflow_id
        .parse()
        .map_err(|_| ApiError::bad_request("invalid workflow ID"))?;
    WorkflowRepo::new(state.db())
        .soft_delete_global(admin.org_id, id, admin.user_id)
        .await
        .map_err(|_| ApiError::not_found("workflow not found"))?;
    Ok(Json(AdminWorkflowDeleteResponse { ok: true }))
}
