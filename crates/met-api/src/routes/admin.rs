//! Admin API routes.
//!
//! Provides endpoints for:
//! - User management (list, update, lock/unlock, delete)
//! - Group management (CRUD, membership)
//! - Role management (assign/revoke)
//! - Project admin operations (schedule deletion, force delete)

use crate::auth::{hash_password, hash_token};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::{delete, get, post},
};
use chrono::{Duration, Utc};
use met_core::ids::{
    AuthProviderId, GroupId, JoinTokenId, OidcGroupMappingId, PipelineId, ProjectId, UserId,
};
use met_core::models::{
    AuthProvider, CreateAuthProvider, CreateGroup, CreateOidcGroupMapping, Group, GroupMembership,
    GroupRole, JoinToken, JoinTokenDescriptionHistory, JoinTokenScope, OidcGroupMapping,
    PermissionRole, UpdateAuthProvider, User, UserRole, generate_join_token,
};
use met_store::repos::{
    ApiTokenRepo, AuthProviderRepo, GroupRepo, JobRunRepo, JoinTokenRepo, OrgPolicyPatch,
    OrgPolicyRepo, PipelineRepo, ProjectRepo, RoleRepo, UserRepo,
};
use serde::{Deserialize, Serialize};
use tracing::instrument;
use uuid::Uuid;

use crate::{
    error::{ApiError, ApiResult},
    extractors::{Auth, PaginatedResponse, Pagination},
    routes::auth::{AdminResetPasswordRequest, AdminResetPasswordResponse},
    routes::tokens::{
        CreateTokenRequest, CreateTokenResponseBody, TokenResponse, create_api_token_for_user,
    },
    state::AppState,
};

/// Build the admin router.
pub fn router() -> Router<AppState> {
    Router::new()
        .merge(crate::routes::admin_workflows::router())
        .merge(crate::routes::meticulous_apps::admin_router())
        // User management
        .route("/users", get(list_users).post(create_service_account_user))
        .route("/users/{id}", get(get_user).patch(update_user))
        .route("/users/{id}/lock", post(lock_user))
        .route("/users/{id}/unlock", post(unlock_user))
        .route("/users/{id}/reset-password", post(admin_reset_password))
        .route(
            "/users/{id}/tokens",
            post(admin_create_service_account_token),
        )
        .route("/users/{id}/delete", post(delete_user))
        // Group management
        .route("/groups", get(list_groups).post(create_group))
        .route(
            "/groups/{id}",
            get(get_group).patch(update_group).delete(delete_group),
        )
        .route(
            "/groups/{id}/members",
            get(list_group_members).post(add_group_member),
        )
        .route(
            "/groups/{id}/members/{user_id}",
            delete(remove_group_member).patch(update_group_member),
        )
        // Role management
        .route("/roles", get(list_roles))
        .route(
            "/users/{id}/roles",
            get(get_user_roles).post(assign_role),
        )
        .route("/users/{id}/roles/{role}", delete(revoke_role))
        // Project admin operations
        .route(
            "/projects/{id}/schedule-deletion",
            post(schedule_project_deletion),
        )
        .route(
            "/projects/{id}/cancel-deletion",
            post(cancel_project_deletion),
        )
        .route(
            "/projects/{id}/force-delete",
            post(force_delete_project),
        )
        .route("/archive", get(list_org_archive))
        .route("/projects/{id}/unarchive", post(admin_unarchive_project))
        .route("/pipelines/{id}/unarchive", post(admin_unarchive_pipeline))
        .route("/pipelines/{id}/purge", post(admin_purge_archived_pipeline))
        .route("/policy", get(get_admin_org_policy).patch(patch_admin_org_policy))
        .route("/tokens", get(list_admin_org_api_tokens))
        .route(
            "/projects/{id}/members",
            get(list_project_members).post(add_project_member),
        )
        // Auth provider management
        .route(
            "/auth-providers",
            get(list_auth_providers).post(create_auth_provider),
        )
        .route(
            "/auth-providers/{id}",
            get(get_auth_provider)
                .patch(update_auth_provider)
                .delete(delete_auth_provider),
        )
        .route(
            "/auth-providers/{id}/enable",
            post(enable_auth_provider),
        )
        .route(
            "/auth-providers/{id}/disable",
            post(disable_auth_provider),
        )
        // OIDC group mapping management
        .route(
            "/auth-providers/{id}/group-mappings",
            get(list_group_mappings).post(create_group_mapping),
        )
        .route(
            "/auth-providers/{provider_id}/group-mappings/{mapping_id}",
            delete(delete_group_mapping),
        )
        // Join token management
        .route(
            "/join-tokens",
            get(list_join_tokens).post(create_join_token),
        )
        .route("/join-tokens/{id}/revoke", post(revoke_join_token))
        .route(
            "/join-tokens/{id}",
            get(get_join_token)
                .patch(update_join_token)
                .delete(delete_join_token),
        )
        .route("/ops/jobs-dlq", get(list_jobs_dlq))
        .route("/ops/job-queue", get(list_job_queue))
}

// ============================================================================
// Middleware / Guards
// ============================================================================

pub(crate) fn require_admin(user: &crate::extractors::CurrentUser) -> ApiResult<()> {
    if !user.has_permission("*") {
        return Err(ApiError::forbidden("admin access required"));
    }
    Ok(())
}

// ============================================================================
// Ops / JetStream
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct JobsDlqQuery {
    pub limit: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct JobsDlqListResponse {
    pub messages: Vec<serde_json::Value>,
}

#[instrument(skip(state))]
async fn list_jobs_dlq(
    State(state): State<AppState>,
    Auth(admin): Auth,
    Query(q): Query<JobsDlqQuery>,
) -> ApiResult<Json<JobsDlqListResponse>> {
    require_admin(&admin)?;
    let Some(nats) = state.nats_ops.as_ref() else {
        return Err(ApiError::unavailable(
            "NATS is not connected; job DLQ preview is unavailable",
        ));
    };
    let limit = q.limit.unwrap_or(50);
    let messages = nats
        .fetch_recent_jobs_dlq(admin.org_id, limit)
        .await
        .map_err(|e| ApiError::internal(format!("DLQ fetch failed: {e}")))?;
    Ok(Json(JobsDlqListResponse { messages }))
}

#[derive(Debug, Deserialize)]
pub struct JobQueueQuery {
    pub limit: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct JobQueueEntryResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_run_id: Option<String>,
    pub run_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
    pub job_name: String,
    pub job_status: String,
    pub attempt: i32,
    pub job_run_created_at: String,
    pub run_number: i64,
    pub run_status: String,
    pub pipeline_id: String,
    pub pipeline_name: String,
    pub project_id: String,
    pub project_slug: String,
}

#[derive(Debug, Serialize)]
pub struct JobQueueListResponse {
    pub count: usize,
    pub data: Vec<JobQueueEntryResponse>,
}

#[instrument(skip(state))]
async fn list_job_queue(
    State(state): State<AppState>,
    Auth(admin): Auth,
    Query(q): Query<JobQueueQuery>,
) -> ApiResult<Json<JobQueueListResponse>> {
    require_admin(&admin)?;
    let limit = i64::from(q.limit.unwrap_or(200).min(500));
    let repo = JobRunRepo::new(state.db());
    let rows = repo
        .list_job_queue_for_org(admin.org_id, limit)
        .await
        .map_err(|e| ApiError::internal(format!("job queue query failed: {e}")))?;

    let data: Vec<JobQueueEntryResponse> = rows
        .into_iter()
        .map(|r| JobQueueEntryResponse {
            job_run_id: r.job_run_id.map(|u| u.to_string()),
            run_id: r.run_id.to_string(),
            job_id: r.job_id.map(|u| u.to_string()),
            job_name: r.job_name,
            job_status: format!("{:?}", r.status).to_lowercase(),
            attempt: r.attempt,
            job_run_created_at: r.job_run_created_at.to_rfc3339(),
            run_number: r.run_number,
            run_status: format!("{:?}", r.run_status).to_lowercase(),
            pipeline_id: r.pipeline_id.to_string(),
            pipeline_name: r.pipeline_name,
            project_id: r.project_id.to_string(),
            project_slug: r.project_slug,
        })
        .collect();

    let count = data.len();
    Ok(Json(JobQueueListResponse { count, data }))
}

// ============================================================================
// User Management
// ============================================================================

#[derive(Debug, Serialize)]
pub struct UserResponse {
    pub id: String,
    pub username: String,
    pub email: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub is_active: bool,
    pub is_admin: bool,
    #[serde(default)]
    pub service_account: bool,
    pub password_must_change: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_login_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl From<&User> for UserResponse {
    fn from(u: &User) -> Self {
        Self {
            id: u.id.to_string(),
            username: u.username.clone(),
            email: u.email.clone(),
            display_name: u.display_name.clone(),
            is_active: u.is_active,
            is_admin: u.is_admin,
            service_account: u.service_account,
            password_must_change: u.password_must_change,
            last_login_at: u.last_login_at.map(|t| t.to_rfc3339()),
            created_at: u.created_at.to_rfc3339(),
            updated_at: u.updated_at.to_rfc3339(),
        }
    }
}

#[instrument(skip(state))]
async fn list_users(
    State(state): State<AppState>,
    Auth(admin): Auth,
    pagination: Pagination,
) -> ApiResult<Json<PaginatedResponse<UserResponse>>> {
    require_admin(&admin)?;

    let repo = UserRepo::new(state.db());
    let users = repo.list(admin.org_id, pagination.sql_limit(), 0).await?;

    let response = PaginatedResponse::new(
        users.iter().map(UserResponse::from).collect(),
        pagination.limit,
        |u| u.id.clone(),
    );

    Ok(Json(response))
}

#[derive(Debug, Deserialize)]
pub struct AdminCreateServiceAccountRequest {
    pub username: String,
    pub email: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub is_admin: bool,
}

#[instrument(skip(state, req))]
async fn create_service_account_user(
    State(state): State<AppState>,
    Auth(admin): Auth,
    Json(req): Json<AdminCreateServiceAccountRequest>,
) -> ApiResult<Json<UserResponse>> {
    require_admin(&admin)?;

    let username = req.username.trim();
    let email = req.email.trim();
    if username.is_empty() || email.is_empty() {
        return Err(ApiError::bad_request("username and email are required"));
    }

    let repo = UserRepo::new(state.db());
    if repo
        .get_by_username(admin.org_id, username)
        .await?
        .is_some()
    {
        return Err(ApiError::bad_request("username already exists"));
    }
    if repo.get_by_email(admin.org_id, email).await?.is_some() {
        return Err(ApiError::bad_request("email already exists"));
    }

    let user = repo
        .create(
            admin.org_id,
            username,
            email,
            req.display_name.as_deref(),
            None,
            req.is_admin,
            true,
            false,
        )
        .await?;

    tracing::info!(
        admin_id = %admin.user_id,
        new_user_id = %user.id,
        "service account user created"
    );

    Ok(Json(UserResponse::from(&user)))
}

#[derive(Debug, Serialize)]
pub struct OrgPolicyApiResponse {
    pub max_api_token_ttl_days: i32,
    pub user_rl_primary_period_secs: i32,
    pub user_rl_primary_max: i32,
    pub user_rl_secondary_period_secs: i32,
    pub user_rl_secondary_max: i32,
    pub app_rl_primary_period_secs: i32,
    pub app_rl_primary_max: i32,
    pub app_rl_secondary_period_secs: i32,
    pub app_rl_secondary_max: i32,
}

impl From<met_store::repos::OrgPolicy> for OrgPolicyApiResponse {
    fn from(p: met_store::repos::OrgPolicy) -> Self {
        Self {
            max_api_token_ttl_days: p.max_api_token_ttl_days,
            user_rl_primary_period_secs: p.user_rl_primary_period_secs,
            user_rl_primary_max: p.user_rl_primary_max,
            user_rl_secondary_period_secs: p.user_rl_secondary_period_secs,
            user_rl_secondary_max: p.user_rl_secondary_max,
            app_rl_primary_period_secs: p.app_rl_primary_period_secs,
            app_rl_primary_max: p.app_rl_primary_max,
            app_rl_secondary_period_secs: p.app_rl_secondary_period_secs,
            app_rl_secondary_max: p.app_rl_secondary_max,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct PatchOrgPolicyRequest {
    pub max_api_token_ttl_days: Option<i32>,
    pub user_rl_primary_period_secs: Option<i32>,
    pub user_rl_primary_max: Option<i32>,
    pub user_rl_secondary_period_secs: Option<i32>,
    pub user_rl_secondary_max: Option<i32>,
    pub app_rl_primary_period_secs: Option<i32>,
    pub app_rl_primary_max: Option<i32>,
    pub app_rl_secondary_period_secs: Option<i32>,
    pub app_rl_secondary_max: Option<i32>,
}

#[instrument(skip(state))]
async fn get_admin_org_policy(
    State(state): State<AppState>,
    Auth(admin): Auth,
) -> ApiResult<Json<OrgPolicyApiResponse>> {
    require_admin(&admin)?;
    let p = OrgPolicyRepo::new(state.db())
        .get(admin.org_id)
        .await?;
    Ok(Json(OrgPolicyApiResponse::from(p)))
}

#[instrument(skip(state, req))]
async fn patch_admin_org_policy(
    State(state): State<AppState>,
    Auth(admin): Auth,
    Json(req): Json<PatchOrgPolicyRequest>,
) -> ApiResult<Json<OrgPolicyApiResponse>> {
    require_admin(&admin)?;
    let patch = OrgPolicyPatch {
        max_api_token_ttl_days: req.max_api_token_ttl_days,
        user_rl_primary_period_secs: req.user_rl_primary_period_secs,
        user_rl_primary_max: req.user_rl_primary_max,
        user_rl_secondary_period_secs: req.user_rl_secondary_period_secs,
        user_rl_secondary_max: req.user_rl_secondary_max,
        app_rl_primary_period_secs: req.app_rl_primary_period_secs,
        app_rl_primary_max: req.app_rl_primary_max,
        app_rl_secondary_period_secs: req.app_rl_secondary_period_secs,
        app_rl_secondary_max: req.app_rl_secondary_max,
    };
    let p = OrgPolicyRepo::new(state.db())
        .upsert(admin.org_id, &patch)
        .await?;
    Ok(Json(OrgPolicyApiResponse::from(p)))
}

#[derive(Debug, Serialize)]
pub struct AdminOrgTokenRow {
    pub token: TokenResponse,
    pub owner_user_id: String,
    pub owner_email: String,
}

#[instrument(skip(state))]
async fn list_admin_org_api_tokens(
    State(state): State<AppState>,
    Auth(admin): Auth,
    pagination: Pagination,
) -> ApiResult<Json<Vec<AdminOrgTokenRow>>> {
    require_admin(&admin)?;
    let repo = ApiTokenRepo::new(state.db());
    let tokens = repo
        .list_for_org(admin.org_id, pagination.sql_limit(), 0)
        .await?;
    let users = UserRepo::new(state.db());
    let mut out = Vec::with_capacity(tokens.len());
    for t in tokens {
        let owner = users.get(t.user_id).await?;
        out.push(AdminOrgTokenRow {
            token: TokenResponse::from(&t),
            owner_user_id: t.user_id.to_string(),
            owner_email: owner.email,
        });
    }
    Ok(Json(out))
}

#[derive(Debug, Serialize)]
pub struct ArchiveListResponse {
    pub projects: Vec<serde_json::Value>,
    pub pipelines: Vec<serde_json::Value>,
}

#[instrument(skip(state))]
async fn list_org_archive(
    State(state): State<AppState>,
    Auth(admin): Auth,
) -> ApiResult<Json<ArchiveListResponse>> {
    require_admin(&admin)?;
    let prepo = ProjectRepo::new(state.db());
    let projects = prepo.list_archived(admin.org_id, 500, 0).await?;
    let pipes = PipelineRepo::new(state.db())
        .list_archived_for_org(admin.org_id)
        .await?;
    let projects_json = projects
        .iter()
        .map(|p| {
            serde_json::json!({
                "id": p.id.to_string(),
                "name": p.name,
                "slug": p.slug,
                "archived_at": p.archived_at.map(|t| t.to_rfc3339()),
            })
        })
        .collect();
    let pipelines_json = pipes
        .iter()
        .map(|p| {
            serde_json::json!({
                "id": p.id.to_string(),
                "project_id": p.project_id.to_string(),
                "name": p.name,
                "slug": p.slug,
                "archived_at": p.archived_at.map(|t| t.to_rfc3339()),
            })
        })
        .collect();
    Ok(Json(ArchiveListResponse {
        projects: projects_json,
        pipelines: pipelines_json,
    }))
}

#[instrument(skip(state))]
async fn admin_unarchive_project(
    State(state): State<AppState>,
    Auth(admin): Auth,
    Path(project_id): Path<ProjectId>,
) -> ApiResult<Json<serde_json::Value>> {
    require_admin(&admin)?;
    let repo = ProjectRepo::new(state.db());
    let p = repo.get_including_deleted(project_id).await?;
    if p.org_id != admin.org_id {
        return Err(ApiError::forbidden(
            "cannot modify projects in other organizations",
        ));
    }
    repo.unarchive(project_id).await?;
    PipelineRepo::new(state.db())
        .unarchive_all_in_project(project_id)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    Ok(Json(serde_json::json!({ "message": "project unarchived" })))
}

#[instrument(skip(state))]
async fn admin_unarchive_pipeline(
    State(state): State<AppState>,
    Auth(admin): Auth,
    Path(pipeline_id): Path<PipelineId>,
) -> ApiResult<Json<serde_json::Value>> {
    require_admin(&admin)?;
    let prepo = PipelineRepo::new(state.db());
    let pipe = prepo.get(pipeline_id).await?;
    let proj = ProjectRepo::new(state.db()).get(pipe.project_id).await?;
    if proj.org_id != admin.org_id {
        return Err(ApiError::forbidden(
            "cannot modify pipelines in other organizations",
        ));
    }
    prepo.unarchive(pipeline_id).await?;
    Ok(Json(serde_json::json!({ "message": "pipeline unarchived" })))
}

#[derive(Debug, Deserialize)]
pub struct AddProjectMemberRequest {
    /// Legacy: prefer `principal_id` + `principal_type` for new clients.
    #[serde(default)]
    pub user_id: Option<UserId>,
    /// `user` (default) or `group`.
    #[serde(default)]
    pub principal_type: Option<String>,
    #[serde(default)]
    pub principal_id: Option<Uuid>,
    /// One of: `admin`, `developer`, `readonly`.
    pub role: String,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct ProjectMemberRow {
    principal_type: String,
    principal_id: Uuid,
    role: String,
    display_name: Option<String>,
}

#[instrument(skip(state))]
async fn list_project_members(
    State(state): State<AppState>,
    Auth(admin): Auth,
    Path(project_id): Path<ProjectId>,
) -> ApiResult<Json<Vec<ProjectMemberRow>>> {
    require_admin(&admin)?;
    let proj = ProjectRepo::new(state.db()).get(project_id).await?;
    if proj.org_id != admin.org_id {
        return Err(ApiError::forbidden(
            "cannot access projects in other organizations",
        ));
    }
    let rows: Vec<ProjectMemberRow> = sqlx::query_as(
        r#"
        SELECT pm.principal_type::text AS principal_type,
               pm.principal_id,
               pm.role::text AS role,
               CASE
                 WHEN pm.principal_type = 'user' THEN u.email
                 ELSE g.name
               END AS display_name
        FROM project_members pm
        LEFT JOIN users u ON pm.principal_type = 'user' AND u.id = pm.principal_id
        LEFT JOIN groups g ON pm.principal_type = 'group' AND g.id = pm.principal_id
        WHERE pm.project_id = $1
        ORDER BY pm.principal_type::text, display_name NULLS LAST
        "#,
    )
    .bind(project_id.as_uuid())
    .fetch_all(state.db())
    .await
    .map_err(met_store::StoreError::from)?;
    Ok(Json(rows))
}

enum ProjectPrincipalTarget {
    User(UserId),
    Group(GroupId),
}

fn resolve_add_project_member_principal(req: &AddProjectMemberRequest) -> ApiResult<ProjectPrincipalTarget> {
    let pt = req
        .principal_type
        .as_deref()
        .unwrap_or("user")
        .trim()
        .to_lowercase();
    match pt.as_str() {
        "user" => {
            let uid = if let Some(pid) = req.principal_id {
                UserId::from_uuid(pid)
            } else if let Some(u) = req.user_id {
                u
            } else {
                return Err(ApiError::bad_request(
                    "user principal requires `user_id` or `principal_id`",
                ));
            };
            Ok(ProjectPrincipalTarget::User(uid))
        }
        "group" => {
            let gid = req.principal_id.ok_or_else(|| {
                ApiError::bad_request("group principal requires `principal_id`")
            })?;
            Ok(ProjectPrincipalTarget::Group(GroupId::from_uuid(gid)))
        }
        _ => Err(ApiError::bad_request("principal_type must be user or group")),
    }
}

#[instrument(skip(state, req))]
async fn add_project_member(
    State(state): State<AppState>,
    Auth(admin): Auth,
    Path(project_id): Path<ProjectId>,
    Json(req): Json<AddProjectMemberRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    require_admin(&admin)?;
    let proj = ProjectRepo::new(state.db()).get(project_id).await?;
    if proj.org_id != admin.org_id {
        return Err(ApiError::forbidden(
            "cannot modify projects in other organizations",
        ));
    }
    let target = resolve_add_project_member_principal(&req)?;
    match target {
        ProjectPrincipalTarget::User(uid) => {
            let urow = UserRepo::new(state.db()).get(uid).await?;
            if urow.org_id != admin.org_id {
                return Err(ApiError::bad_request("user is not in this organization"));
            }
        }
        ProjectPrincipalTarget::Group(gid) => {
            let grow = GroupRepo::new(state.db()).get(gid).await?;
            if grow.org_id != admin.org_id {
                return Err(ApiError::bad_request("group is not in this organization"));
            }
        }
    }
    let role = req.role.trim().to_lowercase();
    if !matches!(role.as_str(), "admin" | "developer" | "readonly") {
        return Err(ApiError::bad_request(
            "role must be admin, developer, or readonly",
        ));
    }
    let (ptype, pid) = match target {
        ProjectPrincipalTarget::User(u) => ("user", u.as_uuid()),
        ProjectPrincipalTarget::Group(g) => ("group", g.as_uuid()),
    };
    sqlx::query(
        r#"
        INSERT INTO project_members (project_id, principal_type, principal_id, role)
        VALUES ($1, $2::project_principal_type, $3, $4::project_role)
        ON CONFLICT (project_id, principal_type, principal_id)
        DO UPDATE SET role = EXCLUDED.role
        "#,
    )
    .bind(project_id.as_uuid())
    .bind(ptype)
    .bind(pid)
    .bind(&role)
    .execute(state.db())
    .await
    .map_err(met_store::StoreError::from)?;
    Ok(Json(serde_json::json!({ "message": "project member saved" })))
}

#[instrument(skip(state))]
async fn admin_purge_archived_pipeline(
    State(state): State<AppState>,
    Auth(admin): Auth,
    Path(pipeline_id): Path<PipelineId>,
) -> ApiResult<Json<serde_json::Value>> {
    require_admin(&admin)?;
    let prepo = PipelineRepo::new(state.db());
    let pipe = prepo.get(pipeline_id).await?;
    if pipe.archived_at.is_none() {
        return Err(ApiError::bad_request(
            "pipeline must be archived before purge",
        ));
    }
    let proj = ProjectRepo::new(state.db()).get(pipe.project_id).await?;
    if proj.org_id != admin.org_id {
        return Err(ApiError::forbidden(
            "cannot delete pipelines in other organizations",
        ));
    }
    prepo.delete(pipeline_id).await?;
    Ok(Json(serde_json::json!({ "message": "pipeline purged" })))
}

#[instrument(skip(state))]
async fn get_user(
    State(state): State<AppState>,
    Auth(admin): Auth,
    Path(user_id): Path<UserId>,
) -> ApiResult<Json<UserResponse>> {
    require_admin(&admin)?;

    let repo = UserRepo::new(state.db());
    let user = repo.get(user_id).await?;

    if user.org_id != admin.org_id {
        return Err(ApiError::forbidden(
            "cannot access users in other organizations",
        ));
    }

    Ok(Json(UserResponse::from(&user)))
}

#[derive(Debug, Deserialize)]
pub struct UpdateUserRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_admin: Option<bool>,
}

#[instrument(skip(state, req))]
async fn update_user(
    State(state): State<AppState>,
    Auth(admin): Auth,
    Path(user_id): Path<UserId>,
    Json(req): Json<UpdateUserRequest>,
) -> ApiResult<Json<UserResponse>> {
    require_admin(&admin)?;

    let repo = UserRepo::new(state.db());
    let mut user = repo.get(user_id).await?;

    if user.org_id != admin.org_id {
        return Err(ApiError::forbidden(
            "cannot modify users in other organizations",
        ));
    }

    // Prevent admin from removing their own admin status
    if user_id == admin.user_id && req.is_admin == Some(false) {
        return Err(ApiError::bad_request("cannot remove your own admin status"));
    }

    if let Some(display_name) = req.display_name {
        user.display_name = Some(display_name);
    }

    if let Some(is_admin) = req.is_admin {
        user.is_admin = is_admin;
    }

    // Update in database
    sqlx::query(
        r#"
        UPDATE users SET display_name = $2, is_admin = $3, updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(user_id.as_uuid())
    .bind(&user.display_name)
    .bind(user.is_admin)
    .execute(state.db())
    .await
    .map_err(met_store::StoreError::from)?;

    tracing::info!(admin_id = %admin.user_id, target_user_id = %user_id, "user updated");

    Ok(Json(UserResponse::from(&user)))
}

#[instrument(skip(state))]
async fn lock_user(
    State(state): State<AppState>,
    Auth(admin): Auth,
    Path(user_id): Path<UserId>,
) -> ApiResult<Json<UserResponse>> {
    require_admin(&admin)?;

    let repo = UserRepo::new(state.db());
    let user = repo.get(user_id).await?;

    if user.org_id != admin.org_id {
        return Err(ApiError::forbidden(
            "cannot lock users in other organizations",
        ));
    }

    if user_id == admin.user_id {
        return Err(ApiError::forbidden(
            "cannot lock your own account; use another admin if you need this account disabled",
        ));
    }

    sqlx::query(
        r#"
        UPDATE users SET is_active = false, updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(user_id.as_uuid())
    .execute(state.db())
    .await
    .map_err(met_store::StoreError::from)?;

    let user = repo.get(user_id).await?;

    tracing::info!(admin_id = %admin.user_id, target_user_id = %user_id, "user locked");

    Ok(Json(UserResponse::from(&user)))
}

#[instrument(skip(state))]
async fn unlock_user(
    State(state): State<AppState>,
    Auth(admin): Auth,
    Path(user_id): Path<UserId>,
) -> ApiResult<Json<UserResponse>> {
    require_admin(&admin)?;

    let repo = UserRepo::new(state.db());
    let user = repo.get(user_id).await?;

    if user.org_id != admin.org_id {
        return Err(ApiError::forbidden(
            "cannot unlock users in other organizations",
        ));
    }

    sqlx::query(
        r#"
        UPDATE users SET is_active = true, updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(user_id.as_uuid())
    .execute(state.db())
    .await
    .map_err(met_store::StoreError::from)?;

    let user = repo.get(user_id).await?;

    tracing::info!(admin_id = %admin.user_id, target_user_id = %user_id, "user unlocked");

    Ok(Json(UserResponse::from(&user)))
}

#[instrument(skip(state))]
async fn delete_user(
    State(state): State<AppState>,
    Auth(admin): Auth,
    Path(user_id): Path<UserId>,
) -> ApiResult<Json<serde_json::Value>> {
    require_admin(&admin)?;

    let repo = UserRepo::new(state.db());
    let user = repo.get(user_id).await?;

    if user.org_id != admin.org_id {
        return Err(ApiError::forbidden(
            "cannot delete users in other organizations",
        ));
    }

    if user_id == admin.user_id {
        return Err(ApiError::bad_request("cannot delete your own account"));
    }

    // Remove all privileges before soft-deleting (GDPR compliance)
    // This ensures that if the user is ever restored, they start fresh

    // 1. Remove all role assignments
    sqlx::query("DELETE FROM user_roles WHERE user_id = $1")
        .bind(user_id.as_uuid())
        .execute(state.db())
        .await
        .map_err(met_store::StoreError::from)?;

    // 2. Remove all group memberships
    sqlx::query("DELETE FROM group_memberships WHERE user_id = $1")
        .bind(user_id.as_uuid())
        .execute(state.db())
        .await
        .map_err(met_store::StoreError::from)?;

    // 3. Revoke all API tokens (set revoked_at instead of deleting for audit trail)
    sqlx::query(
        "UPDATE api_tokens SET revoked_at = NOW() WHERE user_id = $1 AND revoked_at IS NULL",
    )
    .bind(user_id.as_uuid())
    .execute(state.db())
    .await
    .map_err(met_store::StoreError::from)?;

    // 4. Soft-delete the user and remove admin privileges
    sqlx::query(
        r#"
        UPDATE users 
        SET deleted_at = NOW(), 
            updated_at = NOW(), 
            is_admin = false,
            is_active = false
        WHERE id = $1
        "#,
    )
    .bind(user_id.as_uuid())
    .execute(state.db())
    .await
    .map_err(met_store::StoreError::from)?;

    tracing::info!(
        admin_id = %admin.user_id,
        target_user_id = %user_id,
        "user deleted with all privileges removed"
    );

    Ok(Json(serde_json::json!({ "message": "user deleted" })))
}

/// Admin endpoint to reset a user's password.
#[utoipa::path(
    post,
    path = "/api/v1/admin/users/{id}/reset-password",
    params(("id" = String, Path, description = "User ID")),
    request_body = AdminResetPasswordRequest,
    responses(
        (status = 200, description = "Password reset", body = AdminResetPasswordResponse),
        (status = 400, description = "Bad request"),
        (status = 403, description = "Admin access required"),
    ),
    tag = "admin",
)]
#[instrument(skip(state, req))]
pub async fn admin_reset_password(
    State(state): State<AppState>,
    Auth(admin): Auth,
    Path(user_id): Path<UserId>,
    Json(req): Json<AdminResetPasswordRequest>,
) -> ApiResult<Json<AdminResetPasswordResponse>> {
    require_admin(&admin)?;

    if !state.config.auth.password_enabled {
        return Err(ApiError::forbidden("password authentication is disabled"));
    }

    let user_repo = UserRepo::new(state.db());
    let min_length = state.config.auth.min_password_length;
    if req.new_password.len() < min_length {
        return Err(ApiError::bad_request(format!(
            "password must be at least {} characters",
            min_length
        )));
    }

    let target_user = user_repo.get(user_id).await?;

    if target_user.org_id != admin.org_id {
        return Err(ApiError::forbidden(
            "cannot reset password for users in other organizations",
        ));
    }

    let new_hash = hash_password(&req.new_password)
        .map_err(|e| ApiError::internal(format!("failed to hash password: {e}")))?;

    user_repo.update_password(user_id, &new_hash).await?;

    tracing::info!(
        admin_id = %admin.user_id,
        target_user_id = %user_id,
        "admin reset user password"
    );

    Ok(Json(AdminResetPasswordResponse {
        message: "password reset successfully".to_string(),
    }))
}

#[instrument(skip(state, req))]
async fn admin_create_service_account_token(
    State(state): State<AppState>,
    Auth(admin): Auth,
    Path(user_id): Path<UserId>,
    Json(req): Json<CreateTokenRequest>,
) -> ApiResult<Json<CreateTokenResponseBody>> {
    require_admin(&admin)?;
    let target = UserRepo::new(state.db()).get(user_id).await?;
    if target.org_id != admin.org_id {
        return Err(ApiError::forbidden(
            "cannot create tokens for users in other organizations",
        ));
    }
    if !target.service_account {
        return Err(ApiError::bad_request(
            "admin-created API tokens are limited to service account users",
        ));
    }
    let body = create_api_token_for_user(
        &state,
        admin.org_id,
        user_id,
        req,
        false, /* service accounts may hold more than two tokens when provisioned by admin */
    )
    .await?;
    tracing::info!(
        admin_id = %admin.user_id,
        target_user_id = %user_id,
        "admin created API token for service account"
    );
    Ok(Json(body))
}

// ============================================================================
// Group Management
// ============================================================================

#[derive(Debug, Serialize)]
pub struct GroupResponse {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub member_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[instrument(skip(state))]
async fn list_groups(
    State(state): State<AppState>,
    Auth(admin): Auth,
    pagination: Pagination,
) -> ApiResult<Json<PaginatedResponse<GroupResponse>>> {
    require_admin(&admin)?;

    let repo = GroupRepo::new(state.db());
    let groups = repo.list(admin.org_id, pagination.sql_limit(), 0).await?;

    let mut responses = Vec::with_capacity(groups.len());
    for group in groups {
        let member_count = repo.count_members(group.id).await?;
        responses.push(GroupResponse {
            id: group.id.to_string(),
            name: group.name,
            description: group.description,
            member_count,
            created_at: group.created_at.to_rfc3339(),
            updated_at: group.updated_at.to_rfc3339(),
        });
    }

    let response = PaginatedResponse::new(responses, pagination.limit, |g| g.id.clone());

    Ok(Json(response))
}

#[derive(Debug, Deserialize)]
pub struct CreateGroupRequest {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[instrument(skip(state, req))]
async fn create_group(
    State(state): State<AppState>,
    Auth(admin): Auth,
    Json(req): Json<CreateGroupRequest>,
) -> ApiResult<Json<GroupResponse>> {
    require_admin(&admin)?;

    if req.name.is_empty() {
        return Err(ApiError::bad_request("name is required"));
    }

    let repo = GroupRepo::new(state.db());

    let input = CreateGroup {
        name: req.name,
        description: req.description,
    };

    let group = repo.create(admin.org_id, &input).await?;

    tracing::info!(admin_id = %admin.user_id, group_id = %group.id, "group created");

    Ok(Json(GroupResponse {
        id: group.id.to_string(),
        name: group.name,
        description: group.description,
        member_count: 0,
        created_at: group.created_at.to_rfc3339(),
        updated_at: group.updated_at.to_rfc3339(),
    }))
}

#[instrument(skip(state))]
async fn get_group(
    State(state): State<AppState>,
    Auth(admin): Auth,
    Path(group_id): Path<GroupId>,
) -> ApiResult<Json<GroupResponse>> {
    require_admin(&admin)?;

    let repo = GroupRepo::new(state.db());
    let group = repo.get(group_id).await?;

    if group.org_id != admin.org_id {
        return Err(ApiError::forbidden(
            "cannot access groups in other organizations",
        ));
    }

    let member_count = repo.count_members(group_id).await?;

    Ok(Json(GroupResponse {
        id: group.id.to_string(),
        name: group.name,
        description: group.description,
        member_count,
        created_at: group.created_at.to_rfc3339(),
        updated_at: group.updated_at.to_rfc3339(),
    }))
}

#[derive(Debug, Deserialize)]
pub struct UpdateGroupRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[instrument(skip(state, req))]
async fn update_group(
    State(state): State<AppState>,
    Auth(admin): Auth,
    Path(group_id): Path<GroupId>,
    Json(req): Json<UpdateGroupRequest>,
) -> ApiResult<Json<GroupResponse>> {
    require_admin(&admin)?;

    let repo = GroupRepo::new(state.db());
    let existing = repo.get(group_id).await?;

    if existing.org_id != admin.org_id {
        return Err(ApiError::forbidden(
            "cannot modify groups in other organizations",
        ));
    }

    let group = repo
        .update(group_id, req.name.as_deref(), req.description.as_deref())
        .await?;
    let member_count = repo.count_members(group_id).await?;

    tracing::info!(admin_id = %admin.user_id, group_id = %group_id, "group updated");

    Ok(Json(GroupResponse {
        id: group.id.to_string(),
        name: group.name,
        description: group.description,
        member_count,
        created_at: group.created_at.to_rfc3339(),
        updated_at: group.updated_at.to_rfc3339(),
    }))
}

#[instrument(skip(state))]
async fn delete_group(
    State(state): State<AppState>,
    Auth(admin): Auth,
    Path(group_id): Path<GroupId>,
) -> ApiResult<Json<serde_json::Value>> {
    require_admin(&admin)?;

    let repo = GroupRepo::new(state.db());
    let group = repo.get(group_id).await?;

    if group.org_id != admin.org_id {
        return Err(ApiError::forbidden(
            "cannot delete groups in other organizations",
        ));
    }

    repo.delete(group_id).await?;

    tracing::info!(admin_id = %admin.user_id, group_id = %group_id, "group deleted");

    Ok(Json(serde_json::json!({ "message": "group deleted" })))
}

// ============================================================================
// Group Membership
// ============================================================================

#[derive(Debug, Serialize)]
pub struct GroupMemberResponse {
    pub user_id: String,
    pub username: String,
    pub email: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub role: GroupRole,
    pub joined_at: String,
}

#[instrument(skip(state))]
async fn list_group_members(
    State(state): State<AppState>,
    Auth(admin): Auth,
    Path(group_id): Path<GroupId>,
) -> ApiResult<Json<Vec<GroupMemberResponse>>> {
    require_admin(&admin)?;

    let group_repo = GroupRepo::new(state.db());
    let user_repo = UserRepo::new(state.db());

    let group = group_repo.get(group_id).await?;
    if group.org_id != admin.org_id {
        return Err(ApiError::forbidden(
            "cannot access groups in other organizations",
        ));
    }

    let memberships = group_repo.list_members(group_id).await?;

    let mut responses = Vec::with_capacity(memberships.len());
    for m in memberships {
        let user = user_repo.get(m.user_id).await?;
        responses.push(GroupMemberResponse {
            user_id: m.user_id.to_string(),
            username: user.username,
            email: user.email,
            display_name: user.display_name,
            role: m.role,
            joined_at: m.created_at.to_rfc3339(),
        });
    }

    Ok(Json(responses))
}

#[derive(Debug, Deserialize)]
pub struct AddGroupMemberRequest {
    pub user_id: UserId,
    #[serde(default)]
    pub role: GroupRole,
}

#[instrument(skip(state, req))]
async fn add_group_member(
    State(state): State<AppState>,
    Auth(admin): Auth,
    Path(group_id): Path<GroupId>,
    Json(req): Json<AddGroupMemberRequest>,
) -> ApiResult<Json<GroupMemberResponse>> {
    require_admin(&admin)?;

    let group_repo = GroupRepo::new(state.db());
    let user_repo = UserRepo::new(state.db());

    let group = group_repo.get(group_id).await?;
    if group.org_id != admin.org_id {
        return Err(ApiError::forbidden(
            "cannot modify groups in other organizations",
        ));
    }

    let user = user_repo.get(req.user_id).await?;
    if user.org_id != admin.org_id {
        return Err(ApiError::forbidden(
            "cannot add users from other organizations",
        ));
    }

    let membership = group_repo
        .add_member(group_id, req.user_id, req.role)
        .await?;

    tracing::info!(
        admin_id = %admin.user_id,
        group_id = %group_id,
        user_id = %req.user_id,
        "member added to group"
    );

    Ok(Json(GroupMemberResponse {
        user_id: membership.user_id.to_string(),
        username: user.username,
        email: user.email,
        display_name: user.display_name,
        role: membership.role,
        joined_at: membership.created_at.to_rfc3339(),
    }))
}

#[derive(Debug, Deserialize)]
pub struct UpdateGroupMemberRequest {
    pub role: GroupRole,
}

#[instrument(skip(state, req))]
async fn update_group_member(
    State(state): State<AppState>,
    Auth(admin): Auth,
    Path((group_id, user_id)): Path<(GroupId, UserId)>,
    Json(req): Json<UpdateGroupMemberRequest>,
) -> ApiResult<Json<GroupMemberResponse>> {
    require_admin(&admin)?;

    let group_repo = GroupRepo::new(state.db());
    let user_repo = UserRepo::new(state.db());

    let group = group_repo.get(group_id).await?;
    if group.org_id != admin.org_id {
        return Err(ApiError::forbidden(
            "cannot modify groups in other organizations",
        ));
    }

    let user = user_repo.get(user_id).await?;
    let membership = group_repo
        .update_member_role(group_id, user_id, req.role)
        .await?;

    tracing::info!(
        admin_id = %admin.user_id,
        group_id = %group_id,
        user_id = %user_id,
        role = ?req.role,
        "group member role updated"
    );

    Ok(Json(GroupMemberResponse {
        user_id: membership.user_id.to_string(),
        username: user.username,
        email: user.email,
        display_name: user.display_name,
        role: membership.role,
        joined_at: membership.created_at.to_rfc3339(),
    }))
}

#[instrument(skip(state))]
async fn remove_group_member(
    State(state): State<AppState>,
    Auth(admin): Auth,
    Path((group_id, user_id)): Path<(GroupId, UserId)>,
) -> ApiResult<Json<serde_json::Value>> {
    require_admin(&admin)?;

    let group_repo = GroupRepo::new(state.db());

    let group = group_repo.get(group_id).await?;
    if group.org_id != admin.org_id {
        return Err(ApiError::forbidden(
            "cannot modify groups in other organizations",
        ));
    }

    group_repo.remove_member(group_id, user_id).await?;

    tracing::info!(
        admin_id = %admin.user_id,
        group_id = %group_id,
        user_id = %user_id,
        "member removed from group"
    );

    Ok(Json(serde_json::json!({ "message": "member removed" })))
}

// ============================================================================
// Role Management
// ============================================================================

#[derive(Debug, Serialize)]
pub struct RoleInfo {
    pub name: String,
    pub description: String,
    pub permissions: Vec<String>,
}

#[instrument(skip(_state))]
async fn list_roles(
    State(_state): State<AppState>,
    Auth(admin): Auth,
) -> ApiResult<Json<Vec<RoleInfo>>> {
    require_admin(&admin)?;

    let roles = vec![
        RoleInfo {
            name: "admin".to_string(),
            description: "Full system access".to_string(),
            permissions: PermissionRole::Admin
                .permissions()
                .iter()
                .map(|s| (*s).to_string())
                .collect(),
        },
        RoleInfo {
            name: "auditor".to_string(),
            description: "Read-only access to all resources and audit logs".to_string(),
            permissions: PermissionRole::Auditor
                .permissions()
                .iter()
                .map(|s| (*s).to_string())
                .collect(),
        },
        RoleInfo {
            name: "security_lead".to_string(),
            description: "User management, token revocation, and audit logs".to_string(),
            permissions: PermissionRole::SecurityLead
                .permissions()
                .iter()
                .map(|s| (*s).to_string())
                .collect(),
        },
        RoleInfo {
            name: "user".to_string(),
            description: "Standard read/write for assigned projects".to_string(),
            permissions: PermissionRole::User
                .permissions()
                .iter()
                .map(|s| (*s).to_string())
                .collect(),
        },
        RoleInfo {
            name: "security_auditor".to_string(),
            description: "Org-wide blast-radius search and security reads".to_string(),
            permissions: PermissionRole::SecurityAuditor
                .permissions()
                .iter()
                .map(|s| (*s).to_string())
                .collect(),
        },
    ];

    Ok(Json(roles))
}

#[derive(Debug, Serialize)]
pub struct UserRoleResponse {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub granted_by: Option<String>,
    pub granted_at: String,
}

impl From<&UserRole> for UserRoleResponse {
    fn from(r: &UserRole) -> Self {
        Self {
            role: format!("{:?}", r.role).to_lowercase(),
            granted_by: r.granted_by.map(|id| id.to_string()),
            granted_at: r.granted_at.to_rfc3339(),
        }
    }
}

#[instrument(skip(state))]
async fn get_user_roles(
    State(state): State<AppState>,
    Auth(admin): Auth,
    Path(user_id): Path<UserId>,
) -> ApiResult<Json<Vec<UserRoleResponse>>> {
    require_admin(&admin)?;

    let user_repo = UserRepo::new(state.db());
    let user = user_repo.get(user_id).await?;
    if user.org_id != admin.org_id {
        return Err(ApiError::forbidden(
            "cannot access users in other organizations",
        ));
    }

    let role_repo = RoleRepo::new(state.db());
    let roles = role_repo.get_user_roles(user_id).await?;

    Ok(Json(roles.iter().map(UserRoleResponse::from).collect()))
}

#[derive(Debug, Deserialize)]
pub struct AssignRoleRequest {
    pub role: String,
}

#[instrument(skip(state, req))]
async fn assign_role(
    State(state): State<AppState>,
    Auth(admin): Auth,
    Path(user_id): Path<UserId>,
    Json(req): Json<AssignRoleRequest>,
) -> ApiResult<Json<UserRoleResponse>> {
    require_admin(&admin)?;

    let user_repo = UserRepo::new(state.db());
    let user = user_repo.get(user_id).await?;
    if user.org_id != admin.org_id {
        return Err(ApiError::forbidden(
            "cannot modify users in other organizations",
        ));
    }

    let role = match req.role.to_lowercase().as_str() {
        "admin" => PermissionRole::Admin,
        "auditor" => PermissionRole::Auditor,
        "security_lead" => PermissionRole::SecurityLead,
        "security_auditor" => PermissionRole::SecurityAuditor,
        "user" => PermissionRole::User,
        _ => return Err(ApiError::bad_request("invalid role")),
    };

    let role_repo = RoleRepo::new(state.db());
    let user_role = role_repo.assign(user_id, role, Some(admin.user_id)).await?;

    tracing::info!(
        admin_id = %admin.user_id,
        target_user_id = %user_id,
        role = ?role,
        "role assigned"
    );

    Ok(Json(UserRoleResponse::from(&user_role)))
}

#[instrument(skip(state))]
async fn revoke_role(
    State(state): State<AppState>,
    Auth(admin): Auth,
    Path((user_id, role_str)): Path<(UserId, String)>,
) -> ApiResult<Json<serde_json::Value>> {
    require_admin(&admin)?;

    let user_repo = UserRepo::new(state.db());
    let user = user_repo.get(user_id).await?;
    if user.org_id != admin.org_id {
        return Err(ApiError::forbidden(
            "cannot modify users in other organizations",
        ));
    }

    let role = match role_str.to_lowercase().as_str() {
        "admin" => PermissionRole::Admin,
        "auditor" => PermissionRole::Auditor,
        "security_lead" => PermissionRole::SecurityLead,
        "security_auditor" => PermissionRole::SecurityAuditor,
        "user" => PermissionRole::User,
        _ => return Err(ApiError::bad_request("invalid role")),
    };

    // Prevent admin from removing their own admin role
    if user_id == admin.user_id && role == PermissionRole::Admin {
        return Err(ApiError::bad_request("cannot revoke your own admin role"));
    }

    let role_repo = RoleRepo::new(state.db());
    role_repo.revoke(user_id, role).await?;

    tracing::info!(
        admin_id = %admin.user_id,
        target_user_id = %user_id,
        role = ?role,
        "role revoked"
    );

    Ok(Json(serde_json::json!({ "message": "role revoked" })))
}

// ============================================================================
// Project Admin Operations
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct ScheduleDeletionRequest {
    #[serde(default = "default_retention_days")]
    pub retention_days: i64,
}

fn default_retention_days() -> i64 {
    7
}

#[instrument(skip(state, req))]
async fn schedule_project_deletion(
    State(state): State<AppState>,
    Auth(admin): Auth,
    Path(project_id): Path<ProjectId>,
    Json(req): Json<ScheduleDeletionRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    require_admin(&admin)?;

    let repo = ProjectRepo::new(state.db());
    let project = repo.get(project_id).await?;

    if project.org_id != admin.org_id {
        return Err(ApiError::forbidden(
            "cannot delete projects in other organizations",
        ));
    }

    let project = repo
        .schedule_deletion(project_id, req.retention_days)
        .await?;

    tracing::info!(
        admin_id = %admin.user_id,
        project_id = %project_id,
        scheduled_for = ?project.scheduled_deletion_at,
        "project deletion scheduled"
    );

    Ok(Json(serde_json::json!({
        "message": "project deletion scheduled",
        "scheduled_deletion_at": project.scheduled_deletion_at
    })))
}

#[instrument(skip(state))]
async fn cancel_project_deletion(
    State(state): State<AppState>,
    Auth(admin): Auth,
    Path(project_id): Path<ProjectId>,
) -> ApiResult<Json<serde_json::Value>> {
    require_admin(&admin)?;

    let repo = ProjectRepo::new(state.db());
    let project = repo.get(project_id).await?;

    if project.org_id != admin.org_id {
        return Err(ApiError::forbidden(
            "cannot modify projects in other organizations",
        ));
    }

    repo.cancel_deletion(project_id).await?;

    tracing::info!(
        admin_id = %admin.user_id,
        project_id = %project_id,
        "project deletion cancelled"
    );

    Ok(Json(
        serde_json::json!({ "message": "project deletion cancelled" }),
    ))
}

#[instrument(skip(state))]
async fn force_delete_project(
    State(state): State<AppState>,
    Auth(admin): Auth,
    Path(project_id): Path<ProjectId>,
) -> ApiResult<Json<serde_json::Value>> {
    require_admin(&admin)?;

    let repo = ProjectRepo::new(state.db());
    let project = repo.get_including_deleted(project_id).await?;

    if project.org_id != admin.org_id {
        return Err(ApiError::forbidden(
            "cannot delete projects in other organizations",
        ));
    }

    if project.archived_at.is_none() {
        return Err(ApiError::bad_request(
            "project must be archived before permanent delete (use Admin → Archive)",
        ));
    }

    repo.permanent_delete(project_id).await?;

    tracing::warn!(
        admin_id = %admin.user_id,
        project_id = %project_id,
        project_name = %project.name,
        "project permanently deleted"
    );

    Ok(Json(
        serde_json::json!({ "message": "project permanently deleted" }),
    ))
}

// ============================================================================
// Auth Provider Management
// ============================================================================

/// List all auth providers for the organization.
#[instrument(skip(state))]
async fn list_auth_providers(
    State(state): State<AppState>,
    Auth(admin): Auth,
) -> ApiResult<Json<Vec<AuthProviderResponse>>> {
    require_admin(&admin)?;

    let repo = AuthProviderRepo::new(state.db());
    let providers = repo.list(admin.org_id).await?;

    Ok(Json(
        providers
            .into_iter()
            .map(AuthProviderResponse::from)
            .collect(),
    ))
}

/// Auth provider response (without secrets).
#[derive(Debug, Serialize)]
pub struct AuthProviderResponse {
    pub id: String,
    pub name: String,
    pub provider_type: String,
    pub client_id: String,
    pub issuer_url: Option<String>,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

impl From<AuthProvider> for AuthProviderResponse {
    fn from(p: AuthProvider) -> Self {
        Self {
            id: p.id.to_string(),
            name: p.name,
            provider_type: p.provider_type,
            client_id: p.client_id,
            issuer_url: p.issuer_url,
            enabled: p.enabled,
            created_at: p.created_at.to_rfc3339(),
            updated_at: p.updated_at.to_rfc3339(),
        }
    }
}

/// Create auth provider request.
#[derive(Debug, Deserialize)]
pub struct CreateAuthProviderRequest {
    pub name: String,
    pub provider_type: String,
    pub client_id: String,
    pub client_secret: String,
    pub issuer_url: Option<String>,
}

/// Create a new auth provider.
#[instrument(skip(state, req))]
async fn create_auth_provider(
    State(state): State<AppState>,
    Auth(admin): Auth,
    Json(req): Json<CreateAuthProviderRequest>,
) -> ApiResult<Json<AuthProviderResponse>> {
    require_admin(&admin)?;

    // Validate provider type
    if req.provider_type != "oidc" && req.provider_type != "github" {
        return Err(ApiError::bad_request(
            "provider_type must be 'oidc' or 'github'",
        ));
    }

    // OIDC requires issuer_url
    if req.provider_type == "oidc" && req.issuer_url.is_none() {
        return Err(ApiError::bad_request(
            "issuer_url is required for OIDC providers",
        ));
    }

    let repo = AuthProviderRepo::new(state.db());
    let provider = repo
        .create(
            admin.org_id,
            &CreateAuthProvider {
                provider_type: req.provider_type.clone(),
                name: req.name.clone(),
                client_id: req.client_id.clone(),
                client_secret: req.client_secret.clone(),
                issuer_url: req.issuer_url.clone(),
                config: serde_json::json!({}),
            },
            &req.client_secret, // In production, encrypt this
        )
        .await?;

    tracing::info!(
        admin_id = %admin.user_id,
        provider_id = %provider.id,
        provider_name = %provider.name,
        "auth provider created"
    );

    Ok(Json(AuthProviderResponse::from(provider)))
}

/// Get a single auth provider.
#[instrument(skip(state))]
async fn get_auth_provider(
    State(state): State<AppState>,
    Auth(admin): Auth,
    Path(provider_id): Path<AuthProviderId>,
) -> ApiResult<Json<AuthProviderResponse>> {
    require_admin(&admin)?;

    let repo = AuthProviderRepo::new(state.db());
    let provider = repo.get(provider_id).await?;

    if provider.org_id != admin.org_id {
        return Err(ApiError::forbidden(
            "cannot access providers in other organizations",
        ));
    }

    Ok(Json(AuthProviderResponse::from(provider)))
}

/// Update auth provider request.
#[derive(Debug, Deserialize)]
pub struct UpdateAuthProviderRequest {
    pub name: Option<String>,
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    pub issuer_url: Option<String>,
}

/// Update an auth provider.
#[instrument(skip(state, req))]
async fn update_auth_provider(
    State(state): State<AppState>,
    Auth(admin): Auth,
    Path(provider_id): Path<AuthProviderId>,
    Json(req): Json<UpdateAuthProviderRequest>,
) -> ApiResult<Json<AuthProviderResponse>> {
    require_admin(&admin)?;

    let repo = AuthProviderRepo::new(state.db());
    let existing = repo.get(provider_id).await?;

    if existing.org_id != admin.org_id {
        return Err(ApiError::forbidden(
            "cannot modify providers in other organizations",
        ));
    }

    let provider = repo
        .update(
            provider_id,
            &UpdateAuthProvider {
                name: req.name,
                client_id: req.client_id,
                client_secret: req.client_secret.clone(),
                issuer_url: req.issuer_url,
                config: None,
            },
            req.client_secret.as_deref(),
        )
        .await?;

    tracing::info!(
        admin_id = %admin.user_id,
        provider_id = %provider_id,
        "auth provider updated"
    );

    Ok(Json(AuthProviderResponse::from(provider)))
}

/// Enable an auth provider.
#[instrument(skip(state))]
async fn enable_auth_provider(
    State(state): State<AppState>,
    Auth(admin): Auth,
    Path(provider_id): Path<AuthProviderId>,
) -> ApiResult<Json<AuthProviderResponse>> {
    require_admin(&admin)?;

    let repo = AuthProviderRepo::new(state.db());
    let existing = repo.get(provider_id).await?;

    if existing.org_id != admin.org_id {
        return Err(ApiError::forbidden(
            "cannot modify providers in other organizations",
        ));
    }

    let provider = repo.enable(provider_id).await?;

    tracing::info!(
        admin_id = %admin.user_id,
        provider_id = %provider_id,
        "auth provider enabled"
    );

    Ok(Json(AuthProviderResponse::from(provider)))
}

/// Disable an auth provider.
#[instrument(skip(state))]
async fn disable_auth_provider(
    State(state): State<AppState>,
    Auth(admin): Auth,
    Path(provider_id): Path<AuthProviderId>,
) -> ApiResult<Json<AuthProviderResponse>> {
    require_admin(&admin)?;

    let repo = AuthProviderRepo::new(state.db());
    let existing = repo.get(provider_id).await?;

    if existing.org_id != admin.org_id {
        return Err(ApiError::forbidden(
            "cannot modify providers in other organizations",
        ));
    }

    let provider = repo.disable(provider_id).await?;

    tracing::info!(
        admin_id = %admin.user_id,
        provider_id = %provider_id,
        "auth provider disabled"
    );

    Ok(Json(AuthProviderResponse::from(provider)))
}

/// Delete an auth provider.
#[instrument(skip(state))]
async fn delete_auth_provider(
    State(state): State<AppState>,
    Auth(admin): Auth,
    Path(provider_id): Path<AuthProviderId>,
) -> ApiResult<Json<serde_json::Value>> {
    require_admin(&admin)?;

    let repo = AuthProviderRepo::new(state.db());
    let existing = repo.get(provider_id).await?;

    if existing.org_id != admin.org_id {
        return Err(ApiError::forbidden(
            "cannot delete providers in other organizations",
        ));
    }

    repo.delete(provider_id).await?;

    tracing::info!(
        admin_id = %admin.user_id,
        provider_id = %provider_id,
        provider_name = %existing.name,
        "auth provider deleted"
    );

    Ok(Json(
        serde_json::json!({ "message": "auth provider deleted" }),
    ))
}

// ============================================================================
// OIDC Group Mapping Handlers
// ============================================================================

#[derive(Debug, Serialize)]
pub struct GroupMappingResponse {
    pub id: String,
    pub provider_id: String,
    pub oidc_group_claim: String,
    pub meticulous_group_id: String,
    pub role: String,
    pub created_at: String,
}

impl From<OidcGroupMapping> for GroupMappingResponse {
    fn from(m: OidcGroupMapping) -> Self {
        Self {
            id: m.id.to_string(),
            provider_id: m.provider_id.to_string(),
            oidc_group_claim: m.oidc_group_claim,
            meticulous_group_id: m.meticulous_group_id.to_string(),
            role: format!("{:?}", m.role).to_lowercase(),
            created_at: m.created_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateGroupMappingRequest {
    pub oidc_group_claim: String,
    pub meticulous_group_id: GroupId,
    #[serde(default)]
    pub role: Option<GroupRole>,
}

/// List OIDC group mappings for a provider.
#[instrument(skip(state))]
async fn list_group_mappings(
    State(state): State<AppState>,
    Auth(admin): Auth,
    Path(provider_id): Path<AuthProviderId>,
) -> ApiResult<Json<Vec<GroupMappingResponse>>> {
    require_admin(&admin)?;

    let repo = AuthProviderRepo::new(state.db());
    let provider = repo.get(provider_id).await?;

    if provider.org_id != admin.org_id {
        return Err(ApiError::forbidden(
            "cannot access providers in other organizations",
        ));
    }

    let mappings = repo.list_group_mappings(provider_id).await?;
    let responses: Vec<GroupMappingResponse> = mappings
        .into_iter()
        .map(GroupMappingResponse::from)
        .collect();

    Ok(Json(responses))
}

/// Create an OIDC group mapping.
#[instrument(skip(state))]
async fn create_group_mapping(
    State(state): State<AppState>,
    Auth(admin): Auth,
    Path(provider_id): Path<AuthProviderId>,
    Json(req): Json<CreateGroupMappingRequest>,
) -> ApiResult<Json<GroupMappingResponse>> {
    require_admin(&admin)?;

    let auth_repo = AuthProviderRepo::new(state.db());
    let provider = auth_repo.get(provider_id).await?;

    if provider.org_id != admin.org_id {
        return Err(ApiError::forbidden(
            "cannot modify providers in other organizations",
        ));
    }

    // Verify the target group exists and belongs to the same org
    let group_repo = GroupRepo::new(state.db());
    let group = group_repo.get(req.meticulous_group_id).await?;

    if group.org_id != admin.org_id {
        return Err(ApiError::forbidden(
            "cannot map to groups in other organizations",
        ));
    }

    let input = CreateOidcGroupMapping {
        oidc_group_claim: req.oidc_group_claim.clone(),
        meticulous_group_id: req.meticulous_group_id,
        role: req.role.unwrap_or(GroupRole::Member),
    };

    let mapping = auth_repo.create_group_mapping(provider_id, &input).await?;

    tracing::info!(
        admin_id = %admin.user_id,
        provider_id = %provider_id,
        oidc_group = %req.oidc_group_claim,
        meticulous_group = %req.meticulous_group_id,
        "OIDC group mapping created"
    );

    Ok(Json(GroupMappingResponse::from(mapping)))
}

/// Delete an OIDC group mapping.
#[instrument(skip(state))]
async fn delete_group_mapping(
    State(state): State<AppState>,
    Auth(admin): Auth,
    Path((provider_id, mapping_id)): Path<(AuthProviderId, OidcGroupMappingId)>,
) -> ApiResult<Json<serde_json::Value>> {
    require_admin(&admin)?;

    let repo = AuthProviderRepo::new(state.db());
    let provider = repo.get(provider_id).await?;

    if provider.org_id != admin.org_id {
        return Err(ApiError::forbidden(
            "cannot modify providers in other organizations",
        ));
    }

    // Verify mapping exists and belongs to this provider
    let mapping = repo.get_group_mapping(mapping_id).await?;
    if mapping.provider_id != provider_id {
        return Err(ApiError::not_found("group mapping not found"));
    }

    repo.delete_group_mapping(mapping_id).await?;

    tracing::info!(
        admin_id = %admin.user_id,
        provider_id = %provider_id,
        mapping_id = %mapping_id,
        "OIDC group mapping deleted"
    );

    Ok(Json(
        serde_json::json!({ "message": "group mapping deleted" }),
    ))
}

// ============================================================================
// Join Token Management
// ============================================================================

#[derive(Debug, Serialize)]
pub struct JoinTokenResponse {
    pub id: String,
    pub prefix: String,
    pub description: String,
    pub scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope_id: Option<String>,
    pub max_uses: i32,
    pub current_uses: i32,
    pub labels: Vec<String>,
    pub pool_tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    pub revoked: bool,
    pub created_by: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by_name: Option<String>,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consumed_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consumed_by_agent_id: Option<String>,
    /// Present on single-token GET; omitted or empty on list to keep payloads small.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub description_history: Vec<JoinTokenDescriptionHistoryEntry>,
    #[serde(default)]
    pub agents: Vec<JoinTokenAgentInfo>,
}

/// One edit in the join token description timeline.
#[derive(Debug, Serialize)]
pub struct JoinTokenDescriptionHistoryEntry {
    pub description: String,
    pub changed_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changed_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changed_by_name: Option<String>,
}

/// Summary of an agent that registered using a join token.
#[derive(Debug, Serialize)]
pub struct JoinTokenAgentInfo {
    pub id: String,
    pub name: String,
    pub status: String,
    pub registered_at: String,
}

fn join_token_scope_str(scope: JoinTokenScope) -> &'static str {
    match scope {
        JoinTokenScope::Platform => "platform",
        JoinTokenScope::Tenant => "tenant",
        JoinTokenScope::Project => "project",
        JoinTokenScope::Pipeline => "pipeline",
    }
}

impl JoinTokenResponse {
    fn from_token(t: &JoinToken) -> Self {
        Self {
            id: t.id.to_string(),
            prefix: format!("met_join_{}...", &t.token_hash[..8.min(t.token_hash.len())]),
            description: t.description.clone(),
            scope: join_token_scope_str(t.scope).to_string(),
            scope_id: t.scope_id.map(|id| id.to_string()),
            max_uses: t.max_uses,
            current_uses: t.current_uses,
            labels: t.labels.clone(),
            pool_tags: t.pool_tags.clone(),
            expires_at: t.expires_at.map(|dt| dt.to_rfc3339()),
            revoked: t.revoked,
            created_by: t.created_by.to_string(),
            created_by_name: None,
            created_at: t.created_at.to_rfc3339(),
            consumed_at: t.consumed_at.map(|dt| dt.to_rfc3339()),
            consumed_by_agent_id: t.consumed_by_agent_id.map(|a| a.to_string()),
            description_history: Vec::new(),
            agents: Vec::new(),
        }
    }
}

impl From<&JoinToken> for JoinTokenResponse {
    fn from(t: &JoinToken) -> Self {
        Self::from_token(t)
    }
}

#[derive(Debug, Deserialize)]
pub struct UpdateJoinTokenRequest {
    /// New description (required, non-empty when trimmed).
    pub description: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateJoinTokenRequest {
    /// Human-readable description (required, non-empty when trimmed).
    #[serde(default)]
    pub description: Option<String>,
    /// Token scope: platform, tenant, project, or pipeline
    #[serde(default)]
    pub scope: Option<String>,
    /// Scope ID for project/pipeline scope
    #[serde(default)]
    pub scope_id: Option<uuid::Uuid>,
    /// Ignored — tokens are always single-use (`max_uses = 1`).
    #[serde(default)]
    pub max_uses: Option<i32>,
    /// Labels to apply to agents using this token
    #[serde(default)]
    pub labels: Vec<String>,
    /// Pool tags to apply to agents using this token
    #[serde(default)]
    pub pool_tags: Vec<String>,
    /// Expiration in days (None = never expires)
    #[serde(default)]
    pub expires_in_days: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct CreateJoinTokenResponse {
    pub token: JoinTokenResponse,
    pub plain_token: String,
}

/// Query parameters for listing join tokens (page-based pagination + search).
#[derive(Debug, Deserialize)]
pub struct JoinTokenListQuery {
    /// 1-based page index.
    #[serde(default = "join_token_list_default_page")]
    pub page: u32,
    /// Page size: one of 20, 50, 100, 200 (other values are clamped to the nearest allowed size).
    #[serde(default = "join_token_list_default_limit")]
    pub limit: u32,
    /// Case-insensitive search on description or stored token hash (matches visible prefix / hash fragments).
    pub q: Option<String>,
}

fn join_token_list_default_page() -> u32 {
    1
}

fn join_token_list_default_limit() -> u32 {
    20
}

fn normalize_join_token_limit(n: u32) -> u32 {
    match n {
        20 | 50 | 100 | 200 => n,
        0 => 20,
        n if n <= 35 => 20,
        n if n <= 75 => 50,
        n if n <= 150 => 100,
        _ => 200,
    }
}

/// Paginated join token list (tenant-scoped for the admin org).
#[derive(Debug, Serialize)]
pub struct JoinTokenListResponse {
    /// Join tokens for this page.
    pub data: Vec<JoinTokenResponse>,
    pub pagination: JoinTokenListPagination,
}

#[derive(Debug, Serialize)]
pub struct JoinTokenListPagination {
    pub page: u32,
    pub per_page: u32,
    pub total: u64,
    pub has_more: bool,
}

/// List join tokens for the organization with pagination and optional search.
#[instrument(skip(state, query))]
async fn list_join_tokens(
    State(state): State<AppState>,
    Auth(admin): Auth,
    Query(query): Query<JoinTokenListQuery>,
) -> ApiResult<Json<JoinTokenListResponse>> {
    require_admin(&admin)?;

    let page = query.page.max(1);
    let per_page = normalize_join_token_limit(query.limit);
    let offset = i64::from((page - 1).saturating_mul(per_page));
    let limit = i64::from(per_page);

    let search = query.q.as_deref();
    let repo = JoinTokenRepo::new(state.db());
    let total = repo.count_by_org_filtered(admin.org_id, search).await? as u64;
    let tokens = repo
        .list_by_org_filtered(admin.org_id, search, limit, offset)
        .await?;

    let items = enrich_join_tokens(&tokens, state.db()).await?;
    let has_more = offset.saturating_add(items.len() as i64) < total as i64;

    Ok(Json(JoinTokenListResponse {
        data: items,
        pagination: JoinTokenListPagination {
            page,
            per_page,
            total,
            has_more,
        },
    }))
}

/// Enriches join tokens with creator names and associated agents.
pub(crate) async fn enrich_join_tokens(
    tokens: &[JoinToken],
    db: &sqlx::PgPool,
) -> ApiResult<Vec<JoinTokenResponse>> {
    if tokens.is_empty() {
        return Ok(Vec::new());
    }

    let token_ids: Vec<uuid::Uuid> = tokens.iter().map(|t| t.id.as_uuid()).collect();
    let creator_ids: Vec<uuid::Uuid> = tokens.iter().map(|t| t.created_by.as_uuid()).collect();

    #[derive(sqlx::FromRow)]
    struct CreatorRow {
        id: uuid::Uuid,
        display_name: Option<String>,
        username: String,
    }

    let creators: Vec<CreatorRow> =
        sqlx::query_as("SELECT id, display_name, username FROM users WHERE id = ANY($1)")
            .bind(&creator_ids)
            .fetch_all(db)
            .await
            .map_err(met_store::StoreError::from)?;

    let creator_map: std::collections::HashMap<uuid::Uuid, &CreatorRow> =
        creators.iter().map(|c| (c.id, c)).collect();

    #[derive(sqlx::FromRow)]
    struct AgentRow {
        id: uuid::Uuid,
        name: String,
        status: met_core::models::AgentStatus,
        join_token_id: Option<uuid::Uuid>,
        created_at: chrono::DateTime<chrono::Utc>,
    }

    let agents: Vec<AgentRow> = sqlx::query_as(
        "SELECT id, name, status, join_token_id, created_at FROM agents WHERE join_token_id = ANY($1) AND deregistered_at IS NULL",
    )
    .bind(&token_ids)
    .fetch_all(db)
    .await
    .map_err(met_store::StoreError::from)?;

    let mut agent_map: std::collections::HashMap<uuid::Uuid, Vec<JoinTokenAgentInfo>> =
        std::collections::HashMap::new();
    for a in &agents {
        if let Some(jt_id) = a.join_token_id {
            agent_map
                .entry(jt_id)
                .or_default()
                .push(JoinTokenAgentInfo {
                    id: a.id.to_string(),
                    name: a.name.clone(),
                    status: format!("{:?}", a.status).to_lowercase(),
                    registered_at: a.created_at.to_rfc3339(),
                });
        }
    }

    let items = tokens
        .iter()
        .map(|t| {
            let mut resp = JoinTokenResponse::from_token(t);
            if let Some(creator) = creator_map.get(&t.created_by.as_uuid()) {
                resp.created_by_name = Some(
                    creator
                        .display_name
                        .clone()
                        .unwrap_or_else(|| creator.username.clone()),
                );
            }
            if let Some(token_agents) = agent_map.remove(&t.id.as_uuid()) {
                resp.agents = token_agents;
            }
            resp
        })
        .collect();

    Ok(items)
}

/// Get a join token by ID, enriched with creator info and associated agents.
#[instrument(skip(state))]
async fn get_join_token(
    State(state): State<AppState>,
    Auth(admin): Auth,
    Path(token_id): Path<JoinTokenId>,
) -> ApiResult<Json<JoinTokenResponse>> {
    require_admin(&admin)?;

    let repo = JoinTokenRepo::new(state.db());
    let token = repo.get(token_id).await?;

    if token.scope == JoinTokenScope::Tenant {
        if token.scope_id != Some(admin.org_id.as_uuid()) {
            return Err(ApiError::forbidden(
                "cannot access tokens from other organizations",
            ));
        }
    }

    let mut items = enrich_join_tokens(&[token], state.db()).await?;
    let mut resp = items.remove(0);

    let hist = JoinTokenRepo::new(state.db())
        .list_description_history(token_id)
        .await
        .map_err(met_store::StoreError::from)?;
    resp.description_history = join_token_history_to_api_entries(state.db(), hist).await?;

    Ok(Json(resp))
}

/// Create a new join token.
#[instrument(skip(state, req))]
async fn create_join_token(
    State(state): State<AppState>,
    Auth(admin): Auth,
    Json(req): Json<CreateJoinTokenRequest>,
) -> ApiResult<Json<CreateJoinTokenResponse>> {
    require_admin(&admin)?;

    let description = req.description.as_deref().unwrap_or("").trim().to_string();
    if description.is_empty() {
        return Err(ApiError::bad_request("description is required"));
    }

    // Parse scope
    let scope = match req.scope.as_deref().unwrap_or("tenant") {
        "platform" => JoinTokenScope::Platform,
        "tenant" => JoinTokenScope::Tenant,
        "project" => JoinTokenScope::Project,
        "pipeline" => JoinTokenScope::Pipeline,
        _ => {
            return Err(ApiError::bad_request(
                "invalid scope, must be: platform, tenant, project, or pipeline",
            ));
        }
    };

    // Generate the plain token
    let plain_token = generate_join_token();
    let token_hash = hash_token(&plain_token);

    // Calculate expiration
    let expires_at = req
        .expires_in_days
        .map(|days| Utc::now() + Duration::days(days));

    // Determine scope_id based on scope type
    let scope_id = match scope {
        JoinTokenScope::Platform => None,
        JoinTokenScope::Tenant => Some(admin.org_id.as_uuid()),
        JoinTokenScope::Project | JoinTokenScope::Pipeline => {
            req.scope_id.ok_or_else(|| {
                ApiError::bad_request("scope_id is required for project and pipeline scopes")
            })?;
            req.scope_id
        }
    };

    let org_id_col = match scope {
        JoinTokenScope::Tenant => Some(admin.org_id),
        _ => None,
    };

    let now = Utc::now();
    let token = JoinToken {
        id: JoinTokenId::new(),
        token_hash,
        scope,
        scope_id,
        description,
        org_id: org_id_col,
        max_uses: 1,
        current_uses: 0,
        labels: req.labels,
        pool_tags: req.pool_tags,
        expires_at,
        revoked: false,
        created_by: admin.user_id,
        created_at: now,
        updated_at: now,
        consumed_by_agent_id: None,
        consumed_at: None,
    };

    let repo = JoinTokenRepo::new(state.db());
    let created = repo.create(&token).await?;

    repo.insert_description_history(
        created.id,
        &created.description,
        created.created_by,
        created.created_at,
    )
    .await?;

    tracing::info!(
        admin_id = %admin.user_id,
        token_id = %created.id,
        scope = ?scope,
        "join token created"
    );

    Ok(Json(CreateJoinTokenResponse {
        token: JoinTokenResponse::from(&created),
        plain_token,
    }))
}

async fn join_token_history_to_api_entries(
    db: &sqlx::PgPool,
    rows: Vec<JoinTokenDescriptionHistory>,
) -> ApiResult<Vec<JoinTokenDescriptionHistoryEntry>> {
    if rows.is_empty() {
        return Ok(Vec::new());
    }

    let user_ids: Vec<uuid::Uuid> = rows.iter().filter_map(|r| r.changed_by).collect();
    let mut name_map: std::collections::HashMap<uuid::Uuid, (String, Option<String>)> =
        std::collections::HashMap::new();
    if !user_ids.is_empty() {
        #[derive(sqlx::FromRow)]
        struct UserNameRow {
            id: uuid::Uuid,
            username: String,
            display_name: Option<String>,
        }
        let users: Vec<UserNameRow> =
            sqlx::query_as("SELECT id, username, display_name FROM users WHERE id = ANY($1)")
                .bind(&user_ids)
                .fetch_all(db)
                .await
                .map_err(met_store::StoreError::from)?;
        for u in users {
            name_map.insert(u.id, (u.username, u.display_name));
        }
    }

    Ok(rows
        .into_iter()
        .map(|r| {
            let changed_by_name = r.changed_by.and_then(|uid| {
                name_map
                    .get(&uid)
                    .map(|(username, display)| display.clone().unwrap_or_else(|| username.clone()))
            });
            JoinTokenDescriptionHistoryEntry {
                description: r.description,
                changed_at: r.changed_at.to_rfc3339(),
                changed_by: r.changed_by.map(|id| id.to_string()),
                changed_by_name,
            }
        })
        .collect())
}

/// Update join token description (append-only history).
#[instrument(skip(state, req))]
async fn update_join_token(
    State(state): State<AppState>,
    Auth(admin): Auth,
    Path(token_id): Path<JoinTokenId>,
    Json(req): Json<UpdateJoinTokenRequest>,
) -> ApiResult<Json<JoinTokenResponse>> {
    require_admin(&admin)?;

    let description = req.description.trim().to_string();
    if description.is_empty() {
        return Err(ApiError::bad_request("description is required"));
    }

    let repo = JoinTokenRepo::new(state.db());
    let existing = repo.get(token_id).await?;

    if existing.scope == JoinTokenScope::Tenant {
        if existing.scope_id != Some(admin.org_id.as_uuid()) {
            return Err(ApiError::forbidden(
                "cannot update tokens from other organizations",
            ));
        }
    }

    let updated = repo
        .update_description(token_id, &description, admin.user_id)
        .await
        .map_err(met_store::StoreError::from)?;

    let mut items = enrich_join_tokens(&[updated], state.db()).await?;
    let mut resp = items.remove(0);
    let hist = JoinTokenRepo::new(state.db())
        .list_description_history(token_id)
        .await
        .map_err(met_store::StoreError::from)?;
    resp.description_history = join_token_history_to_api_entries(state.db(), hist).await?;

    tracing::info!(
        admin_id = %admin.user_id,
        token_id = %token_id,
        "join token description updated"
    );

    Ok(Json(resp))
}

/// Revoke a join token.
#[instrument(skip(state))]
async fn revoke_join_token(
    State(state): State<AppState>,
    Auth(admin): Auth,
    Path(token_id): Path<JoinTokenId>,
) -> ApiResult<Json<serde_json::Value>> {
    require_admin(&admin)?;

    let repo = JoinTokenRepo::new(state.db());
    let token = repo.get(token_id).await?;

    // Verify the token belongs to this org (tenant scope check)
    if token.scope == JoinTokenScope::Tenant {
        if token.scope_id != Some(admin.org_id.as_uuid()) {
            return Err(ApiError::forbidden(
                "cannot revoke tokens from other organizations",
            ));
        }
    }

    repo.revoke(token_id).await?;

    tracing::info!(
        admin_id = %admin.user_id,
        token_id = %token_id,
        "join token revoked"
    );

    Ok(Json(serde_json::json!({ "message": "join token revoked" })))
}

/// Permanently delete a join token (removes DB row; agents lose join_token link).
#[instrument(skip(state))]
async fn delete_join_token(
    State(state): State<AppState>,
    Auth(admin): Auth,
    Path(token_id): Path<JoinTokenId>,
) -> ApiResult<Json<serde_json::Value>> {
    require_admin(&admin)?;

    let repo = JoinTokenRepo::new(state.db());
    let token = repo.get(token_id).await?;

    if token.scope == JoinTokenScope::Tenant {
        if token.scope_id != Some(admin.org_id.as_uuid()) {
            return Err(ApiError::forbidden(
                "cannot delete tokens from other organizations",
            ));
        }
    }

    repo.delete_by_id(token_id).await?;

    tracing::info!(
        admin_id = %admin.user_id,
        token_id = %token_id,
        "join token deleted"
    );

    Ok(Json(serde_json::json!({ "message": "join token deleted" })))
}
