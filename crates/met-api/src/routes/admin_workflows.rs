//! Admin actions for org workflow catalog (approve, trust, soft-delete, set-deprecation).
//!
//! `require_admin` (SuperAdmin only) still guards delete and re-import.
//! Approve / reject / trust / untrust are also accessible to SecurityEngineer
//! (`workflow:approve`, `workflow:reject`, `workflow:trust`, `workflow:untrust`).
//! SecurityEngineer callers must supply a non-empty `note`; SuperAdmins may omit it.

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use met_store::repos::{WorkflowRepo, WorkflowTrustState};
use serde::{Deserialize, Serialize};
use tracing::instrument;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::routes::admin::require_admin;
use crate::routes::workflows::WorkflowResponse;
use crate::{
    error::{ApiError, ApiResult},
    extractors::{Auth, CurrentUser},
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/workflows/{workflow_id}/approve",
            post(approve_catalog_workflow),
        )
        .route(
            "/workflows/{workflow_id}/reject",
            post(reject_catalog_workflow),
        )
        .route(
            "/workflows/{workflow_id}/trust",
            post(trust_catalog_workflow),
        )
        .route(
            "/workflows/{workflow_id}/untrust",
            post(untrust_catalog_workflow),
        )
        .route(
            "/workflows/{workflow_id}/delete",
            post(soft_delete_catalog_workflow),
        )
        .route(
            "/workflows/{workflow_id}/re-import",
            post(re_import_catalog_workflow),
        )
        .route(
            "/workflows/{workflow_id}/set-deprecation",
            post(set_workflow_deprecation),
        )
        .route(
            "/workflows/{workflow_id}/moderation-events",
            get(list_moderation_events),
        )
}

// ============================================================================
// Shared guards and helpers
// ============================================================================

/// Request body shared by approve / reject / trust / untrust.
#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct WorkflowModerationRequest {
    /// Markdown note explaining the decision.
    /// Required when the caller has `workflow:*` permission (SecurityEngineer)
    /// rather than `*` (SuperAdmin). May be multi-line.
    #[serde(default)]
    pub note: Option<String>,
}

/// Require either SuperAdmin (`*`) or the named permission.
/// Returns the caller's note and whether a note is required.
fn require_workflow_moderator(
    user: &CurrentUser,
    action_permission: &str,
) -> ApiResult<Option<String>> {
    if user.has_permission("*") {
        return Ok(None); // superadmin: note is optional
    }
    if user.has_permission(action_permission) {
        return Ok(None); // permission satisfied; note check happens at call site
    }
    Err(ApiError::forbidden(format!(
        "requires '{action_permission}' (or '*') permission"
    )))
}

/// Validate and return the note. For SecurityEngineer callers (non-`*`) a non-empty note is required.
fn validate_note(user: &CurrentUser, note: Option<String>) -> ApiResult<Option<String>> {
    if !user.has_permission("*") {
        // Non-superadmin must supply a non-empty note
        let n = note.as_deref().unwrap_or("").trim().to_string();
        if n.is_empty() {
            return Err(ApiError::bad_request(
                "a note is required when approving/rejecting/trusting/untrusting as SecurityEngineer",
            ));
        }
        return Ok(Some(n));
    }
    Ok(note.map(|n| n.trim().to_string()).filter(|n| !n.is_empty()))
}

/// Insert a row into `workflow_moderation_events`.
async fn record_moderation_event(
    state: &AppState,
    workflow_id: Uuid,
    org_id: uuid::Uuid,
    action: &str,
    actor_user_id: uuid::Uuid,
    note: Option<&str>,
) -> ApiResult<()> {
    sqlx::query(
        r#"
        INSERT INTO workflow_moderation_events
            (workflow_id, org_id, action, actor_user_id, note)
        VALUES ($1, $2, $3, $4, $5)
        "#,
    )
    .bind(workflow_id)
    .bind(org_id)
    .bind(action)
    .bind(actor_user_id)
    .bind(note)
    .execute(state.db())
    .await
    .map_err(met_store::StoreError::from)?;
    Ok(())
}

// ============================================================================
// Responses
// ============================================================================

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminWorkflowOpResponse {
    pub workflow: WorkflowResponse,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminWorkflowDeleteResponse {
    pub ok: bool,
}

// ============================================================================
// Approve / Reject / Trust / Untrust
// ============================================================================

#[utoipa::path(
    post,
    path = "/api/v1/admin/workflows/{workflow_id}/approve",
    tag = "admin"
)]
#[instrument(skip(state, body))]
async fn approve_catalog_workflow(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(workflow_id): Path<String>,
    body: Option<Json<WorkflowModerationRequest>>,
) -> ApiResult<Json<AdminWorkflowOpResponse>> {
    require_workflow_moderator(&user, "workflow:approve")?;
    let note = validate_note(&user, body.and_then(|b| b.0.note))?;

    let id: Uuid = workflow_id
        .parse()
        .map_err(|_| ApiError::bad_request("invalid workflow ID"))?;
    let row = WorkflowRepo::new(state.db())
        .approve_global(user.org_id, id, user.user_id)
        .await
        .map_err(|_| ApiError::not_found("workflow not found"))?;

    record_moderation_event(
        &state,
        id,
        user.org_id.as_uuid(),
        "approve",
        user.user_id.as_uuid(),
        note.as_deref(),
    )
    .await?;

    Ok(Json(AdminWorkflowOpResponse {
        workflow: WorkflowResponse::from(row),
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/admin/workflows/{workflow_id}/reject",
    tag = "admin"
)]
#[instrument(skip(state, body))]
async fn reject_catalog_workflow(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(workflow_id): Path<String>,
    body: Option<Json<WorkflowModerationRequest>>,
) -> ApiResult<Json<AdminWorkflowOpResponse>> {
    require_workflow_moderator(&user, "workflow:reject")?;
    let note = validate_note(&user, body.and_then(|b| b.0.note))?;

    let id: Uuid = workflow_id
        .parse()
        .map_err(|_| ApiError::bad_request("invalid workflow ID"))?;
    let row = WorkflowRepo::new(state.db())
        .reject_global(user.org_id, id, user.user_id)
        .await
        .map_err(|_| ApiError::not_found("workflow not found"))?;

    record_moderation_event(
        &state,
        id,
        user.org_id.as_uuid(),
        "reject",
        user.user_id.as_uuid(),
        note.as_deref(),
    )
    .await?;

    Ok(Json(AdminWorkflowOpResponse {
        workflow: WorkflowResponse::from(row),
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/admin/workflows/{workflow_id}/trust",
    tag = "admin"
)]
#[instrument(skip(state, body))]
async fn trust_catalog_workflow(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(workflow_id): Path<String>,
    body: Option<Json<WorkflowModerationRequest>>,
) -> ApiResult<Json<AdminWorkflowOpResponse>> {
    require_workflow_moderator(&user, "workflow:trust")?;
    let note = validate_note(&user, body.and_then(|b| b.0.note))?;

    let id: Uuid = workflow_id
        .parse()
        .map_err(|_| ApiError::bad_request("invalid workflow ID"))?;
    let row = WorkflowRepo::new(state.db())
        .set_global_trust(user.org_id, id, WorkflowTrustState::Trusted, user.user_id)
        .await
        .map_err(|_| ApiError::not_found("workflow not found"))?;

    record_moderation_event(
        &state,
        id,
        user.org_id.as_uuid(),
        "trust",
        user.user_id.as_uuid(),
        note.as_deref(),
    )
    .await?;

    Ok(Json(AdminWorkflowOpResponse {
        workflow: WorkflowResponse::from(row),
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/admin/workflows/{workflow_id}/untrust",
    tag = "admin"
)]
#[instrument(skip(state, body))]
async fn untrust_catalog_workflow(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(workflow_id): Path<String>,
    body: Option<Json<WorkflowModerationRequest>>,
) -> ApiResult<Json<AdminWorkflowOpResponse>> {
    require_workflow_moderator(&user, "workflow:untrust")?;
    let note = validate_note(&user, body.and_then(|b| b.0.note))?;

    let id: Uuid = workflow_id
        .parse()
        .map_err(|_| ApiError::bad_request("invalid workflow ID"))?;
    let row = WorkflowRepo::new(state.db())
        .set_global_trust(user.org_id, id, WorkflowTrustState::Untrusted, user.user_id)
        .await
        .map_err(|_| ApiError::not_found("workflow not found"))?;

    record_moderation_event(
        &state,
        id,
        user.org_id.as_uuid(),
        "untrust",
        user.user_id.as_uuid(),
        note.as_deref(),
    )
    .await?;

    Ok(Json(AdminWorkflowOpResponse {
        workflow: WorkflowResponse::from(row),
    }))
}

// ============================================================================
// Delete / Re-import (SuperAdmin only)
// ============================================================================

#[utoipa::path(
    post,
    path = "/api/v1/admin/workflows/{workflow_id}/delete",
    tag = "admin"
)]
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

    record_moderation_event(
        &state,
        id,
        admin.org_id.as_uuid(),
        "delete",
        admin.user_id.as_uuid(),
        None,
    )
    .await?;

    Ok(Json(AdminWorkflowDeleteResponse { ok: true }))
}

/// Re-import an existing Git-sourced catalog workflow at the tip of its stored ref.
#[utoipa::path(
    post,
    path = "/api/v1/admin/workflows/{workflow_id}/re-import",
    tag = "admin"
)]
#[instrument(skip(state))]
async fn re_import_catalog_workflow(
    State(state): State<AppState>,
    Auth(admin): Auth,
    Path(workflow_id): Path<String>,
) -> ApiResult<Json<AdminWorkflowOpResponse>> {
    require_admin(&admin)?;

    let id: Uuid = workflow_id
        .parse()
        .map_err(|_| ApiError::bad_request("invalid workflow ID"))?;
    let existing = WorkflowRepo::new(state.db())
        .get_by_id(admin.org_id, id)
        .await
        .map_err(|_| ApiError::not_found("workflow not found"))?;

    let repo_slug = existing
        .scm_repository
        .as_deref()
        .ok_or_else(|| ApiError::bad_request("workflow has no SCM repository"))?
        .to_string();
    let git_ref = existing
        .scm_ref
        .as_deref()
        .ok_or_else(|| ApiError::bad_request("workflow has no SCM ref"))?
        .to_string();
    let workflow_path = existing
        .scm_path
        .as_deref()
        .ok_or_else(|| ApiError::bad_request("workflow has no SCM path"))?
        .to_string();
    let credentials_path = existing
        .catalog_metadata
        .get("catalog_scm_credentials_path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            ApiError::bad_request("workflow has no stored credentials path for re-import")
        })?
        .to_string();

    let req = crate::routes::workflows_catalog::ImportCatalogWorkflowGitRequest {
        repository: repo_slug,
        git_ref,
        workflow_path,
        credentials_path,
    };

    let row = crate::routes::workflows_catalog::import_catalog_workflow_git_execute(
        &state,
        admin.user_id,
        admin.org_id,
        met_core::ids::ProjectId::from_uuid(uuid::Uuid::nil()),
        req,
    )
    .await?;

    Ok(Json(AdminWorkflowOpResponse {
        workflow: WorkflowResponse::from(row),
    }))
}

// ============================================================================
// Set deprecation period
// ============================================================================

#[derive(Debug, Deserialize, ToSchema)]
pub struct SetDeprecationRequest {
    /// Set to `null` to clear the deprecation period.
    pub deprecated_after: Option<DateTime<Utc>>,
    /// Human-readable markdown note for the deprecation reason.
    #[serde(default)]
    pub deprecation_note: Option<String>,
}

#[utoipa::path(
    post,
    path = "/api/v1/admin/workflows/{workflow_id}/set-deprecation",
    request_body = SetDeprecationRequest,
    tag = "admin"
)]
#[instrument(skip(state, req))]
async fn set_workflow_deprecation(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(workflow_id): Path<String>,
    Json(req): Json<SetDeprecationRequest>,
) -> ApiResult<Json<AdminWorkflowOpResponse>> {
    // Both SuperAdmin and SecurityEngineer may set deprecation periods.
    if !user.has_any_permission(&["*", "workflow:approve"]) {
        return Err(ApiError::forbidden(
            "setting a deprecation period requires workflow:approve (or *) permission",
        ));
    }

    let id: Uuid = workflow_id
        .parse()
        .map_err(|_| ApiError::bad_request("invalid workflow ID"))?;

    let row = WorkflowRepo::new(state.db())
        .set_deprecation(
            user.org_id,
            id,
            req.deprecated_after,
            req.deprecation_note.as_deref(),
        )
        .await
        .map_err(|_| ApiError::not_found("workflow not found"))?;

    Ok(Json(AdminWorkflowOpResponse {
        workflow: WorkflowResponse::from(row),
    }))
}

// ============================================================================
// Moderation event history
// ============================================================================

#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct ModerationEventRow {
    pub id: Uuid,
    pub workflow_id: Uuid,
    pub action: String,
    pub actor_user_id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor_email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor_display_name: Option<String>,
    pub note: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ModerationEventsResponse {
    pub events: Vec<ModerationEventRow>,
}

#[utoipa::path(
    get,
    path = "/api/v1/admin/workflows/{workflow_id}/moderation-events",
    tag = "admin"
)]
#[instrument(skip(state))]
async fn list_moderation_events(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(workflow_id): Path<String>,
) -> ApiResult<Json<ModerationEventsResponse>> {
    // Auditor, SecurityEngineer, or SuperAdmin may view the history.
    if !user.has_any_permission(&["*", "workflow:approve", "audit:read", "read:*"]) {
        return Err(ApiError::forbidden(
            "insufficient permissions to view moderation events",
        ));
    }

    let id: Uuid = workflow_id
        .parse()
        .map_err(|_| ApiError::bad_request("invalid workflow ID"))?;

    // Verify the workflow belongs to this org before returning events.
    let _ = WorkflowRepo::new(state.db())
        .get_by_id(user.org_id, id)
        .await
        .map_err(|_| ApiError::not_found("workflow not found"))?;

    let events: Vec<ModerationEventRow> = sqlx::query_as(
        r#"
        SELECT
            m.id,
            m.workflow_id,
            m.action,
            m.actor_user_id,
            u.email AS actor_email,
            u.display_name AS actor_display_name,
            m.note,
            m.created_at
        FROM workflow_moderation_events m
        LEFT JOIN users u ON u.id = m.actor_user_id AND u.org_id = m.org_id
        WHERE m.workflow_id = $1 AND m.org_id = $2
        ORDER BY m.created_at DESC
        LIMIT 200
        "#,
    )
    .bind(id)
    .bind(user.org_id.as_uuid())
    .fetch_all(state.db())
    .await
    .map_err(met_store::StoreError::from)?;

    Ok(Json(ModerationEventsResponse { events }))
}
