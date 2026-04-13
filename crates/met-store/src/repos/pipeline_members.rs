//! Per-pipeline ACL (`pipeline_members`).
//!
//! Effective pipeline role = max(project-inherited role, direct pipeline role).
//! Inherited members cannot be removed at the pipeline level.

use std::str::FromStr;

use met_core::ids::{PipelineId, ProjectId, UserId};
use met_core::models::ResourceVisibility;
use sqlx::PgPool;

use crate::error::{Result, StoreError};

/// Pipeline-level role (mirrors DB enum `pipeline_role`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineRole {
    Admin,
    Developer,
    Readonly,
}

impl PipelineRole {
    fn rank(self) -> i8 {
        match self {
            Self::Readonly => 0,
            Self::Developer => 1,
            Self::Admin => 2,
        }
    }

    #[must_use]
    pub fn can_edit_definition(self) -> bool {
        matches!(self, Self::Admin | Self::Developer)
    }

    #[must_use]
    pub fn can_manage_members(self) -> bool {
        matches!(self, Self::Admin)
    }

    #[must_use]
    pub fn can_trigger(self) -> bool {
        matches!(self, Self::Admin | Self::Developer)
    }
}

impl FromStr for PipelineRole {
    type Err = ();

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "admin" => std::result::Result::Ok(Self::Admin),
            "developer" => std::result::Result::Ok(Self::Developer),
            "readonly" => std::result::Result::Ok(Self::Readonly),
            _ => std::result::Result::Err(()),
        }
    }
}

fn max_pipeline_role(roles: impl Iterator<Item = PipelineRole>) -> Option<PipelineRole> {
    roles.max_by_key(|r| r.rank())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_can_edit_definition() {
        assert!(PipelineRole::Admin.can_edit_definition());
        assert!(PipelineRole::Developer.can_edit_definition());
        assert!(!PipelineRole::Readonly.can_edit_definition());
    }

    #[test]
    fn test_can_manage_members() {
        assert!(PipelineRole::Admin.can_manage_members());
        assert!(!PipelineRole::Developer.can_manage_members());
        assert!(!PipelineRole::Readonly.can_manage_members());
    }

    #[test]
    fn test_can_trigger() {
        assert!(PipelineRole::Admin.can_trigger());
        assert!(PipelineRole::Developer.can_trigger());
        assert!(!PipelineRole::Readonly.can_trigger());
    }

    #[test]
    fn test_parse_role_valid() {
        assert_eq!(
            "admin".parse::<PipelineRole>(),
            std::result::Result::Ok(PipelineRole::Admin)
        );
        assert_eq!(
            "developer".parse::<PipelineRole>(),
            std::result::Result::Ok(PipelineRole::Developer)
        );
        assert_eq!(
            "readonly".parse::<PipelineRole>(),
            std::result::Result::Ok(PipelineRole::Readonly)
        );
    }

    #[test]
    fn test_parse_role_invalid() {
        assert_eq!(
            PipelineRole::from_str("superuser"),
            std::result::Result::Err(())
        );
        assert_eq!(PipelineRole::from_str(""), std::result::Result::Err(()));
    }

    #[test]
    fn test_max_pipeline_role_readonly_developer() {
        let roles = vec![PipelineRole::Readonly, PipelineRole::Developer];
        assert_eq!(
            max_pipeline_role(roles.into_iter()),
            Some(PipelineRole::Developer)
        );
    }

    #[test]
    fn test_max_pipeline_role_readonly_admin() {
        let roles = vec![PipelineRole::Readonly, PipelineRole::Admin];
        assert_eq!(
            max_pipeline_role(roles.into_iter()),
            Some(PipelineRole::Admin)
        );
    }

    #[test]
    fn test_max_pipeline_role_empty() {
        let roles: Vec<PipelineRole> = vec![];
        assert_eq!(max_pipeline_role(roles.into_iter()), None);
    }
}

/// Row returned by [`PipelineAccessRepo::list_members`].
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PipelineMemberRow {
    pub id: uuid::Uuid,
    pub pipeline_id: PipelineId,
    pub principal_type: String,
    pub principal_id: uuid::Uuid,
    pub role: String,
    pub inherited: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub display_name: Option<String>,
}

pub struct PipelineAccessRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> PipelineAccessRepo<'a> {
    #[must_use]
    pub const fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Effective pipeline role for a user, combining direct and inherited memberships.
    pub async fn effective_role_for_user(
        &self,
        pipeline_id: PipelineId,
        user_id: UserId,
    ) -> Result<Option<PipelineRole>> {
        let rows: Vec<(String,)> = sqlx::query_as(
            r#"
            SELECT role::text
            FROM pipeline_members
            WHERE pipeline_id = $1
              AND principal_type = 'user'
              AND principal_id = $2
            UNION ALL
            SELECT pm.role::text
            FROM pipeline_members pm
            INNER JOIN group_memberships gm
              ON gm.group_id = pm.principal_id
            WHERE pm.pipeline_id = $1
              AND pm.principal_type = 'group'
              AND gm.user_id = $2
            "#,
        )
        .bind(pipeline_id.as_uuid())
        .bind(user_id.as_uuid())
        .fetch_all(self.pool)
        .await?;

        Ok(max_pipeline_role(
            rows.into_iter().filter_map(|(s,)| s.parse().ok()),
        ))
    }

    /// Visibility-aware effective role, following the same pattern as project access.
    pub async fn effective_role_with_visibility(
        &self,
        pipeline_id: PipelineId,
        user_id: Option<UserId>,
        visibility: ResourceVisibility,
    ) -> Result<Option<PipelineRole>> {
        if visibility == ResourceVisibility::Public {
            if let Some(uid) = user_id {
                let explicit = self.effective_role_for_user(pipeline_id, uid).await?;
                return Ok(Some(explicit.unwrap_or(PipelineRole::Readonly)));
            }
            return Ok(Some(PipelineRole::Readonly));
        }

        let Some(uid) = user_id else {
            return Ok(None);
        };

        let explicit = self.effective_role_for_user(pipeline_id, uid).await?;

        match visibility {
            ResourceVisibility::Authenticated => {
                Ok(Some(explicit.unwrap_or(PipelineRole::Readonly)))
            }
            ResourceVisibility::Private => Ok(explicit),
            ResourceVisibility::Public => unreachable!(),
        }
    }

    /// Add a direct (non-inherited) pipeline member.
    pub async fn add_member(
        &self,
        pipeline_id: PipelineId,
        principal_type: &str,
        principal_id: uuid::Uuid,
        role: &str,
    ) -> Result<bool> {
        let res = sqlx::query(
            r#"
            INSERT INTO pipeline_members (pipeline_id, principal_type, principal_id, role, inherited)
            VALUES ($1, $2::pipeline_principal_type, $3, $4::pipeline_role, false)
            ON CONFLICT (pipeline_id, principal_type, principal_id)
            DO UPDATE SET role = EXCLUDED.role
            WHERE pipeline_members.inherited = false
            "#,
        )
        .bind(pipeline_id.as_uuid())
        .bind(principal_type)
        .bind(principal_id)
        .bind(role)
        .execute(self.pool)
        .await?;
        Ok(res.rows_affected() > 0)
    }

    /// Remove a direct pipeline member. Rejects inherited members.
    pub async fn remove_member(
        &self,
        pipeline_id: PipelineId,
        principal_id: uuid::Uuid,
    ) -> Result<bool> {
        let inherited_check: Option<(bool,)> = sqlx::query_as(
            r#"
            SELECT inherited
            FROM pipeline_members
            WHERE pipeline_id = $1 AND principal_id = $2
            "#,
        )
        .bind(pipeline_id.as_uuid())
        .bind(principal_id)
        .fetch_optional(self.pool)
        .await?;

        if let Some((true,)) = inherited_check {
            return Err(StoreError::validation(
                "cannot remove inherited pipeline member; manage at the project level",
            ));
        }

        let res = sqlx::query(
            r#"
            DELETE FROM pipeline_members
            WHERE pipeline_id = $1 AND principal_id = $2 AND inherited = false
            "#,
        )
        .bind(pipeline_id.as_uuid())
        .bind(principal_id)
        .execute(self.pool)
        .await?;
        Ok(res.rows_affected() > 0)
    }

    /// Sync inherited members from the parent project into a pipeline.
    ///
    /// Project members are mirrored as inherited pipeline members with the same
    /// role (mapped from project_role to pipeline_role). Existing inherited rows
    /// are updated; stale inherited rows are removed.
    pub async fn sync_inherited_from_project(
        &self,
        pipeline_id: PipelineId,
        project_id: ProjectId,
    ) -> Result<()> {
        sqlx::query(
            r#"
            DELETE FROM pipeline_members
            WHERE pipeline_id = $1 AND inherited = true
              AND principal_id NOT IN (
                SELECT principal_id FROM project_members WHERE project_id = $2
              )
            "#,
        )
        .bind(pipeline_id.as_uuid())
        .bind(project_id.as_uuid())
        .execute(self.pool)
        .await?;

        sqlx::query(
            r#"
            INSERT INTO pipeline_members (pipeline_id, principal_type, principal_id, role, inherited)
            SELECT $1,
                   pm.principal_type::text::pipeline_principal_type,
                   pm.principal_id,
                   pm.role::text::pipeline_role,
                   true
            FROM project_members pm
            WHERE pm.project_id = $2
            ON CONFLICT (pipeline_id, principal_type, principal_id)
            DO UPDATE SET role = EXCLUDED.role
            WHERE pipeline_members.inherited = true
            "#,
        )
        .bind(pipeline_id.as_uuid())
        .bind(project_id.as_uuid())
        .execute(self.pool)
        .await?;

        Ok(())
    }

    /// List all members of a pipeline with display names.
    pub async fn list_members(&self, pipeline_id: PipelineId) -> Result<Vec<PipelineMemberRow>> {
        let rows = sqlx::query_as::<_, PipelineMemberRow>(
            r#"
            SELECT pm.id, pm.pipeline_id, pm.principal_type::text, pm.principal_id,
                   pm.role::text, pm.inherited, pm.created_at,
                   CASE
                     WHEN pm.principal_type = 'user' THEN u.email
                     ELSE g.name
                   END AS display_name
            FROM pipeline_members pm
            LEFT JOIN users u ON pm.principal_type = 'user' AND u.id = pm.principal_id
            LEFT JOIN groups g ON pm.principal_type = 'group' AND g.id = pm.principal_id
            WHERE pm.pipeline_id = $1
            ORDER BY pm.inherited DESC, pm.created_at
            "#,
        )
        .bind(pipeline_id.as_uuid())
        .fetch_all(self.pool)
        .await?;
        Ok(rows)
    }
}
