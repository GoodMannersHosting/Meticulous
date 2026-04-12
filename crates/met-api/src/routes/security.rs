//! Read-only security search (“blast radius”) across identifiers and run footprint telemetry
//! (binary path / SHA256, outbound egress destinations).

use axum::{
    Json, Router,
    extract::{Query, State},
    routing::get,
};
use met_core::ids::{AgentId, RunId};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::net::IpAddr;
use uuid::Uuid;

use crate::{
    error::{ApiError, ApiResult},
    extractors::Auth,
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new().route("/security/blast-radius", get(blast_radius_search))
}

#[derive(Debug, Deserialize)]
pub struct BlastRadiusQuery {
    pub q: String,
}

#[derive(Debug, Serialize)]
pub struct BlastRadiusHit {
    pub kind: &'static str,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct BlastRadiusResponse {
    pub query: String,
    pub hits: Vec<BlastRadiusHit>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum OrgSearchMode {
    FullOrg,
    ProjectAllowList(Vec<Uuid>),
    PipelineAllowList(Vec<Uuid>),
    OpenOrg,
}

fn blast_radius_mode(user: &crate::extractors::CurrentUser) -> OrgSearchMode {
    if user.has_permission("*")
        || user.has_permission("security:blast-radius:all")
        || user.has_permission("security:blast-radius:org")
    {
        return OrgSearchMode::FullOrg;
    }
    if user.is_api_token {
        if let Some(ref pl) = user.pipeline_ids {
            if !pl.is_empty() {
                return OrgSearchMode::PipelineAllowList(pl.iter().map(|p| p.as_uuid()).collect());
            }
        }
    }
    if let Some(ref pids) = user.project_ids {
        return OrgSearchMode::ProjectAllowList(pids.iter().map(|p| p.as_uuid()).collect());
    }
    OrgSearchMode::OpenOrg
}

/// Substring match on commit digest, run id, pipeline id/name/slug, or project name/slug.
/// Footprint search also matches observed binary paths, SHA256 digests, and outbound egress (`dst_ip` in CIDR).
const RUN_BLAST_MATCH: &str = r#"
  (
    (r.commit_sha IS NOT NULL AND r.commit_sha ILIKE $2 ESCAPE '\')
    OR CAST(r.id AS TEXT) ILIKE $2 ESCAPE '\'
    OR CAST(p.id AS TEXT) ILIKE $2 ESCAPE '\'
    OR p.name ILIKE $2 ESCAPE '\'
    OR COALESCE(p.slug, '') ILIKE $2 ESCAPE '\'
    OR pr.name ILIKE $2 ESCAPE '\'
    OR COALESCE(pr.slug, '') ILIKE $2 ESCAPE '\'
  )
"#;

/// Pipelines matching by id, name, or slug (even when they have no runs yet).
const PIPELINE_BLAST_MATCH: &str = r#"
  (
    CAST(p.id AS TEXT) ILIKE $2 ESCAPE '\'
    OR p.name ILIKE $2 ESCAPE '\'
    OR COALESCE(p.slug, '') ILIKE $2 ESCAPE '\'
  )
"#;

/// Projects matching by id, name, or slug.
const PROJECT_BLAST_MATCH: &str = r#"
  (
    CAST(pr.id AS TEXT) ILIKE $2 ESCAPE '\'
    OR pr.name ILIKE $2 ESCAPE '\'
    OR COALESCE(pr.slug, '') ILIKE $2 ESCAPE '\'
  )
"#;

fn push_hit(
    hits: &mut Vec<BlastRadiusHit>,
    seen: &mut HashSet<(String, String)>,
    kind: &'static str,
    id: String,
    project_id: Option<String>,
    detail: Option<String>,
) {
    if seen.insert((kind.to_string(), id.clone())) {
        hits.push(BlastRadiusHit {
            kind,
            id,
            project_id,
            detail,
        });
    }
}

/// Normalize user input into a PostgreSQL `cidr` literal for egress search (`dst_ip <<= cidr`).
/// Unix paths like `/usr/bin/foo` are rejected (invalid IP on the left of `/`).
fn foot_net_search_cidr(needle: &str) -> Option<String> {
    let s = needle.trim();
    if s.is_empty() {
        return None;
    }
    if let Ok(ip) = s.parse::<IpAddr>() {
        return Some(match ip {
            IpAddr::V4(a) => format!("{}/32", a),
            IpAddr::V6(a) => format!("{}/128", a),
        });
    }
    let (host, pref_s) = s.rsplit_once('/')?;
    let prefix: u8 = pref_s.parse().ok()?;
    let ip: IpAddr = host.parse().ok()?;
    let max_pfx = match ip {
        IpAddr::V4(_) => 32_u8,
        IpAddr::V6(_) => 128_u8,
    };
    if prefix > max_pfx {
        return None;
    }
    Some(s.to_string())
}

/// Run rows from binary execution telemetry (path or sha256 substring match).
async fn blast_radius_runs_from_binary_footprint(
    db: &sqlx::PgPool,
    org_uuid: Uuid,
    mode: &OrgSearchMode,
    like: &str,
) -> Result<Vec<(Uuid, Option<String>, Option<String>, String)>, sqlx::Error> {
    type Row = (Uuid, Option<String>, Option<String>, String);
    match mode {
        OrgSearchMode::FullOrg | OrgSearchMode::OpenOrg => {
            sqlx::query_as::<_, Row>(
                r#"
                SELECT r.id,
                       MAX(r.commit_sha) AS commit_sha,
                       MAX(p.name) AS pipe_name,
                       MIN(rbe.binary_path) AS sample_path
                FROM run_binary_executions rbe
                INNER JOIN runs r ON r.id = rbe.run_id
                INNER JOIN pipelines p ON p.id = r.pipeline_id
                INNER JOIN projects pr ON pr.id = p.project_id
                WHERE pr.org_id = $1
                  AND pr.deleted_at IS NULL
                  AND (
                    rbe.binary_path ILIKE $2 ESCAPE '\'
                    OR rbe.binary_sha256 ILIKE $2 ESCAPE '\'
                  )
                GROUP BY r.id
                ORDER BY MAX(r.created_at) DESC
                LIMIT 25
                "#,
            )
            .bind(org_uuid)
            .bind(like)
            .fetch_all(db)
            .await
        }
        OrgSearchMode::ProjectAllowList(pids) => {
            if pids.is_empty() {
                return Ok(vec![]);
            }
            sqlx::query_as::<_, Row>(
                r#"
                SELECT r.id,
                       MAX(r.commit_sha) AS commit_sha,
                       MAX(p.name) AS pipe_name,
                       MIN(rbe.binary_path) AS sample_path
                FROM run_binary_executions rbe
                INNER JOIN runs r ON r.id = rbe.run_id
                INNER JOIN pipelines p ON p.id = r.pipeline_id
                INNER JOIN projects pr ON pr.id = p.project_id
                WHERE p.project_id = ANY($1)
                  AND pr.deleted_at IS NULL
                  AND (
                    rbe.binary_path ILIKE $2 ESCAPE '\'
                    OR rbe.binary_sha256 ILIKE $2 ESCAPE '\'
                  )
                GROUP BY r.id
                ORDER BY MAX(r.created_at) DESC
                LIMIT 25
                "#,
            )
            .bind(pids)
            .bind(like)
            .fetch_all(db)
            .await
        }
        OrgSearchMode::PipelineAllowList(pipes) => {
            if pipes.is_empty() {
                return Ok(vec![]);
            }
            sqlx::query_as::<_, Row>(
                r#"
                SELECT r.id,
                       MAX(r.commit_sha) AS commit_sha,
                       MAX(p.name) AS pipe_name,
                       MIN(rbe.binary_path) AS sample_path
                FROM run_binary_executions rbe
                INNER JOIN runs r ON r.id = rbe.run_id
                INNER JOIN pipelines p ON p.id = r.pipeline_id
                INNER JOIN projects pr ON pr.id = p.project_id
                WHERE r.pipeline_id = ANY($1)
                  AND pr.org_id = $3
                  AND pr.deleted_at IS NULL
                  AND (
                    rbe.binary_path ILIKE $2 ESCAPE '\'
                    OR rbe.binary_sha256 ILIKE $2 ESCAPE '\'
                  )
                GROUP BY r.id
                ORDER BY MAX(r.created_at) DESC
                LIMIT 25
                "#,
            )
            .bind(pipes)
            .bind(like)
            .bind(org_uuid)
            .fetch_all(db)
            .await
        }
    }
}

/// Run rows from outbound network telemetry (`dst_ip` inside searched CIDR).
async fn blast_radius_runs_from_egress_footprint(
    db: &sqlx::PgPool,
    org_uuid: Uuid,
    mode: &OrgSearchMode,
    cidr: &str,
) -> Result<Vec<(Uuid, Option<String>, Option<String>, String)>, sqlx::Error> {
    type Row = (Uuid, Option<String>, Option<String>, String);
    match mode {
        OrgSearchMode::FullOrg | OrgSearchMode::OpenOrg => {
            sqlx::query_as::<_, Row>(
                r#"
                SELECT r.id,
                       MAX(r.commit_sha) AS commit_sha,
                       MAX(p.name) AS pipe_name,
                       MIN(host(n.dst_ip) || ':' || n.dst_port::text) AS sample_dst
                FROM run_network_connections n
                INNER JOIN runs r ON r.id = n.run_id
                INNER JOIN pipelines p ON p.id = r.pipeline_id
                INNER JOIN projects pr ON pr.id = p.project_id
                WHERE pr.org_id = $1
                  AND pr.deleted_at IS NULL
                  AND n.direction = 'outbound'
                  AND n.dst_ip <<= $2::cidr
                GROUP BY r.id
                ORDER BY MAX(r.created_at) DESC
                LIMIT 25
                "#,
            )
            .bind(org_uuid)
            .bind(cidr)
            .fetch_all(db)
            .await
        }
        OrgSearchMode::ProjectAllowList(pids) => {
            if pids.is_empty() {
                return Ok(vec![]);
            }
            sqlx::query_as::<_, Row>(
                r#"
                SELECT r.id,
                       MAX(r.commit_sha) AS commit_sha,
                       MAX(p.name) AS pipe_name,
                       MIN(host(n.dst_ip) || ':' || n.dst_port::text) AS sample_dst
                FROM run_network_connections n
                INNER JOIN runs r ON r.id = n.run_id
                INNER JOIN pipelines p ON p.id = r.pipeline_id
                INNER JOIN projects pr ON pr.id = p.project_id
                WHERE p.project_id = ANY($1)
                  AND pr.deleted_at IS NULL
                  AND n.direction = 'outbound'
                  AND n.dst_ip <<= $2::cidr
                GROUP BY r.id
                ORDER BY MAX(r.created_at) DESC
                LIMIT 25
                "#,
            )
            .bind(pids)
            .bind(cidr)
            .fetch_all(db)
            .await
        }
        OrgSearchMode::PipelineAllowList(pipes) => {
            if pipes.is_empty() {
                return Ok(vec![]);
            }
            sqlx::query_as::<_, Row>(
                r#"
                SELECT r.id,
                       MAX(r.commit_sha) AS commit_sha,
                       MAX(p.name) AS pipe_name,
                       MIN(host(n.dst_ip) || ':' || n.dst_port::text) AS sample_dst
                FROM run_network_connections n
                INNER JOIN runs r ON r.id = n.run_id
                INNER JOIN pipelines p ON p.id = r.pipeline_id
                INNER JOIN projects pr ON pr.id = p.project_id
                WHERE r.pipeline_id = ANY($1)
                  AND pr.org_id = $3
                  AND pr.deleted_at IS NULL
                  AND n.direction = 'outbound'
                  AND n.dst_ip <<= $2::cidr
                GROUP BY r.id
                ORDER BY MAX(r.created_at) DESC
                LIMIT 25
                "#,
            )
            .bind(pipes)
            .bind(cidr)
            .bind(org_uuid)
            .fetch_all(db)
            .await
        }
    }
}

/// GET `/api/v1/security/blast-radius?q=...`
async fn blast_radius_search(
    State(state): State<AppState>,
    Auth(user): Auth,
    Query(q): Query<BlastRadiusQuery>,
) -> ApiResult<Json<BlastRadiusResponse>> {
    let needle = q.q.trim();
    let net_cidr = foot_net_search_cidr(needle);
    if needle.len() < 3 && net_cidr.is_none() {
        return Err(ApiError::bad_request(
            "query must be at least 3 characters (or a valid IP / CIDR for egress search)",
        ));
    }
    let like = format!("%{}%", needle.replace('%', "\\%").replace('_', "\\_"));
    let org_uuid = user.org_id.as_uuid();
    let mode = blast_radius_mode(&user);

    let mut hits: Vec<BlastRadiusHit> = Vec::new();
    let mut seen: HashSet<(String, String)> = HashSet::new();

    // Runs: commit digest, run id, pipeline / project names and ids.
    let run_rows: Vec<(Uuid, Uuid, Option<String>, Option<String>)> = match mode {
        OrgSearchMode::FullOrg | OrgSearchMode::OpenOrg => {
            let q = format!(
                r#"
                SELECT r.id, r.pipeline_id, r.commit_sha, p.name
                FROM runs r
                INNER JOIN pipelines p ON p.id = r.pipeline_id
                INNER JOIN projects pr ON pr.id = p.project_id
                WHERE pr.org_id = $1
                  AND pr.deleted_at IS NULL
                  AND {RUN_BLAST_MATCH}
                ORDER BY r.created_at DESC
                LIMIT 25
                "#
            );
            sqlx::query_as(&q)
                .bind(org_uuid)
                .bind(&like)
                .fetch_all(state.db())
                .await
                .map_err(|e| ApiError::internal(e.to_string()))?
        }
        OrgSearchMode::ProjectAllowList(ref pids) => {
            if pids.is_empty() {
                vec![]
            } else {
                let q = format!(
                    r#"
                    SELECT r.id, r.pipeline_id, r.commit_sha, p.name
                    FROM runs r
                    INNER JOIN pipelines p ON p.id = r.pipeline_id
                    INNER JOIN projects pr ON pr.id = p.project_id
                    WHERE p.project_id = ANY($1)
                      AND pr.deleted_at IS NULL
                      AND {RUN_BLAST_MATCH}
                    ORDER BY r.created_at DESC
                    LIMIT 25
                    "#
                );
                sqlx::query_as(&q)
                    .bind(pids)
                    .bind(&like)
                    .fetch_all(state.db())
                    .await
                    .map_err(|e| ApiError::internal(e.to_string()))?
            }
        }
        OrgSearchMode::PipelineAllowList(ref pipes) => {
            if pipes.is_empty() {
                vec![]
            } else {
                let q = format!(
                    r#"
                    SELECT r.id, r.pipeline_id, r.commit_sha, p.name
                    FROM runs r
                    INNER JOIN pipelines p ON p.id = r.pipeline_id
                    INNER JOIN projects pr ON pr.id = p.project_id
                    WHERE r.pipeline_id = ANY($1)
                      AND pr.org_id = $3
                      AND pr.deleted_at IS NULL
                      AND {RUN_BLAST_MATCH}
                    ORDER BY r.created_at DESC
                    LIMIT 25
                    "#
                );
                sqlx::query_as(&q)
                    .bind(pipes)
                    .bind(&like)
                    .bind(org_uuid)
                    .fetch_all(state.db())
                    .await
                    .map_err(|e| ApiError::internal(e.to_string()))?
            }
        }
    };

    for (rid, _pipe_id, commit, pipe_name) in run_rows {
        let detail = commit.filter(|s| !s.trim().is_empty()).or(pipe_name);
        push_hit(
            &mut hits,
            &mut seen,
            "run",
            RunId::from_uuid(rid).to_string(),
            None,
            detail,
        );
    }

    // Pipelines (including those with no runs yet).
    let pipeline_rows: Vec<(Uuid, Uuid, String, String)> = match &mode {
        OrgSearchMode::FullOrg | OrgSearchMode::OpenOrg => {
            let q = format!(
                r#"
                SELECT p.id, p.project_id, p.name, pr.name
                FROM pipelines p
                INNER JOIN projects pr ON pr.id = p.project_id
                WHERE pr.org_id = $1
                  AND pr.deleted_at IS NULL
                  AND {PIPELINE_BLAST_MATCH}
                ORDER BY p.updated_at DESC NULLS LAST
                LIMIT 25
                "#
            );
            sqlx::query_as(&q)
                .bind(org_uuid)
                .bind(&like)
                .fetch_all(state.db())
                .await
                .map_err(|e| ApiError::internal(e.to_string()))?
        }
        OrgSearchMode::ProjectAllowList(pids) => {
            if pids.is_empty() {
                vec![]
            } else {
                let q = format!(
                    r#"
                SELECT p.id, p.project_id, p.name, pr.name
                FROM pipelines p
                INNER JOIN projects pr ON pr.id = p.project_id
                WHERE p.project_id = ANY($1)
                  AND pr.deleted_at IS NULL
                  AND {PIPELINE_BLAST_MATCH}
                ORDER BY p.updated_at DESC NULLS LAST
                LIMIT 25
                "#
                );
                sqlx::query_as(&q)
                    .bind(pids)
                    .bind(&like)
                    .fetch_all(state.db())
                    .await
                    .map_err(|e| ApiError::internal(e.to_string()))?
            }
        }
        OrgSearchMode::PipelineAllowList(pipes) => {
            if pipes.is_empty() {
                vec![]
            } else {
                let q = format!(
                    r#"
                SELECT p.id, p.project_id, p.name, pr.name
                FROM pipelines p
                INNER JOIN projects pr ON pr.id = p.project_id
                WHERE p.id = ANY($1)
                  AND pr.org_id = $3
                  AND pr.deleted_at IS NULL
                  AND {PIPELINE_BLAST_MATCH}
                ORDER BY p.updated_at DESC NULLS LAST
                LIMIT 25
                "#
                );
                sqlx::query_as(&q)
                    .bind(pipes)
                    .bind(&like)
                    .bind(org_uuid)
                    .fetch_all(state.db())
                    .await
                    .map_err(|e| ApiError::internal(e.to_string()))?
            }
        }
    };
    for (pipe_id, project_id, pipe_name, project_name) in pipeline_rows {
        push_hit(
            &mut hits,
            &mut seen,
            "pipeline",
            pipe_id.to_string(),
            Some(project_id.to_string()),
            Some(format!("{pipe_name} · {project_name}")),
        );
    }

    // Projects (org-scoped).
    let project_rows: Vec<(Uuid, String)> = match &mode {
        OrgSearchMode::FullOrg | OrgSearchMode::OpenOrg => {
            let q = format!(
                r#"
                SELECT pr.id, pr.name
                FROM projects pr
                WHERE pr.org_id = $1
                  AND pr.deleted_at IS NULL
                  AND {PROJECT_BLAST_MATCH}
                ORDER BY pr.updated_at DESC NULLS LAST
                LIMIT 25
                "#
            );
            sqlx::query_as(&q)
                .bind(org_uuid)
                .bind(&like)
                .fetch_all(state.db())
                .await
                .map_err(|e| ApiError::internal(e.to_string()))?
        }
        OrgSearchMode::ProjectAllowList(pids) => {
            if pids.is_empty() {
                vec![]
            } else {
                let q = format!(
                    r#"
                SELECT pr.id, pr.name
                FROM projects pr
                WHERE pr.id = ANY($1)
                  AND pr.deleted_at IS NULL
                  AND {PROJECT_BLAST_MATCH}
                ORDER BY pr.updated_at DESC NULLS LAST
                LIMIT 25
                "#
                );
                sqlx::query_as(&q)
                    .bind(pids)
                    .bind(&like)
                    .fetch_all(state.db())
                    .await
                    .map_err(|e| ApiError::internal(e.to_string()))?
            }
        }
        OrgSearchMode::PipelineAllowList(pipes) => {
            if pipes.is_empty() {
                vec![]
            } else {
                let q = format!(
                    r#"
                SELECT DISTINCT pr.id, pr.name
                FROM projects pr
                INNER JOIN pipelines p ON p.project_id = pr.id
                WHERE p.id = ANY($1)
                  AND pr.org_id = $3
                  AND pr.deleted_at IS NULL
                  AND {PROJECT_BLAST_MATCH}
                ORDER BY pr.name
                LIMIT 25
                "#
                );
                sqlx::query_as(&q)
                    .bind(pipes)
                    .bind(&like)
                    .bind(org_uuid)
                    .fetch_all(state.db())
                    .await
                    .map_err(|e| ApiError::internal(e.to_string()))?
            }
        }
    };
    for (proj_id, proj_name) in project_rows {
        push_hit(
            &mut hits,
            &mut seen,
            "project",
            proj_id.to_string(),
            None,
            Some(proj_name),
        );
    }

    // Runs linked by execution footprint (binary path or SHA256 substring).
    let binary_rows = blast_radius_runs_from_binary_footprint(state.db(), org_uuid, &mode, &like)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    for (rid, commit_opt, pipe_name_opt, sample_path) in binary_rows {
        let foot = format!("footprint (binary): {sample_path}");
        let detail = match (commit_opt.filter(|s| !s.trim().is_empty()), pipe_name_opt) {
            (Some(c), Some(p)) => Some(format!("{c} · {p} · {foot}")),
            (Some(c), None) => Some(format!("{c} · {foot}")),
            (None, Some(p)) => Some(format!("{p} · {foot}")),
            (None, None) => Some(foot),
        };
        push_hit(
            &mut hits,
            &mut seen,
            "run",
            RunId::from_uuid(rid).to_string(),
            None,
            detail,
        );
    }

    if let Some(ref cidr_s) = net_cidr {
        let egress_rows =
            blast_radius_runs_from_egress_footprint(state.db(), org_uuid, &mode, cidr_s)
                .await
                .map_err(|e| ApiError::internal(e.to_string()))?;
        for (rid, commit_opt, pipe_name_opt, sample_dst) in egress_rows {
            let foot = format!("footprint (egress): {sample_dst}");
            let detail = match (commit_opt.filter(|s| !s.trim().is_empty()), pipe_name_opt) {
                (Some(c), Some(p)) => Some(format!("{c} · {p} · {foot}")),
                (Some(c), None) => Some(format!("{c} · {foot}")),
                (None, Some(p)) => Some(format!("{p} · {foot}")),
                (None, None) => Some(foot),
            };
            push_hit(
                &mut hits,
                &mut seen,
                "run",
                RunId::from_uuid(rid).to_string(),
                None,
                detail,
            );
        }
    }

    // Agents by id/name (bounded to org).
    let agent_rows: Vec<(Uuid, String)> = sqlx::query_as(
        r#"
        SELECT id, name
        FROM agents
        WHERE org_id = $1
          AND (
            CAST(id AS TEXT) ILIKE $2
            OR name ILIKE $2
            OR COALESCE(ip_address, '') ILIKE $2
          )
        ORDER BY last_heartbeat_at DESC NULLS LAST
        LIMIT 25
        "#,
    )
    .bind(org_uuid)
    .bind(&like)
    .fetch_all(state.db())
    .await
    .map_err(|e| ApiError::internal(e.to_string()))?;

    for (aid, name) in agent_rows {
        push_hit(
            &mut hits,
            &mut seen,
            "agent",
            AgentId::from_uuid(aid).to_string(),
            None,
            Some(name),
        );
    }

    Ok(Json(BlastRadiusResponse {
        query: needle.to_string(),
        hits,
    }))
}
