//! Resolve per-project and per-pipeline roles with visibility awareness.
//!
//! Combines `project_members` / `pipeline_members` with the three-tier
//! `ResourceVisibility` model (ADR-021).

use met_core::ids::{OrganizationId, PipelineId, ProjectId};
use met_core::models::installation_has_read_access;
use met_store::repos::{
    PipelineAccessRepo, PipelineRepo, PipelineRole, PlatformSettingsRepo, ProjectAccessRepo,
    ProjectRepo, ProjectRole,
};

use crate::auth::AppInstallationPrincipal;
use crate::error::{ApiError, ApiResult};
use crate::extractors::CurrentUser;

/// Authenticated human/API token or Meticulous App installation (read-scoped API).
#[derive(Debug, Clone)]
pub enum SessionOrApp {
    User(CurrentUser),
    App(AppInstallationPrincipal),
}

#[must_use]
pub fn caller_org_id(caller: &SessionOrApp) -> OrganizationId {
    match caller {
        SessionOrApp::User(u) => u.org_id,
        SessionOrApp::App(p) => p.org_id,
    }
}

/// Ensures an installation may read `project_id` (matches scope and has `read` or `*` permission).
pub fn ensure_app_can_read_project(
    principal: &AppInstallationPrincipal,
    project_id: ProjectId,
) -> ApiResult<()> {
    if principal.project_id != project_id {
        return Err(ApiError::forbidden("no access to this project"));
    }
    if !installation_has_read_access(&principal.permissions) {
        return Err(ApiError::forbidden(
            "installation does not grant read permission",
        ));
    }
    Ok(())
}

/// Whether the user holds the `super_admin` permission role (unrestricted break-glass).
pub fn is_super_admin(user: &CurrentUser) -> bool {
    user.has_permission("*")
}

/// Whether the user holds the `admin` permission role (metadata-only).
///
/// Returns `true` when the user has admin-level metadata permissions but NOT
/// the wildcard `*` (which is `super_admin`).
pub fn is_platform_admin(user: &CurrentUser) -> bool {
    user.has_permission("admin:metadata") && !user.has_permission("*")
}

/// Effective role on a project. `super_admin` → `Admin`; `platform_admin` → metadata-only
/// (callers must enforce content restrictions separately).
///
/// Enforces API-token project scope via [`CurrentUser::can_access_project`].
pub async fn effective_project_role(
    pool: &sqlx::PgPool,
    user: &CurrentUser,
    project_id: ProjectId,
) -> ApiResult<ProjectRole> {
    if !user.can_access_project(project_id) {
        return Err(ApiError::forbidden("no access to this project"));
    }
    if is_super_admin(user) {
        return Ok(ProjectRole::Admin);
    }
    let project = ProjectRepo::new(pool).get(project_id).await?;
    let access = ProjectAccessRepo::new(pool);
    let role = access
        .effective_role_for_user_with_visibility(project_id, Some(user.user_id), project.visibility)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    match role {
        Some(r) => Ok(r),
        None => Err(ApiError::forbidden("no access to this project")),
    }
}

/// Like [`effective_project_role`], but requires the project to exist in the user's organization.
pub async fn effective_project_role_in_user_org(
    pool: &sqlx::PgPool,
    user: &CurrentUser,
    project_id: ProjectId,
) -> ApiResult<ProjectRole> {
    let project = ProjectRepo::new(pool).get(project_id).await?;
    if project.org_id != user.org_id {
        return Err(ApiError::not_found("project not found"));
    }
    effective_project_role(pool, user, project_id).await
}

/// Effective pipeline role for a user, combining project-inherited and
/// direct pipeline membership with visibility checks.
pub async fn effective_pipeline_role(
    pool: &sqlx::PgPool,
    user: &CurrentUser,
    pipeline_id: PipelineId,
    project_id: ProjectId,
) -> ApiResult<PipelineRole> {
    if !user.can_access_pipeline(pipeline_id, project_id) {
        return Err(ApiError::forbidden("no access to this pipeline"));
    }
    if is_super_admin(user) {
        return Ok(PipelineRole::Admin);
    }

    let pipeline = PipelineRepo::new(pool).get(pipeline_id).await?;
    let access = PipelineAccessRepo::new(pool);
    let role = access
        .effective_role_with_visibility(pipeline_id, Some(user.user_id), pipeline.visibility)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    match role {
        Some(r) => Ok(r),
        None => Err(ApiError::forbidden("no access to this pipeline")),
    }
}

/// Enforces API-token pipeline allowlists after project access is established.
pub fn ensure_api_token_pipeline_scope(
    user: &CurrentUser,
    pipeline_id: PipelineId,
    project_id: ProjectId,
) -> ApiResult<()> {
    if !user.is_api_token {
        return Ok(());
    }
    if user.can_access_pipeline(pipeline_id, project_id) {
        Ok(())
    } else {
        Err(ApiError::forbidden("no access to this pipeline"))
    }
}

pub fn ensure_session_or_app_pipeline_scope(
    caller: &SessionOrApp,
    pipeline_id: PipelineId,
    project_id: ProjectId,
) -> ApiResult<()> {
    match caller {
        SessionOrApp::User(u) => ensure_api_token_pipeline_scope(u, pipeline_id, project_id),
        SessionOrApp::App(_) => Ok(()),
    }
}

/// User org + ACL, or app installation read access to exactly this project.
pub async fn effective_project_role_session_or_app_in_user_org(
    pool: &sqlx::PgPool,
    caller: &SessionOrApp,
    project_id: ProjectId,
) -> ApiResult<ProjectRole> {
    match caller {
        SessionOrApp::User(u) => effective_project_role_in_user_org(pool, u, project_id).await,
        SessionOrApp::App(p) => {
            let project = ProjectRepo::new(pool).get(project_id).await?;
            if project.org_id != p.org_id {
                return Err(ApiError::not_found("project not found"));
            }
            ensure_app_can_read_project(p, project_id)?;
            Ok(ProjectRole::Readonly)
        }
    }
}

/// Check if unauthenticated access to public resources is enabled at the platform level.
pub async fn is_unauthenticated_access_enabled(pool: &sqlx::PgPool) -> ApiResult<bool> {
    PlatformSettingsRepo::new(pool)
        .allow_unauthenticated_access()
        .await
        .map_err(|e| ApiError::internal(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use met_core::ids::{OrganizationId, UserId};
    use std::collections::HashSet;

    fn make_user(perms: &[&str]) -> CurrentUser {
        CurrentUser {
            user_id: UserId::new(),
            org_id: OrganizationId::new(),
            email: "test@example.com".to_string(),
            name: None,
            permissions: perms.iter().map(|s| s.to_string()).collect::<HashSet<_>>(),
            is_api_token: false,
            project_ids: None,
            pipeline_ids: None,
            password_must_change: false,
            api_token_id: None,
        }
    }

    #[test]
    fn test_wildcard_is_super_admin() {
        let user = make_user(&["*"]);
        assert!(is_super_admin(&user));
        assert!(!is_platform_admin(&user));
    }

    #[test]
    fn test_admin_metadata_is_platform_admin() {
        let user = make_user(&["admin:metadata"]);
        assert!(!is_super_admin(&user));
        assert!(is_platform_admin(&user));
    }

    #[test]
    fn test_wildcard_and_admin_metadata_is_super_admin() {
        let user = make_user(&["admin:metadata", "*"]);
        assert!(is_super_admin(&user));
        assert!(!is_platform_admin(&user));
    }

    #[test]
    fn test_no_special_permissions_is_neither() {
        let user = make_user(&["pipeline:read", "run:read"]);
        assert!(!is_super_admin(&user));
        assert!(!is_platform_admin(&user));
    }
}
