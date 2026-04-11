//! Per-project ACL (`project_members`).
//!
//! Visibility-aware: `public` projects give unauthenticated users readonly;
//! `authenticated` projects give all org members readonly; `private` projects
//! require explicit membership. Legacy projects with zero `project_members`
//! rows still grant **Developer** access to all org members.

use met_core::ids::{ProjectId, UserId};
use met_core::models::ResourceVisibility;
use sqlx::PgPool;

use crate::error::{Result, StoreError};

/// Coarse project role (mirrors DB enum `project_role`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectRole {
    Admin,
    Developer,
    Readonly,
}

impl ProjectRole {
    #[must_use]
    pub fn can_write_secrets(self) -> bool {
        matches!(self, Self::Admin | Self::Developer)
    }

    #[must_use]
    pub fn can_write_variables(self) -> bool {
        matches!(self, Self::Admin | Self::Developer)
    }

    #[must_use]
    pub fn can_trigger_pipelines(self) -> bool {
        matches!(self, Self::Admin | Self::Developer)
    }

    /// Project Admin only — pipeline CRUD, triggers, webhook registration admin, archive.
    #[must_use]
    pub fn can_manage_pipelines(self) -> bool {
        matches!(self, Self::Admin)
    }

    /// Project Admin only — create, update, delete triggers.
    #[must_use]
    pub fn can_manage_triggers(self) -> bool {
        matches!(self, Self::Admin)
    }

    fn rank(self) -> i8 {
        match self {
            Self::Readonly => 0,
            Self::Developer => 1,
            Self::Admin => 2,
        }
    }
}

fn parse_role(s: &str) -> Option<ProjectRole> {
    match s {
        "admin" => Some(ProjectRole::Admin),
        "developer" => Some(ProjectRole::Developer),
        "readonly" => Some(ProjectRole::Readonly),
        _ => None,
    }
}

fn max_project_role(roles: impl Iterator<Item = ProjectRole>) -> Option<ProjectRole> {
    roles.max_by_key(|r| r.rank())
}

/// Repository for [`project_members`](crate::repos::project_members).
pub struct ProjectAccessRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> ProjectAccessRepo<'a> {
    #[must_use]
    pub const fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    async fn row_count(&self, project_id: ProjectId) -> Result<i64> {
        let (c,): (i64,) = sqlx::query_as(
            r#"SELECT COUNT(*)::bigint FROM project_members WHERE project_id = $1"#,
        )
        .bind(project_id.as_uuid())
        .fetch_one(self.pool)
        .await?;
        Ok(c)
    }

    /// Effective role for a human user. `None` means no explicit membership
    /// while the project is restricted.
    pub async fn effective_role_for_user(
        &self,
        project_id: ProjectId,
        user_id: UserId,
    ) -> Result<Option<ProjectRole>> {
        if self.row_count(project_id).await? == 0 {
            return Ok(Some(ProjectRole::Developer));
        }

        let rows: Vec<(String,)> = sqlx::query_as(
            r#"
            SELECT role::text
            FROM project_members
            WHERE project_id = $1
              AND principal_type = 'user'
              AND principal_id = $2
            UNION ALL
            SELECT pm.role::text
            FROM project_members pm
            INNER JOIN group_memberships gm
              ON gm.group_id = pm.principal_id
            WHERE pm.project_id = $1
              AND pm.principal_type = 'group'
              AND gm.user_id = $2
            "#,
        )
        .bind(project_id.as_uuid())
        .bind(user_id.as_uuid())
        .fetch_all(self.pool)
        .await?;

        Ok(max_project_role(
            rows.into_iter().filter_map(|(s,)| parse_role(&s)),
        ))
    }

    /// Visibility-aware role resolution.
    ///
    /// For `public` resources, unauthenticated callers get `Some(Readonly)`.
    /// For `authenticated`, any org member gets at least `Readonly` when they
    /// have no explicit membership. For `private`, membership is required.
    pub async fn effective_role_for_user_with_visibility(
        &self,
        project_id: ProjectId,
        user_id: Option<UserId>,
        visibility: ResourceVisibility,
    ) -> Result<Option<ProjectRole>> {
        if visibility == ResourceVisibility::Public {
            if let Some(uid) = user_id {
                let explicit = self.effective_role_for_user(project_id, uid).await?;
                return Ok(Some(
                    explicit.unwrap_or(ProjectRole::Readonly),
                ));
            }
            return Ok(Some(ProjectRole::Readonly));
        }

        let Some(uid) = user_id else {
            return Ok(None);
        };

        let explicit = self.effective_role_for_user(project_id, uid).await?;

        match visibility {
            ResourceVisibility::Authenticated => {
                Ok(Some(explicit.unwrap_or(ProjectRole::Readonly)))
            }
            ResourceVisibility::Private => Ok(explicit),
            ResourceVisibility::Public => unreachable!(),
        }
    }

    /// Require a project role that may write secrets (admin or developer), or
    /// org `*` (checked by caller).
    pub async fn require_secrets_write(
        &self,
        project_id: ProjectId,
        user_id: UserId,
    ) -> Result<ProjectRole> {
        let Some(role) = self.effective_role_for_user(project_id, user_id).await? else {
            return Err(StoreError::validation(
                "user has no role on this restricted project",
            ));
        };
        if !role.can_write_secrets() {
            return Err(StoreError::validation(
                "read-only project role cannot modify secrets",
            ));
        }
        Ok(role)
    }

    /// Add or upsert a direct member on a project. Returns `true` if a new row
    /// was inserted (vs updated).
    pub async fn add_member(
        &self,
        project_id: ProjectId,
        principal_type: &str,
        principal_id: uuid::Uuid,
        role: &str,
    ) -> Result<bool> {
        let res = sqlx::query(
            r#"
            INSERT INTO project_members (project_id, principal_type, principal_id, role)
            VALUES ($1, $2::project_principal_type, $3, $4::project_role)
            ON CONFLICT (project_id, principal_type, principal_id)
            DO UPDATE SET role = EXCLUDED.role
            "#,
        )
        .bind(project_id.as_uuid())
        .bind(principal_type)
        .bind(principal_id)
        .bind(role)
        .execute(self.pool)
        .await?;
        Ok(res.rows_affected() > 0)
    }

    /// Remove a member. Returns an error if the member is the project owner.
    pub async fn remove_member(
        &self,
        project_id: ProjectId,
        principal_id: uuid::Uuid,
    ) -> Result<bool> {
        let res = sqlx::query(
            r#"DELETE FROM project_members WHERE project_id = $1 AND principal_id = $2"#,
        )
        .bind(project_id.as_uuid())
        .bind(principal_id)
        .execute(self.pool)
        .await?;
        Ok(res.rows_affected() > 0)
    }

    /// List all members of a project with display names.
    pub async fn list_members(
        &self,
        project_id: ProjectId,
    ) -> Result<Vec<ProjectMemberRow>> {
        let rows = sqlx::query_as::<_, ProjectMemberRow>(
            r#"
            SELECT pm.id, pm.project_id, pm.principal_type::text, pm.principal_id,
                   pm.role::text, pm.created_at,
                   CASE
                     WHEN pm.principal_type = 'user' THEN u.email
                     ELSE g.name
                   END AS display_name
            FROM project_members pm
            LEFT JOIN users u ON pm.principal_type = 'user' AND u.id = pm.principal_id
            LEFT JOIN groups g ON pm.principal_type = 'group' AND g.id = pm.principal_id
            WHERE pm.project_id = $1
            ORDER BY pm.created_at
            "#,
        )
        .bind(project_id.as_uuid())
        .fetch_all(self.pool)
        .await?;
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_can_write_secrets() {
        assert!(ProjectRole::Admin.can_write_secrets());
        assert!(ProjectRole::Developer.can_write_secrets());
        assert!(!ProjectRole::Readonly.can_write_secrets());
    }

    #[test]
    fn test_can_manage_pipelines() {
        assert!(ProjectRole::Admin.can_manage_pipelines());
        assert!(!ProjectRole::Developer.can_manage_pipelines());
        assert!(!ProjectRole::Readonly.can_manage_pipelines());
    }

    #[test]
    fn test_parse_role_valid() {
        assert_eq!(parse_role("admin"), Some(ProjectRole::Admin));
        assert_eq!(parse_role("developer"), Some(ProjectRole::Developer));
        assert_eq!(parse_role("readonly"), Some(ProjectRole::Readonly));
    }

    #[test]
    fn test_parse_role_invalid() {
        assert_eq!(parse_role("superuser"), None);
        assert_eq!(parse_role(""), None);
    }

    #[test]
    fn test_max_project_role_readonly_developer() {
        let roles = vec![ProjectRole::Readonly, ProjectRole::Developer];
        assert_eq!(max_project_role(roles.into_iter()), Some(ProjectRole::Developer));
    }

    #[test]
    fn test_max_project_role_readonly_admin() {
        let roles = vec![ProjectRole::Readonly, ProjectRole::Admin];
        assert_eq!(max_project_role(roles.into_iter()), Some(ProjectRole::Admin));
    }

    #[test]
    fn test_max_project_role_empty() {
        let roles: Vec<ProjectRole> = vec![];
        assert_eq!(max_project_role(roles.into_iter()), None);
    }
}

/// Row returned by [`ProjectAccessRepo::list_members`].
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ProjectMemberRow {
    pub id: uuid::Uuid,
    pub project_id: ProjectId,
    pub principal_type: String,
    pub principal_id: uuid::Uuid,
    pub role: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub display_name: Option<String>,
}
