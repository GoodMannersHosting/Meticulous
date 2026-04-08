//! Resolve per-project roles from `project_members` (and legacy open projects).

use met_core::ids::{OrganizationId, PipelineId, ProjectId};
use met_core::models::installation_has_read_access;
use met_store::repos::{ProjectAccessRepo, ProjectRole, ProjectRepo};

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

/// Effective role on a project (`*` admins are treated as project admin).
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
    if user.has_permission("*") {
        return Ok(ProjectRole::Admin);
    }
    let access = ProjectAccessRepo::new(pool);
    let role = access
        .effective_role_for_user(project_id, user.user_id)
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
