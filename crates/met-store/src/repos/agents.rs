//! Agent repository.

use chrono::Utc;
use met_core::ids::{AgentId, OrganizationId};
use met_core::models::{Agent, AgentStatus};
use sqlx::PgPool;

use crate::error::{Result, StoreError};

/// Repository for agent operations.
pub struct AgentRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> AgentRepo<'a> {
    /// Create a new agent repository.
    #[must_use]
    pub const fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Register a new agent.
    pub async fn register(&self, agent: &Agent) -> Result<Agent> {
        let registered = sqlx::query_as::<_, Agent>(
            r#"
            INSERT INTO agents (id, org_id, name, status, pool, tags, capabilities, os, arch, version, ip_address, max_jobs, running_jobs, last_heartbeat_at, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
            ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name,
                status = EXCLUDED.status,
                pool = EXCLUDED.pool,
                tags = EXCLUDED.tags,
                capabilities = EXCLUDED.capabilities,
                os = EXCLUDED.os,
                arch = EXCLUDED.arch,
                version = EXCLUDED.version,
                ip_address = EXCLUDED.ip_address,
                max_jobs = EXCLUDED.max_jobs,
                last_heartbeat_at = EXCLUDED.last_heartbeat_at
            RETURNING id, org_id, name, status, pool, tags, capabilities, os, arch, version, ip_address, max_jobs, running_jobs, last_heartbeat_at, created_at
            "#,
        )
        .bind(agent.id.as_uuid())
        .bind(agent.org_id.as_uuid())
        .bind(&agent.name)
        .bind(&agent.status)
        .bind(&agent.pool)
        .bind(&agent.tags)
        .bind(&agent.capabilities)
        .bind(&agent.os)
        .bind(&agent.arch)
        .bind(&agent.version)
        .bind(&agent.ip_address)
        .bind(agent.max_jobs)
        .bind(agent.running_jobs)
        .bind(agent.last_heartbeat_at)
        .bind(agent.created_at)
        .fetch_one(self.pool)
        .await?;

        Ok(registered)
    }

    /// Get an agent by ID.
    pub async fn get(&self, id: AgentId) -> Result<Agent> {
        sqlx::query_as::<_, Agent>(
            r#"
            SELECT id, org_id, name, status, pool, tags, capabilities, os, arch, version, ip_address, max_jobs, running_jobs, last_heartbeat_at, created_at
            FROM agents
            WHERE id = $1
            "#,
        )
        .bind(id.as_uuid())
        .fetch_optional(self.pool)
        .await?
        .ok_or_else(|| StoreError::not_found("agent", id))
    }

    /// List agents in an organization.
    pub async fn list_by_org(
        &self,
        org_id: OrganizationId,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Agent>> {
        let agents = sqlx::query_as::<_, Agent>(
            r#"
            SELECT id, org_id, name, status, pool, tags, capabilities, os, arch, version, ip_address, max_jobs, running_jobs, last_heartbeat_at, created_at
            FROM agents
            WHERE org_id = $1
            ORDER BY created_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(org_id.as_uuid())
        .bind(limit)
        .bind(offset)
        .fetch_all(self.pool)
        .await?;

        Ok(agents)
    }

    /// List available agents (online and with capacity).
    pub async fn list_available(
        &self,
        org_id: OrganizationId,
        tags: &[String],
    ) -> Result<Vec<Agent>> {
        let agents = sqlx::query_as::<_, Agent>(
            r#"
            SELECT id, org_id, name, status, pool, tags, capabilities, os, arch, version, ip_address, max_jobs, running_jobs, last_heartbeat_at, created_at
            FROM agents
            WHERE org_id = $1 
                AND status = 'online'
                AND running_jobs < max_jobs
                AND tags @> $2
            ORDER BY running_jobs ASC, last_heartbeat_at DESC
            "#,
        )
        .bind(org_id.as_uuid())
        .bind(tags)
        .fetch_all(self.pool)
        .await?;

        Ok(agents)
    }

    /// Update agent status.
    pub async fn update_status(&self, id: AgentId, status: AgentStatus) -> Result<()> {
        let result = sqlx::query(
            r#"
            UPDATE agents
            SET status = $2
            WHERE id = $1
            "#,
        )
        .bind(id.as_uuid())
        .bind(status)
        .execute(self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(StoreError::not_found("agent", id));
        }

        Ok(())
    }

    /// Update agent heartbeat.
    pub async fn heartbeat(&self, id: AgentId, running_jobs: i32) -> Result<()> {
        let now = Utc::now();

        let result = sqlx::query(
            r#"
            UPDATE agents
            SET last_heartbeat_at = $2, running_jobs = $3, status = 'online'
            WHERE id = $1
            "#,
        )
        .bind(id.as_uuid())
        .bind(now)
        .bind(running_jobs)
        .execute(self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(StoreError::not_found("agent", id));
        }

        Ok(())
    }

    /// Mark stale agents as offline.
    pub async fn mark_stale_offline(&self, max_heartbeat_age_secs: i64) -> Result<u64> {
        let result = sqlx::query(
            r#"
            UPDATE agents
            SET status = 'offline'
            WHERE status = 'online'
                AND last_heartbeat_at < NOW() - ($1 || ' seconds')::interval
            "#,
        )
        .bind(max_heartbeat_age_secs.to_string())
        .execute(self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    /// Count agents by status in an organization.
    pub async fn count_by_status(
        &self,
        org_id: OrganizationId,
        status: AgentStatus,
    ) -> Result<i64> {
        let (count,): (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*)
            FROM agents
            WHERE org_id = $1 AND status = $2
            "#,
        )
        .bind(org_id.as_uuid())
        .bind(status)
        .fetch_one(self.pool)
        .await?;

        Ok(count)
    }
}
