//! Admin API routes.
//!
//! Provides endpoints for:
//! - User management (list, update, lock/unlock, delete)
//! - Group management (CRUD, membership)
//! - Role management (assign/revoke)
//! - Project admin operations (schedule deletion, force delete)

use axum::{
    extract::{Path, State},
    routing::{delete, get, patch, post},
    Json, Router,
};
use met_core::ids::{GroupId, ProjectId, UserId};
use met_core::models::{
    CreateGroup, Group, GroupMembership, GroupRole, PermissionRole, User, UserRole,
};
use met_store::repos::{GroupRepo, ProjectRepo, RoleRepo, UserRepo};
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::{
    error::{ApiError, ApiResult},
    extractors::{Auth, Pagination, PaginatedResponse},
    state::AppState,
};

/// Build the admin router.
pub fn router() -> Router<AppState> {
    Router::new()
        // User management
        .route("/admin/users", get(list_users))
        .route("/admin/users/{id}", get(get_user).patch(update_user))
        .route("/admin/users/{id}/lock", post(lock_user))
        .route("/admin/users/{id}/unlock", post(unlock_user))
        .route("/admin/users/{id}/delete", post(delete_user))
        // Group management
        .route("/admin/groups", get(list_groups).post(create_group))
        .route(
            "/admin/groups/{id}",
            get(get_group).patch(update_group).delete(delete_group),
        )
        .route("/admin/groups/{id}/members", get(list_group_members).post(add_group_member))
        .route("/admin/groups/{id}/members/{user_id}", delete(remove_group_member).patch(update_group_member))
        // Role management
        .route("/admin/roles", get(list_roles))
        .route("/admin/users/{id}/roles", get(get_user_roles).post(assign_role))
        .route("/admin/users/{id}/roles/{role}", delete(revoke_role))
        // Project admin operations
        .route("/admin/projects/{id}/schedule-deletion", post(schedule_project_deletion))
        .route("/admin/projects/{id}/cancel-deletion", post(cancel_project_deletion))
        .route("/admin/projects/{id}/force-delete", post(force_delete_project))
}

// ============================================================================
// Middleware / Guards
// ============================================================================

fn require_admin(user: &crate::extractors::CurrentUser) -> ApiResult<()> {
    if !user.has_permission("*") {
        return Err(ApiError::forbidden("admin access required"));
    }
    Ok(())
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
    let users = repo
        .list(admin.org_id, pagination.sql_limit(), 0)
        .await?;

    let response = PaginatedResponse::new(
        users.iter().map(UserResponse::from).collect(),
        pagination.limit,
        |u| u.id.clone(),
    );

    Ok(Json(response))
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
        return Err(ApiError::forbidden("cannot access users in other organizations"));
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
        return Err(ApiError::forbidden("cannot modify users in other organizations"));
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
        return Err(ApiError::forbidden("cannot lock users in other organizations"));
    }

    if user_id == admin.user_id {
        return Err(ApiError::bad_request("cannot lock your own account"));
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
        return Err(ApiError::forbidden("cannot unlock users in other organizations"));
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
        return Err(ApiError::forbidden("cannot delete users in other organizations"));
    }

    if user_id == admin.user_id {
        return Err(ApiError::bad_request("cannot delete your own account"));
    }

    sqlx::query(
        r#"
        UPDATE users SET deleted_at = NOW(), updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(user_id.as_uuid())
    .execute(state.db())
    .await
    .map_err(met_store::StoreError::from)?;

    tracing::info!(admin_id = %admin.user_id, target_user_id = %user_id, "user deleted");

    Ok(Json(serde_json::json!({ "message": "user deleted" })))
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
    let groups = repo
        .list(admin.org_id, pagination.sql_limit(), 0)
        .await?;

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
        return Err(ApiError::forbidden("cannot access groups in other organizations"));
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
        return Err(ApiError::forbidden("cannot modify groups in other organizations"));
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
        return Err(ApiError::forbidden("cannot delete groups in other organizations"));
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
        return Err(ApiError::forbidden("cannot access groups in other organizations"));
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
        return Err(ApiError::forbidden("cannot modify groups in other organizations"));
    }

    let user = user_repo.get(req.user_id).await?;
    if user.org_id != admin.org_id {
        return Err(ApiError::forbidden("cannot add users from other organizations"));
    }

    let membership = group_repo.add_member(group_id, req.user_id, req.role).await?;

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
        return Err(ApiError::forbidden("cannot modify groups in other organizations"));
    }

    let user = user_repo.get(user_id).await?;
    let membership = group_repo.update_member_role(group_id, user_id, req.role).await?;

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
        return Err(ApiError::forbidden("cannot modify groups in other organizations"));
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
            permissions: PermissionRole::Admin.permissions().iter().map(|s| (*s).to_string()).collect(),
        },
        RoleInfo {
            name: "auditor".to_string(),
            description: "Read-only access to all resources and audit logs".to_string(),
            permissions: PermissionRole::Auditor.permissions().iter().map(|s| (*s).to_string()).collect(),
        },
        RoleInfo {
            name: "security_lead".to_string(),
            description: "User management, token revocation, and audit logs".to_string(),
            permissions: PermissionRole::SecurityLead.permissions().iter().map(|s| (*s).to_string()).collect(),
        },
        RoleInfo {
            name: "user".to_string(),
            description: "Standard read/write for assigned projects".to_string(),
            permissions: PermissionRole::User.permissions().iter().map(|s| (*s).to_string()).collect(),
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
        return Err(ApiError::forbidden("cannot access users in other organizations"));
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
        return Err(ApiError::forbidden("cannot modify users in other organizations"));
    }

    let role = match req.role.to_lowercase().as_str() {
        "admin" => PermissionRole::Admin,
        "auditor" => PermissionRole::Auditor,
        "security_lead" => PermissionRole::SecurityLead,
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
        return Err(ApiError::forbidden("cannot modify users in other organizations"));
    }

    let role = match role_str.to_lowercase().as_str() {
        "admin" => PermissionRole::Admin,
        "auditor" => PermissionRole::Auditor,
        "security_lead" => PermissionRole::SecurityLead,
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
        return Err(ApiError::forbidden("cannot delete projects in other organizations"));
    }

    let project = repo.schedule_deletion(project_id, req.retention_days).await?;

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
        return Err(ApiError::forbidden("cannot modify projects in other organizations"));
    }

    repo.cancel_deletion(project_id).await?;

    tracing::info!(
        admin_id = %admin.user_id,
        project_id = %project_id,
        "project deletion cancelled"
    );

    Ok(Json(serde_json::json!({ "message": "project deletion cancelled" })))
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
        return Err(ApiError::forbidden("cannot delete projects in other organizations"));
    }

    repo.permanent_delete(project_id).await?;

    tracing::warn!(
        admin_id = %admin.user_id,
        project_id = %project_id,
        project_name = %project.name,
        "project permanently deleted"
    );

    Ok(Json(serde_json::json!({ "message": "project permanently deleted" })))
}
