//! Workflow auto-sync schedule API routes.
//!
//! Global org defaults and per-workflow overrides for how frequently
//! a Git-sourced catalog workflow is automatically re-imported.

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use tracing::instrument;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    error::{ApiError, ApiResult},
    extractors::Auth,
    routes::admin::require_admin,
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        // Global (org-level) defaults
        .route(
            "/workflows/catalog/sync-settings",
            get(get_sync_settings).put(put_sync_settings),
        )
        // Per-workflow schedule
        .route(
            "/workflows/{workflow_name}/sync-schedule",
            get(get_workflow_sync_schedule).put(put_workflow_sync_schedule),
        )
        .route(
            "/workflows/{workflow_name}/sync-now",
            post(trigger_sync_now),
        )
}

// ============================================================================
// Responses / requests
// ============================================================================

#[derive(Debug, Serialize, ToSchema)]
pub struct OrgSyncSettingsResponse {
    /// Global default sync interval in minutes. `null` = disabled globally.
    pub default_sync_interval_minutes: Option<i32>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PutOrgSyncSettingsRequest {
    /// Set to `null` or `0` to disable org-wide auto-sync.
    pub default_sync_interval_minutes: Option<i32>,
}

#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct WorkflowSyncScheduleRow {
    pub id: Uuid,
    pub org_id: Uuid,
    pub workflow_name: String,
    pub enabled: bool,
    pub interval_minutes: i32,
    pub last_synced_at: Option<DateTime<Utc>>,
    pub next_sync_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PutWorkflowSyncScheduleRequest {
    pub enabled: bool,
    /// Sync interval in minutes. `0` = disabled.
    pub interval_minutes: i32,
}

// ============================================================================
// Global sync settings
// ============================================================================

#[utoipa::path(
    get,
    path = "/api/v1/workflows/catalog/sync-settings",
    tag = "workflows",
)]
#[instrument(skip(state))]
async fn get_sync_settings(
    State(state): State<AppState>,
    Auth(user): Auth,
) -> ApiResult<Json<OrgSyncSettingsResponse>> {
    require_admin(&user)?;

    let row: Option<(Option<i32>,)> = sqlx::query_as(
        "SELECT default_workflow_sync_interval_minutes FROM organizations WHERE id = $1",
    )
    .bind(user.org_id.as_uuid())
    .fetch_optional(state.db())
    .await
    .map_err(met_store::StoreError::from)?;

    let minutes = row.and_then(|(v,)| v);
    Ok(Json(OrgSyncSettingsResponse {
        default_sync_interval_minutes: minutes,
    }))
}

#[utoipa::path(
    put,
    path = "/api/v1/workflows/catalog/sync-settings",
    request_body = PutOrgSyncSettingsRequest,
    tag = "workflows",
)]
#[instrument(skip(state, req))]
async fn put_sync_settings(
    State(state): State<AppState>,
    Auth(user): Auth,
    Json(req): Json<PutOrgSyncSettingsRequest>,
) -> ApiResult<Json<OrgSyncSettingsResponse>> {
    require_admin(&user)?;

    let interval = req
        .default_sync_interval_minutes
        .filter(|&v| v > 0);

    sqlx::query(
        "UPDATE organizations SET default_workflow_sync_interval_minutes = $1 WHERE id = $2",
    )
    .bind(interval)
    .bind(user.org_id.as_uuid())
    .execute(state.db())
    .await
    .map_err(met_store::StoreError::from)?;

    Ok(Json(OrgSyncSettingsResponse {
        default_sync_interval_minutes: interval,
    }))
}

// ============================================================================
// Per-workflow sync schedule
// ============================================================================

#[utoipa::path(
    get,
    path = "/api/v1/workflows/{workflow_name}/sync-schedule",
    tag = "workflows",
)]
#[instrument(skip(state))]
async fn get_workflow_sync_schedule(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(workflow_name): Path<String>,
) -> ApiResult<Json<Option<WorkflowSyncScheduleRow>>> {
    require_admin(&user)?;

    let row: Option<WorkflowSyncScheduleRow> = sqlx::query_as(
        r#"
        SELECT id, org_id, workflow_name, enabled, interval_minutes,
               last_synced_at, next_sync_at, created_at, updated_at
        FROM workflow_sync_schedules
        WHERE org_id = $1 AND workflow_name = $2
        "#,
    )
    .bind(user.org_id.as_uuid())
    .bind(&workflow_name)
    .fetch_optional(state.db())
    .await
    .map_err(met_store::StoreError::from)?;

    Ok(Json(row))
}

#[utoipa::path(
    put,
    path = "/api/v1/workflows/{workflow_name}/sync-schedule",
    request_body = PutWorkflowSyncScheduleRequest,
    tag = "workflows",
)]
#[instrument(skip(state, req))]
async fn put_workflow_sync_schedule(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(workflow_name): Path<String>,
    Json(req): Json<PutWorkflowSyncScheduleRequest>,
) -> ApiResult<Json<WorkflowSyncScheduleRow>> {
    require_admin(&user)?;

    if req.interval_minutes < 0 {
        return Err(ApiError::bad_request("interval_minutes must be >= 0"));
    }

    let enabled = req.enabled && req.interval_minutes > 0;
    let next_sync_at = if enabled {
        Utc::now() + Duration::minutes(i64::from(req.interval_minutes))
    } else {
        // Keep a placeholder; disabled rows won't be picked up by the background task.
        Utc::now() + Duration::hours(24 * 365)
    };

    let row: WorkflowSyncScheduleRow = sqlx::query_as(
        r#"
        INSERT INTO workflow_sync_schedules
            (org_id, workflow_name, enabled, interval_minutes, next_sync_at)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (org_id, workflow_name) DO UPDATE SET
            enabled          = EXCLUDED.enabled,
            interval_minutes = EXCLUDED.interval_minutes,
            next_sync_at     = EXCLUDED.next_sync_at,
            updated_at       = NOW()
        RETURNING id, org_id, workflow_name, enabled, interval_minutes,
                  last_synced_at, next_sync_at, created_at, updated_at
        "#,
    )
    .bind(user.org_id.as_uuid())
    .bind(&workflow_name)
    .bind(enabled)
    .bind(req.interval_minutes)
    .bind(next_sync_at)
    .fetch_one(state.db())
    .await
    .map_err(met_store::StoreError::from)?;

    Ok(Json(row))
}

/// Trigger an immediate sync for a workflow (idempotent; sets `next_sync_at = now()`).
#[utoipa::path(
    post,
    path = "/api/v1/workflows/{workflow_name}/sync-now",
    tag = "workflows",
)]
#[instrument(skip(state))]
async fn trigger_sync_now(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(workflow_name): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    require_admin(&user)?;

    // Only works if a schedule exists; otherwise the background task has no row to process.
    let rows_affected = sqlx::query(
        r#"
        UPDATE workflow_sync_schedules
        SET next_sync_at = NOW(), updated_at = NOW()
        WHERE org_id = $1 AND workflow_name = $2
        "#,
    )
    .bind(user.org_id.as_uuid())
    .bind(&workflow_name)
    .execute(state.db())
    .await
    .map_err(met_store::StoreError::from)?
    .rows_affected();

    if rows_affected == 0 {
        return Err(ApiError::not_found(format!(
            "no sync schedule for workflow '{workflow_name}'; create one with PUT first"
        )));
    }

    Ok(Json(serde_json::json!({
        "message": "sync scheduled for immediate execution"
    })))
}
