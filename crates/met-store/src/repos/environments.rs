//! Repository for pipeline environments (ADR-016, Phase 2.1).

use met_core::ids::{OrganizationId, ProjectId, UserId};
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{Result, StoreError};

/// Environment row.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct EnvironmentRow {
    pub id: Uuid,
    pub org_id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub display_name: String,
    pub description: Option<String>,
    pub require_approval: bool,
    pub required_approvers: i32,
    pub approval_timeout_hours: i32,
    pub allowed_branches: Option<Vec<String>>,
    pub auto_deploy_branch: Option<String>,
    pub variables: serde_json::Value,
    pub tier: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Approval decision row.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct EnvironmentApprovalRow {
    pub id: Uuid,
    pub run_id: Uuid,
    pub environment_id: Uuid,
    pub approved_by: Option<Uuid>,
    pub decision: String,
    pub comment: Option<String>,
    pub decided_at: chrono::DateTime<chrono::Utc>,
}

pub struct EnvironmentRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> EnvironmentRepo<'a> {
    #[must_use]
    pub const fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn list_by_project(&self, project_id: ProjectId) -> Result<Vec<EnvironmentRow>> {
        let rows = sqlx::query_as::<_, EnvironmentRow>(
            r#"
            SELECT id, org_id, project_id, name, display_name, description,
                   require_approval, required_approvers, approval_timeout_hours,
                   allowed_branches, auto_deploy_branch, variables, tier,
                   created_at, updated_at
            FROM environments
            WHERE project_id = $1
            ORDER BY
                CASE tier
                    WHEN 'development' THEN 0
                    WHEN 'staging' THEN 1
                    WHEN 'production' THEN 2
                    ELSE 3
                END, name
            "#,
        )
        .bind(project_id.as_uuid())
        .fetch_all(self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn get(&self, id: Uuid) -> Result<EnvironmentRow> {
        sqlx::query_as::<_, EnvironmentRow>(
            r#"
            SELECT id, org_id, project_id, name, display_name, description,
                   require_approval, required_approvers, approval_timeout_hours,
                   allowed_branches, auto_deploy_branch, variables, tier,
                   created_at, updated_at
            FROM environments WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(self.pool)
        .await?
        .ok_or_else(|| StoreError::not_found("environment", id))
    }

    pub async fn get_by_name(
        &self,
        project_id: ProjectId,
        name: &str,
    ) -> Result<Option<EnvironmentRow>> {
        sqlx::query_as::<_, EnvironmentRow>(
            r#"
            SELECT id, org_id, project_id, name, display_name, description,
                   require_approval, required_approvers, approval_timeout_hours,
                   allowed_branches, auto_deploy_branch, variables, tier,
                   created_at, updated_at
            FROM environments WHERE project_id = $1 AND name = $2
            "#,
        )
        .bind(project_id.as_uuid())
        .bind(name)
        .fetch_optional(self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn create(
        &self,
        org_id: OrganizationId,
        project_id: ProjectId,
        name: &str,
        display_name: &str,
        description: Option<&str>,
        tier: &str,
    ) -> Result<EnvironmentRow> {
        let row = sqlx::query_as::<_, EnvironmentRow>(
            r#"
            INSERT INTO environments (org_id, project_id, name, display_name, description, tier)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id, org_id, project_id, name, display_name, description,
                      require_approval, required_approvers, approval_timeout_hours,
                      allowed_branches, auto_deploy_branch, variables, tier,
                      created_at, updated_at
            "#,
        )
        .bind(org_id.as_uuid())
        .bind(project_id.as_uuid())
        .bind(name)
        .bind(display_name)
        .bind(description)
        .bind(tier)
        .fetch_one(self.pool)
        .await?;
        Ok(row)
    }

    pub async fn update(
        &self,
        id: Uuid,
        name: Option<&str>,
        display_name: Option<&str>,
        description: Option<&str>,
        tier: Option<&str>,
        require_approval: Option<bool>,
        required_approvers: Option<i32>,
        approval_timeout_hours: Option<i32>,
        allowed_branches: Option<&[String]>,
        auto_deploy_branch: Option<&str>,
        variables: Option<&serde_json::Value>,
    ) -> Result<EnvironmentRow> {
        let existing = self.get(id).await?;
        let new_name: &str = name.unwrap_or(&existing.name);
        if let Some(n) = name
            && n != existing.name
            && let Some(other) = self
                .get_by_name(ProjectId::from_uuid(existing.project_id), n)
                .await?
            && other.id != id
        {
            return Err(StoreError::validation(format!(
                "environment name '{n}' is already in use in this project"
            )));
        }
        let row = sqlx::query_as::<_, EnvironmentRow>(
            r#"
            UPDATE environments SET
                name = $2,
                display_name = $3,
                description = $4,
                tier = $5,
                require_approval = $6,
                required_approvers = $7,
                approval_timeout_hours = $8,
                allowed_branches = $9,
                auto_deploy_branch = $10,
                variables = $11,
                updated_at = NOW()
            WHERE id = $1
            RETURNING id, org_id, project_id, name, display_name, description,
                      require_approval, required_approvers, approval_timeout_hours,
                      allowed_branches, auto_deploy_branch, variables, tier,
                      created_at, updated_at
            "#,
        )
        .bind(id)
        .bind(new_name)
        .bind(display_name.unwrap_or(&existing.display_name))
        .bind(description.or(existing.description.as_deref()))
        .bind(tier.unwrap_or(&existing.tier))
        .bind(require_approval.unwrap_or(existing.require_approval))
        .bind(required_approvers.unwrap_or(existing.required_approvers))
        .bind(approval_timeout_hours.unwrap_or(existing.approval_timeout_hours))
        .bind(
            allowed_branches
                .map(|b| b.to_vec())
                .or(existing.allowed_branches),
        )
        .bind(
            auto_deploy_branch
                .map(String::from)
                .or(existing.auto_deploy_branch),
        )
        .bind(variables.unwrap_or(&existing.variables))
        .fetch_one(self.pool)
        .await?;
        Ok(row)
    }

    pub async fn delete(&self, id: Uuid) -> Result<()> {
        let r = sqlx::query("DELETE FROM environments WHERE id = $1")
            .bind(id)
            .execute(self.pool)
            .await?;
        if r.rows_affected() == 0 {
            return Err(StoreError::not_found("environment", id));
        }
        Ok(())
    }

    /// Record an approval or rejection decision.
    pub async fn record_approval(
        &self,
        run_id: Uuid,
        environment_id: Uuid,
        approved_by: UserId,
        decision: &str,
        comment: Option<&str>,
    ) -> Result<EnvironmentApprovalRow> {
        let row = sqlx::query_as::<_, EnvironmentApprovalRow>(
            r#"
            INSERT INTO environment_approvals (run_id, environment_id, approved_by, decision, comment)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (run_id, environment_id, approved_by)
            DO UPDATE SET decision = EXCLUDED.decision, comment = EXCLUDED.comment, decided_at = NOW()
            RETURNING id, run_id, environment_id, approved_by, decision, comment, decided_at
            "#,
        )
        .bind(run_id)
        .bind(environment_id)
        .bind(approved_by.as_uuid())
        .bind(decision)
        .bind(comment)
        .fetch_one(self.pool)
        .await?;
        Ok(row)
    }

    /// Count approvals for a run + environment.
    pub async fn count_approvals(&self, run_id: Uuid, environment_id: Uuid) -> Result<i64> {
        let (c,): (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*)::bigint FROM environment_approvals
            WHERE run_id = $1 AND environment_id = $2 AND decision = 'approved'
            "#,
        )
        .bind(run_id)
        .bind(environment_id)
        .fetch_one(self.pool)
        .await?;
        Ok(c)
    }

    /// Check if a branch ref matches the environment's allowed_branches patterns.
    pub fn branch_allowed(env: &EnvironmentRow, trigger_ref: &str) -> bool {
        let Some(ref patterns) = env.allowed_branches else {
            return true;
        };
        if patterns.is_empty() {
            return true;
        }
        let branch = trigger_ref
            .strip_prefix("refs/heads/")
            .unwrap_or(trigger_ref);
        patterns.iter().any(|p| branch_glob_match(p, branch))
    }
}

fn branch_glob_match(pattern: &str, branch: &str) -> bool {
    let pat_parts: Vec<&str> = pattern.split('/').collect();
    let branch_parts: Vec<&str> = branch.split('/').collect();
    if pat_parts.len() != branch_parts.len() {
        return false;
    }
    pat_parts
        .iter()
        .zip(branch_parts.iter())
        .all(|(p, b)| *p == "*" || *p == *b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_with_branches(branches: Option<Vec<String>>) -> EnvironmentRow {
        EnvironmentRow {
            id: Uuid::nil(),
            org_id: Uuid::nil(),
            project_id: Uuid::nil(),
            name: "test".into(),
            display_name: "Test".into(),
            description: None,
            require_approval: false,
            required_approvers: 1,
            approval_timeout_hours: 72,
            allowed_branches: branches,
            auto_deploy_branch: None,
            variables: serde_json::json!({}),
            tier: "development".into(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn test_branch_allowed_none() {
        let env = env_with_branches(None);
        assert!(EnvironmentRepo::branch_allowed(&env, "refs/heads/main"));
    }

    #[test]
    fn test_branch_allowed_empty() {
        let env = env_with_branches(Some(vec![]));
        assert!(EnvironmentRepo::branch_allowed(&env, "refs/heads/main"));
    }

    #[test]
    fn test_branch_allowed_exact_match() {
        let env = env_with_branches(Some(vec!["main".into()]));
        assert!(EnvironmentRepo::branch_allowed(&env, "refs/heads/main"));
        assert!(!EnvironmentRepo::branch_allowed(&env, "refs/heads/develop"));
    }

    #[test]
    fn test_branch_allowed_glob() {
        let env = env_with_branches(Some(vec!["release/*".into()]));
        assert!(EnvironmentRepo::branch_allowed(
            &env,
            "refs/heads/release/1.0"
        ));
        assert!(!EnvironmentRepo::branch_allowed(
            &env,
            "refs/heads/feature/foo"
        ));
        assert!(!EnvironmentRepo::branch_allowed(
            &env,
            "refs/heads/release/1.0/hotfix"
        ));
    }

    #[test]
    fn test_branch_glob_match() {
        assert!(branch_glob_match("main", "main"));
        assert!(branch_glob_match("release/*", "release/v1"));
        assert!(!branch_glob_match("release/*", "release/v1/hotfix"));
        assert!(branch_glob_match("*", "anything"));
    }
}
