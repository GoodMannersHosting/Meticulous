//! Per-project ACL (`project_members`). When a project has no rows, all org users keep **Developer**-level access (legacy).

use met_core::ids::{ProjectId, UserId};
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
    roles.max_by_key(|r| match r {
        ProjectRole::Readonly => 0i8,
        ProjectRole::Developer => 1,
        ProjectRole::Admin => 2,
    })
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

    /// Effective role for a human user. `None` means no explicit membership while the project is restricted.
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

        Ok(max_project_role(rows.into_iter().filter_map(|(s,)| parse_role(&s))))
    }

    /// Require a project role that may write secrets (admin or developer), or org `*` (checked by caller).
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
}
