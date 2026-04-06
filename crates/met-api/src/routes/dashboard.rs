//! Dashboard overview (org-scoped stats and recent runs).

use axum::{
    Json, Router,
    extract::{Query, State},
    routing::get,
};
use chrono::{Duration, Utc};
use met_core::ids::RunId;
use met_store::repos::{org_dashboard_stats, org_recent_runs};
use serde::{Deserialize, Serialize};
use tracing::instrument;
use utoipa::ToSchema;

use crate::{
    error::{ApiError, ApiResult},
    extractors::Auth,
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/dashboard/stats", get(dashboard_stats))
        .route("/dashboard/recent-runs", get(dashboard_recent_runs))
}

#[derive(Debug, Deserialize)]
pub struct DashboardWindowQuery {
    /// Time window for completed/failed counts and average duration: `1h`, `4h`, `12h`, `1d`, `3d`, `7d`.
    #[serde(default = "default_window")]
    pub window: String,
}

fn default_window() -> String {
    "1d".to_string()
}

fn parse_window(s: &str) -> Result<Duration, ApiError> {
    match s {
        "1h" => Ok(Duration::hours(1)),
        "4h" => Ok(Duration::hours(4)),
        "12h" => Ok(Duration::hours(12)),
        "1d" => Ok(Duration::days(1)),
        "3d" => Ok(Duration::days(3)),
        "7d" => Ok(Duration::days(7)),
        _ => Err(ApiError::bad_request(
            "invalid `window`; use 1h, 4h, 12h, 1d, 3d, or 7d",
        )),
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DashboardStatsResponse {
    pub active_runs: i64,
    pub completed_runs: i64,
    pub failed_runs: i64,
    pub avg_duration_ms: i64,
    pub agents_online: i64,
    pub agents_total: i64,
    pub pipelines_count: i64,
    pub projects_count: i64,
    /// Echo of the requested window key (e.g. `1d`).
    pub window: String,
}

#[utoipa::path(
    get,
    path = "/api/v1/dashboard/stats",
    params(
        ("window" = Option<String>, Query, description = "1h | 4h | 12h | 1d | 3d | 7d (default 1d)"),
    ),
    responses(
        (status = 200, description = "Dashboard counters", body = DashboardStatsResponse),
        (status = 400, description = "Invalid window"),
    ),
    tag = "dashboard",
)]
#[instrument(skip(state))]
async fn dashboard_stats(
    State(state): State<AppState>,
    Auth(user): Auth,
    Query(q): Query<DashboardWindowQuery>,
) -> ApiResult<Json<DashboardStatsResponse>> {
    let window_key = q.window.clone();
    let dur = parse_window(q.window.trim())?;
    let since = Utc::now() - dur;
    let s = org_dashboard_stats(state.db(), user.org_id, since).await?;
    Ok(Json(DashboardStatsResponse {
        active_runs: s.active_runs,
        completed_runs: s.completed_runs,
        failed_runs: s.failed_runs,
        avg_duration_ms: s.avg_duration_ms,
        agents_online: s.agents_online,
        agents_total: s.agents_total,
        pipelines_count: s.pipelines_count,
        projects_count: s.projects_count,
        window: window_key,
    }))
}

#[derive(Debug, Deserialize)]
pub struct RecentRunsQuery {
    #[serde(default = "default_recent_limit")]
    pub limit: i64,
    #[serde(default = "default_window")]
    pub window: String,
}

fn default_recent_limit() -> i64 {
    10
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RecentRunResponse {
    #[schema(value_type = String)]
    pub id: RunId,
    pub pipeline_name: String,
    pub run_number: i64,
    pub status: String,
    pub triggered_by: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<i64>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[utoipa::path(
    get,
    path = "/api/v1/dashboard/recent-runs",
    params(
        ("limit" = Option<i64>, Query, description = "Max rows (default 10)"),
        ("window" = Option<String>, Query, description = "Only runs created in this window (1h … 7d); default 1d"),
    ),
    responses(
        (status = 200, description = "Recent runs", body = Vec<RecentRunResponse>),
    ),
    tag = "dashboard",
)]
#[instrument(skip(state))]
async fn dashboard_recent_runs(
    State(state): State<AppState>,
    Auth(user): Auth,
    Query(q): Query<RecentRunsQuery>,
) -> ApiResult<Json<Vec<RecentRunResponse>>> {
    let dur = parse_window(q.window.trim())?;
    let since = Utc::now() - dur;
    let limit = q.limit.clamp(1, 50);
    let rows = org_recent_runs(state.db(), user.org_id, Some(since), limit).await?;
    let out: Vec<RecentRunResponse> = rows
        .into_iter()
        .map(|r| {
            let duration_ms = match (r.started_at, r.finished_at) {
                (Some(s), Some(f)) => Some((f - s).num_milliseconds()),
                _ => None,
            };
            RecentRunResponse {
                id: RunId::from(r.run_id),
                pipeline_name: r.pipeline_name,
                run_number: r.run_number,
                status: r.status,
                triggered_by: r.triggered_by,
                duration_ms,
                created_at: r.created_at,
            }
        })
        .collect();
    Ok(Json(out))
}
