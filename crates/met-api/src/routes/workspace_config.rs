//! Cross-project views of environment variables and stored secrets for the workspace hub UI.

use axum::{
    Json, Router,
    extract::{Query, State},
    routing::get,
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::{DateTime, Utc};
use met_core::ids::{PipelineId, ProjectId, VariableId};
use met_store::repos::{BuiltinSecretMetaRow, PipelineRepo, ProjectRepo};
use serde::{Deserialize, Serialize};
use tracing::instrument;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::routes::stored_secrets::StoredSecretResponse;
use crate::{
    error::{ApiError, ApiResult},
    extractors::{Auth, PaginatedResponse, Pagination},
    state::AppState,
};

/// Reusable query for hub list endpoints.
#[derive(Debug, Deserialize)]
pub struct WorkspaceListQuery {
    /// Case-insensitive match on variable **name** or secret **path** / **description**.
    #[serde(default)]
    pub q: Option<String>,
    #[serde(default)]
    pub project_id: Option<ProjectId>,
    #[serde(default)]
    pub pipeline_id: Option<PipelineId>,
    #[serde(default)]
    pub scope_level: WorkspaceScopeLevel,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceScopeLevel {
    #[default]
    All,
    /// Org-wide stored secrets only (`project_id` null). Yields no rows for variables.
    Organization,
    /// Project-scoped rows only (no pipeline): variables with `pipeline_id` null; secrets with
    /// `project_id` set and `pipeline_id` null.
    Project,
    /// Pipeline-scoped rows only.
    Pipeline,
}

impl WorkspaceScopeLevel {
    const fn as_sql_str(&self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Organization => "organization",
            Self::Project => "project",
            Self::Pipeline => "pipeline",
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct WorkspaceVariableListItem {
    #[schema(value_type = String)]
    pub id: VariableId,
    #[schema(value_type = String)]
    pub project_id: ProjectId,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<String>)]
    pub pipeline_id: Option<PipelineId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment_name: Option<String>,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    pub scope: String,
    pub is_sensitive: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub project_name: String,
    pub project_slug: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pipeline_name: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct VariableHubRow {
    id: Uuid,
    project_id: Uuid,
    pipeline_id: Option<Uuid>,
    environment_id: Option<Uuid>,
    name: String,
    value: String,
    scope: String,
    is_sensitive: bool,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    project_name: String,
    project_slug: String,
    pipeline_name: Option<String>,
    environment_name: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct SecretHubRow {
    id: Uuid,
    org_id: Uuid,
    project_id: Option<Uuid>,
    pipeline_id: Option<Uuid>,
    environment_id: Option<Uuid>,
    path: String,
    kind: String,
    version: i32,
    metadata: serde_json::Value,
    description: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    propagate_to_projects: bool,
    project_name: Option<String>,
    project_slug: Option<String>,
    pipeline_name: Option<String>,
    environment_name: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct WorkspaceStoredSecretListItem {
    #[serde(flatten)]
    pub secret: StoredSecretResponse,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_slug: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pipeline_name: Option<String>,
    /// Display name for [`StoredSecretResponse::environment_id`] when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment_name: Option<String>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/workspace/variables", get(list_workspace_variables))
        .route(
            "/workspace/stored-secrets",
            get(list_workspace_stored_secrets),
        )
}

fn ilike_pattern(raw: &str) -> String {
    let escaped = raw
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("%{escaped}%")
}

fn encode_cursor(t: DateTime<Utc>, id: Uuid) -> String {
    let s = format!(
        "{}|{}",
        t.to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true),
        id
    );
    STANDARD.encode(s.as_bytes())
}

fn decode_cursor(cursor: &str) -> Result<(DateTime<Utc>, Uuid), ApiError> {
    let bytes = STANDARD
        .decode(cursor.trim())
        .map_err(|_| ApiError::bad_request("invalid pagination cursor"))?;
    let text =
        String::from_utf8(bytes).map_err(|_| ApiError::bad_request("invalid pagination cursor"))?;
    let (ts, id_str) = text
        .split_once('|')
        .ok_or_else(|| ApiError::bad_request("invalid pagination cursor"))?;
    let t = DateTime::parse_from_rfc3339(ts)
        .map_err(|_| ApiError::bad_request("invalid pagination cursor"))?
        .with_timezone(&Utc);
    let id =
        Uuid::parse_str(id_str).map_err(|_| ApiError::bad_request("invalid pagination cursor"))?;
    Ok((t, id))
}

struct AccessPlan {
    restricted: bool,
    project_uuids: Vec<Uuid>,
}

fn project_access_plan(user: &crate::extractors::CurrentUser) -> AccessPlan {
    match &user.project_ids {
        None => AccessPlan {
            restricted: false,
            project_uuids: Vec::new(),
        },
        Some(ids) => AccessPlan {
            restricted: true,
            project_uuids: ids.iter().map(|p| p.as_uuid()).collect(),
        },
    }
}

async fn resolve_project_pipeline_filters(
    state: &AppState,
    user: &crate::extractors::CurrentUser,
    mut project_id: Option<ProjectId>,
    pipeline_id: Option<PipelineId>,
) -> Result<(Option<ProjectId>, Option<PipelineId>), ApiError> {
    let mut pipeline_id = pipeline_id;
    if let Some(pid) = pipeline_id {
        let pl = PipelineRepo::new(state.db())
            .get(pid)
            .await
            .map_err(ApiError::from)?;
        let proj = ProjectRepo::new(state.db())
            .get(pl.project_id)
            .await
            .map_err(ApiError::from)?;
        if proj.org_id != user.org_id {
            return Err(ApiError::forbidden("pipeline not in your organization"));
        }
        match project_id {
            Some(p) if p != pl.project_id => {
                return Err(ApiError::bad_request(
                    "pipeline_id does not belong to the given project_id",
                ));
            }
            _ => project_id = Some(pl.project_id),
        }
    }

    if let Some(p) = project_id {
        if !user.can_access_project(p) {
            return Err(ApiError::forbidden("no access to this project"));
        }
    }

    if let (Some(pipe), Some(proj)) = (pipeline_id, project_id) {
        crate::project_access::ensure_api_token_pipeline_scope(user, pipe, proj)?;
    }

    Ok((project_id, pipeline_id))
}

#[utoipa::path(
    get,
    path = "/api/v1/workspace/variables",
    params(
        ("q" = Option<String>, Query),
        ("project_id" = Option<String>, Query),
        ("pipeline_id" = Option<String>, Query),
        ("scope_level" = Option<String>, Query, description = "all | organization | project | pipeline"),
        ("cursor" = Option<String>, Query),
        ("per_page" = Option<u32>, Query),
    ),
    responses(
        (status = 200, description = "Paginated variables across org", body = PaginatedResponse<WorkspaceVariableListItem>),
        (status = 403, description = "Forbidden"),
    ),
    tag = "variables",
)]
#[instrument(skip(state))]
async fn list_workspace_variables(
    State(state): State<AppState>,
    Auth(user): Auth,
    pagination: Pagination,
    Query(q): Query<WorkspaceListQuery>,
) -> ApiResult<Json<PaginatedResponse<WorkspaceVariableListItem>>> {
    if matches!(q.scope_level, WorkspaceScopeLevel::Organization) {
        return Ok(Json(PaginatedResponse::empty()));
    }

    let (fp, fpl) =
        resolve_project_pipeline_filters(&state, &user, q.project_id, q.pipeline_id).await?;

    let plan = project_access_plan(&user);
    let pattern =
        q.q.as_ref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(ilike_pattern);

    let (cursor_t, cursor_id) = match &pagination.cursor {
        None => (None, None),
        Some(c) => {
            let (t, id) = decode_cursor(c)?;
            (Some(t), Some(id))
        }
    };

    let scope_str = q.scope_level.as_sql_str();

    let limit_bind = pagination.sql_limit();

    let rows = sqlx::query_as::<_, VariableHubRow>(
        r#"
        SELECT
          v.id, v.project_id, v.pipeline_id, v.environment_id, v.name, v.value, v.scope::text, v.is_sensitive,
          v.created_at, v.updated_at,
          p.name AS project_name, p.slug AS project_slug,
          pl.name AS pipeline_name,
          e.display_name AS environment_name
        FROM variables v
        INNER JOIN projects p ON p.id = v.project_id
        LEFT JOIN pipelines pl ON pl.id = v.pipeline_id
        LEFT JOIN environments e ON e.id = v.environment_id
        WHERE v.org_id = $1
          AND (NOT $2::bool OR v.project_id = ANY($3))
          AND ($4::uuid IS NULL OR v.project_id = $4)
          AND ($5::uuid IS NULL OR v.pipeline_id = $5)
          AND ($6::text IS NULL OR v.name ILIKE $6 ESCAPE '\')
          AND (
            $9 = 'all'
            OR ($9 = 'organization' AND false)
            OR ($9 = 'project' AND v.pipeline_id IS NULL)
            OR ($9 = 'pipeline' AND v.pipeline_id IS NOT NULL)
          )
          AND ($7::timestamptz IS NULL OR (v.updated_at, v.id) < ($7::timestamptz, $8::uuid))
        ORDER BY v.updated_at DESC, v.id DESC
        LIMIT $10
        "#,
    )
    .bind(user.org_id.as_uuid())
    .bind(plan.restricted)
    .bind(&plan.project_uuids)
    .bind(fp.map(|p| p.as_uuid()))
    .bind(fpl.map(|p| p.as_uuid()))
    .bind(pattern.as_ref())
    .bind(cursor_t)
    .bind(cursor_id)
    .bind(scope_str)
    .bind(limit_bind)
    .fetch_all(state.db())
    .await
    .map_err(met_store::StoreError::from)?;

    let out: Vec<WorkspaceVariableListItem> = rows
        .into_iter()
        .map(|r| WorkspaceVariableListItem {
            id: VariableId::from_uuid(r.id),
            project_id: ProjectId::from_uuid(r.project_id),
            pipeline_id: r.pipeline_id.map(PipelineId::from_uuid),
            environment_id: r.environment_id,
            environment_name: r.environment_name,
            name: r.name,
            value: if r.is_sensitive { None } else { Some(r.value) },
            scope: r.scope,
            is_sensitive: r.is_sensitive,
            created_at: r.created_at,
            updated_at: r.updated_at,
            project_name: r.project_name,
            project_slug: r.project_slug,
            pipeline_name: r.pipeline_name,
        })
        .collect();

    let response = PaginatedResponse::new(out, pagination.limit, |v| {
        encode_cursor(v.updated_at, v.id.as_uuid())
    });

    Ok(Json(response))
}

#[utoipa::path(
    get,
    path = "/api/v1/workspace/stored-secrets",
    params(
        ("q" = Option<String>, Query),
        ("project_id" = Option<String>, Query),
        ("pipeline_id" = Option<String>, Query),
        ("scope_level" = Option<String>, Query),
        ("cursor" = Option<String>, Query),
        ("per_page" = Option<u32>, Query),
    ),
    responses(
        (status = 200, description = "Paginated stored-secret metadata", body = PaginatedResponse<WorkspaceStoredSecretListItem>),
        (status = 403, description = "Forbidden"),
    ),
    tag = "stored_secrets",
)]
#[instrument(skip(state))]
async fn list_workspace_stored_secrets(
    State(state): State<AppState>,
    Auth(user): Auth,
    pagination: Pagination,
    Query(q): Query<WorkspaceListQuery>,
) -> ApiResult<Json<PaginatedResponse<WorkspaceStoredSecretListItem>>> {
    let (fp, fpl) =
        resolve_project_pipeline_filters(&state, &user, q.project_id, q.pipeline_id).await?;

    if state.stored_secret_crypto.is_none() {
        return Err(ApiError::unavailable(
            crate::error::STORED_SECRETS_UNAVAILABLE,
        ));
    }

    let plan = project_access_plan(&user);
    let pattern =
        q.q.as_ref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(ilike_pattern);

    let (cursor_t, cursor_id) = match &pagination.cursor {
        None => (None, None),
        Some(c) => {
            let (t, id) = decode_cursor(c)?;
            (Some(t), Some(id))
        }
    };

    let scope_str = q.scope_level.as_sql_str();

    let rows = sqlx::query_as::<_, SecretHubRow>(
        r#"
        WITH latest AS (
          SELECT DISTINCT ON (path, project_id, pipeline_id, environment_id)
            id, org_id, project_id, pipeline_id, environment_id, path, kind, version, metadata, description,
            created_at, updated_at, propagate_to_projects
          FROM builtin_secrets
          WHERE org_id = $1
            AND deleted_at IS NULL
            AND (NOT $2::bool OR project_id IS NULL OR project_id = ANY($3))
            AND ($4::uuid IS NULL OR project_id IS NULL OR project_id = $4)
            AND ($5::uuid IS NULL OR pipeline_id IS NULL OR pipeline_id = $5)
            AND ($6::text IS NULL OR path ILIKE $6 ESCAPE '\' OR COALESCE(description, '') ILIKE $6 ESCAPE '\')
            AND (
              $9 = 'all'
              OR ($9 = 'organization' AND project_id IS NULL)
              OR ($9 = 'project' AND project_id IS NOT NULL AND pipeline_id IS NULL)
              OR ($9 = 'pipeline' AND pipeline_id IS NOT NULL)
            )
          ORDER BY path, project_id, pipeline_id, environment_id, version DESC
        )
        SELECT
          l.id, l.org_id, l.project_id, l.pipeline_id, l.environment_id, l.path, l.kind, l.version,
          l.metadata, l.description, l.created_at, l.updated_at, l.propagate_to_projects,
          p.name AS project_name, p.slug AS project_slug, pl.name AS pipeline_name,
          e.display_name AS environment_name
        FROM latest l
        LEFT JOIN projects p ON p.id = l.project_id
        LEFT JOIN pipelines pl ON pl.id = l.pipeline_id
        LEFT JOIN environments e ON e.id = l.environment_id
        WHERE ($7::timestamptz IS NULL OR (l.updated_at, l.id) < ($7::timestamptz, $8::uuid))
        ORDER BY l.updated_at DESC, l.id DESC
        LIMIT $10
        "#,
    )
    .bind(user.org_id.as_uuid())
    .bind(plan.restricted)
    .bind(&plan.project_uuids)
    .bind(fp.map(|p| p.as_uuid()))
    .bind(fpl.map(|p| p.as_uuid()))
    .bind(pattern.as_ref())
    .bind(cursor_t)
    .bind(cursor_id)
    .bind(scope_str)
    .bind(pagination.sql_limit())
    .fetch_all(state.db())
    .await
    .map_err(met_store::StoreError::from)?;

    let out: Vec<WorkspaceStoredSecretListItem> = rows
        .into_iter()
        .map(|r| {
            let meta = BuiltinSecretMetaRow {
                id: r.id,
                org_id: r.org_id,
                project_id: r.project_id,
                pipeline_id: r.pipeline_id,
                environment_id: r.environment_id,
                path: r.path,
                kind: r.kind,
                version: r.version,
                metadata: r.metadata,
                description: r.description,
                created_at: r.created_at,
                updated_at: r.updated_at,
                propagate_to_projects: r.propagate_to_projects,
            };
            WorkspaceStoredSecretListItem {
                secret: StoredSecretResponse::from(meta),
                project_name: r.project_name,
                project_slug: r.project_slug,
                pipeline_name: r.pipeline_name,
                environment_name: r.environment_name,
            }
        })
        .collect();

    let response = PaginatedResponse::new(out, pagination.limit, |v| {
        encode_cursor(v.secret.updated_at, v.secret.id)
    });

    Ok(Json(response))
}
