//! Org-scoped dashboard aggregates for the web UI.

use chrono::{DateTime, Utc};
use met_core::ids::OrganizationId;
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::Result;

/// Summary counters and averages for an organization over a time window.
#[derive(Debug, Clone, Default)]
pub struct DashboardStats {
    pub active_runs: i64,
    pub completed_runs: i64,
    pub failed_runs: i64,
    pub cancelled_runs: i64,
    /// All runs (any status) with `created_at` in the stats window.
    pub total_runs: i64,
    pub avg_duration_ms: i64,
    /// Agents with heartbeats in an operational state (`online` or `busy`), excluding deregistered.
    pub agents_online: i64,
    /// Registered agents not deregistered (`deregistered_at IS NULL`), matching the agents list UI.
    pub agents_total: i64,
    pub pipelines_count: i64,
    pub projects_count: i64,
}

/// One row for "recent runs" widgets.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DashboardRecentRunRow {
    pub run_id: Uuid,
    pub pipeline_name: String,
    pub run_number: i64,
    pub status: String,
    pub triggered_by: String,
    pub webhook_remote_addr: Option<String>,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
}

/// Load dashboard stats for every project belonging to `org_id`.
pub async fn org_dashboard_stats(
    pool: &PgPool,
    org_id: OrganizationId,
    since: DateTime<Utc>,
) -> Result<DashboardStats> {
    let org_u = org_id.as_uuid();
    let row = sqlx::query_as::<_, (
        i64,
        i64,
        i64,
        i64,
        i64,
        Option<f64>,
        i64,
        i64,
        i64,
        i64,
    )>(
        r#"
        WITH org_projects AS (
            SELECT id FROM projects
            WHERE org_id = $1 AND deleted_at IS NULL
        ),
        org_pipelines AS (
            SELECT p.id FROM pipelines p
            INNER JOIN org_projects op ON op.id = p.project_id
        )
        SELECT
            (
                SELECT COUNT(*)::bigint
                FROM runs r
                INNER JOIN org_pipelines p ON p.id = r.pipeline_id
                WHERE r.status IN ('pending', 'queued', 'running')
            ) AS active_runs,
            (
                SELECT COUNT(*)::bigint
                FROM runs r
                INNER JOIN org_pipelines p ON p.id = r.pipeline_id
                WHERE r.status = 'succeeded'
                  AND COALESCE(r.finished_at, r.created_at) >= $2
            ) AS completed_runs,
            (
                SELECT COUNT(*)::bigint
                FROM runs r
                INNER JOIN org_pipelines p ON p.id = r.pipeline_id
                WHERE r.status = 'failed'
                  AND COALESCE(r.finished_at, r.created_at) >= $2
            ) AS failed_runs,
            (
                SELECT COUNT(*)::bigint
                FROM runs r
                INNER JOIN org_pipelines p ON p.id = r.pipeline_id
                WHERE r.status = 'cancelled'
                  AND COALESCE(r.finished_at, r.created_at) >= $2
            ) AS cancelled_runs,
            (
                SELECT COUNT(*)::bigint
                FROM runs r
                INNER JOIN org_pipelines p ON p.id = r.pipeline_id
                WHERE r.created_at >= $2
            ) AS total_runs,
            (
                SELECT (
                    AVG(
                        EXTRACT(EPOCH FROM (r.finished_at - r.started_at)) * 1000.0
                    )
                )::double precision
                FROM runs r
                INNER JOIN org_pipelines p ON p.id = r.pipeline_id
                WHERE r.status = 'succeeded'
                  AND r.started_at IS NOT NULL
                  AND r.finished_at IS NOT NULL
                  AND COALESCE(r.finished_at, r.created_at) >= $2
            ) AS avg_duration_ms,
            (
                SELECT COUNT(*)::bigint FROM agents
                WHERE org_id = $1
                  AND deregistered_at IS NULL
                  AND status IN ('online', 'busy')
            ) AS agents_online,
            (
                SELECT COUNT(*)::bigint FROM agents
                WHERE org_id = $1
                  AND deregistered_at IS NULL
            ) AS agents_total,
            (
                SELECT COUNT(*)::bigint FROM org_pipelines
            ) AS pipelines_count,
            (
                SELECT COUNT(*)::bigint FROM org_projects
            ) AS projects_count
        "#,
    )
    .bind(org_u)
    .bind(since)
    .fetch_one(pool)
    .await?;

    Ok(DashboardStats {
        active_runs: row.0,
        completed_runs: row.1,
        failed_runs: row.2,
        cancelled_runs: row.3,
        total_runs: row.4,
        avg_duration_ms: row.5.map(|v| v.round() as i64).unwrap_or(0),
        agents_online: row.6,
        agents_total: row.7,
        pipelines_count: row.8,
        projects_count: row.9,
    })
}

/// Recent pipeline runs for the org (newest first), optionally limited to those starting at or after `since`.
pub async fn org_recent_runs(
    pool: &PgPool,
    org_id: OrganizationId,
    since: Option<DateTime<Utc>>,
    limit: i64,
) -> Result<Vec<DashboardRecentRunRow>> {
    let org_u = org_id.as_uuid();
    let rows = if let Some(ts) = since {
        sqlx::query_as::<_, DashboardRecentRunRow>(
            r#"
            SELECT
                r.id AS run_id,
                pl.name AS pipeline_name,
                r.run_number,
                r.status::text AS status,
                r.triggered_by,
                r.webhook_remote_addr,
                r.created_at,
                r.started_at,
                r.finished_at
            FROM runs r
            INNER JOIN pipelines pl ON pl.id = r.pipeline_id
            INNER JOIN projects pr ON pr.id = pl.project_id
            WHERE pr.org_id = $1
              AND pr.deleted_at IS NULL
              AND r.created_at >= $2
            ORDER BY r.created_at DESC
            LIMIT $3
            "#,
        )
        .bind(org_u)
        .bind(ts)
        .bind(limit)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as::<_, DashboardRecentRunRow>(
            r#"
            SELECT
                r.id AS run_id,
                pl.name AS pipeline_name,
                r.run_number,
                r.status::text AS status,
                r.triggered_by,
                r.webhook_remote_addr,
                r.created_at,
                r.started_at,
                r.finished_at
            FROM runs r
            INNER JOIN pipelines pl ON pl.id = r.pipeline_id
            INNER JOIN projects pr ON pr.id = pl.project_id
            WHERE pr.org_id = $1
              AND pr.deleted_at IS NULL
            ORDER BY r.created_at DESC
            LIMIT $2
            "#,
        )
        .bind(org_u)
        .bind(limit)
        .fetch_all(pool)
        .await?
    };

    Ok(rows)
}
