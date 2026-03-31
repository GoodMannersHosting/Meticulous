//! Agent repository.

use chrono::{DateTime, Utc};
use met_core::ids::{AgentId, OrganizationId};
use met_core::models::{Agent, AgentStatus};
use sqlx::PgPool;

use crate::error::{Result, StoreError};

/// Repository for agent operations.
pub struct AgentRepo<'a> {
    pool: &'a PgPool,
}

/// Columns selected / returned for [`Agent`] (`sqlx::FromRow` must match the row).
const AGENT_ROW_SELECT: &str = r#"
    id, org_id, name, status, pool, tags, capabilities, os, arch, version, ip_address,
    max_jobs, running_jobs, last_heartbeat_at, created_at,
    environment_type, kernel_version, public_ips, private_ips, ntp_synchronized,
    container_runtime, container_runtime_version, x509_public_key, join_token_id,
    jwt_expires_at, jwt_renewable, deregistered_at
"#;

impl<'a> AgentRepo<'a> {
    /// Create a new agent repository.
    #[must_use]
    pub const fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Register a new agent.
    pub async fn register(&self, agent: &Agent) -> Result<Agent> {
        let sql = format!(
            r#"
            INSERT INTO agents (
                id, org_id, name, status, pool, tags, capabilities, os, arch, version, ip_address,
                max_jobs, running_jobs, last_heartbeat_at, created_at,
                environment_type, kernel_version, public_ips, private_ips, ntp_synchronized,
                container_runtime, container_runtime_version, x509_public_key, join_token_id,
                jwt_expires_at, jwt_renewable, deregistered_at
            )
            VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15,
                $16, $17, $18, $19, $20, $21, $22, $23, $24, $25, $26, $27
            )
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
                running_jobs = EXCLUDED.running_jobs,
                last_heartbeat_at = EXCLUDED.last_heartbeat_at,
                environment_type = EXCLUDED.environment_type,
                kernel_version = EXCLUDED.kernel_version,
                public_ips = EXCLUDED.public_ips,
                private_ips = EXCLUDED.private_ips,
                ntp_synchronized = EXCLUDED.ntp_synchronized,
                container_runtime = EXCLUDED.container_runtime,
                container_runtime_version = EXCLUDED.container_runtime_version,
                x509_public_key = EXCLUDED.x509_public_key,
                join_token_id = EXCLUDED.join_token_id,
                jwt_expires_at = EXCLUDED.jwt_expires_at,
                jwt_renewable = EXCLUDED.jwt_renewable,
                deregistered_at = EXCLUDED.deregistered_at
            RETURNING {AGENT_ROW_SELECT}
            "#,
            AGENT_ROW_SELECT = AGENT_ROW_SELECT
        );
        let registered = sqlx::query_as::<_, Agent>(&sql)
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
        .bind(&agent.environment_type)
        .bind(&agent.kernel_version)
        .bind(&agent.public_ips)
        .bind(&agent.private_ips)
        .bind(agent.ntp_synchronized)
        .bind(&agent.container_runtime)
        .bind(&agent.container_runtime_version)
        .bind(&agent.x509_public_key)
        .bind(agent.join_token_id.map(|j| j.as_uuid()))
        .bind(agent.jwt_expires_at)
        .bind(agent.jwt_renewable)
        .bind(agent.deregistered_at)
        .fetch_one(self.pool)
        .await?;

        Ok(registered)
    }

    /// Get an agent by ID.
    pub async fn get(&self, id: AgentId) -> Result<Agent> {
        sqlx::query_as::<_, Agent>(&format!(
            r#"
            SELECT {AGENT_ROW_SELECT}
            FROM agents
            WHERE id = $1 AND deregistered_at IS NULL
            "#,
            AGENT_ROW_SELECT = AGENT_ROW_SELECT
        ))
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
        let agents = sqlx::query_as::<_, Agent>(&format!(
            r#"
            SELECT {AGENT_ROW_SELECT}
            FROM agents
            WHERE org_id = $1 AND deregistered_at IS NULL
            ORDER BY created_at DESC
            LIMIT $2 OFFSET $3
            "#,
            AGENT_ROW_SELECT = AGENT_ROW_SELECT
        ))
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
        let agents = sqlx::query_as::<_, Agent>(&format!(
            r#"
            SELECT {AGENT_ROW_SELECT}
            FROM agents
            WHERE org_id = $1
                AND deregistered_at IS NULL
                AND status = 'online'
                AND running_jobs < max_jobs
                AND tags @> $2
            ORDER BY running_jobs ASC, last_heartbeat_at DESC
            "#,
            AGENT_ROW_SELECT = AGENT_ROW_SELECT
        ))
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

    /// Update agent heartbeat from the controller (live connection).
    pub async fn heartbeat(&self, id: AgentId, status: AgentStatus, running_jobs: i32) -> Result<()> {
        let now = Utc::now();

        let result = sqlx::query(
            r#"
            UPDATE agents
            SET last_heartbeat_at = $2, running_jobs = $3, status = $4
            WHERE id = $1 AND deregistered_at IS NULL
            "#,
        )
        .bind(id.as_uuid())
        .bind(now)
        .bind(running_jobs)
        .bind(status)
        .execute(self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(StoreError::not_found("agent", id));
        }

        Ok(())
    }

    /// Mark stale agents as offline (no recent heartbeat from a live agent).
    pub async fn mark_stale_offline(&self, max_heartbeat_age_secs: i64) -> Result<u64> {
        let result = sqlx::query(
            r#"
            UPDATE agents
            SET status = 'offline'
            WHERE deregistered_at IS NULL
                AND status IN ('online', 'busy', 'draining')
                AND last_heartbeat_at IS NOT NULL
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
            WHERE org_id = $1 AND status = $2 AND deregistered_at IS NULL
            "#,
        )
        .bind(org_id.as_uuid())
        .bind(status)
        .fetch_one(self.pool)
        .await?;

        Ok(count)
    }

    /// Validate NTP synchronization and optional binary SHA, then persist the
    /// security bundle fields for an agent. Returns a constraint error if the
    /// agent's clock is not NTP-synchronized or the binary hash doesn't match.
    pub async fn validate_security_bundle(
        &self,
        id: AgentId,
        ntp_synchronized: bool,
        binary_sha256: Option<&str>,
        expected_binary_sha256: Option<&str>,
        environment_type: &str,
        kernel_version: Option<&str>,
        public_ips: &[String],
        private_ips: &[String],
        container_runtime: Option<&str>,
        container_runtime_version: Option<&str>,
        x509_public_key: Option<&[u8]>,
    ) -> Result<()> {
        if !ntp_synchronized {
            return Err(StoreError::Constraint(
                "agent clock is not NTP-synchronized; registration rejected".into(),
            ));
        }

        if let (Some(actual), Some(expected)) = (binary_sha256, expected_binary_sha256) {
            if actual != expected {
                return Err(StoreError::Constraint(format!(
                    "binary SHA-256 mismatch: expected {expected}, got {actual}"
                )));
            }
        }

        let result = sqlx::query(
            r#"
            UPDATE agents
            SET environment_type = $2,
                kernel_version = $3,
                public_ips = $4,
                private_ips = $5,
                ntp_synchronized = $6,
                container_runtime = $7,
                container_runtime_version = $8,
                x509_public_key = $9
            WHERE id = $1
            "#,
        )
        .bind(id.as_uuid())
        .bind(environment_type)
        .bind(kernel_version)
        .bind(public_ips)
        .bind(private_ips)
        .bind(ntp_synchronized)
        .bind(container_runtime)
        .bind(container_runtime_version)
        .bind(x509_public_key)
        .execute(self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(StoreError::not_found("agent", id));
        }

        Ok(())
    }

    /// Update an agent's JWT expiration timestamp and renewal eligibility.
    pub async fn update_jwt_expiry(
        &self,
        id: AgentId,
        jwt_expires_at: chrono::DateTime<Utc>,
        jwt_renewable: bool,
    ) -> Result<()> {
        let result = sqlx::query(
            r#"
            UPDATE agents
            SET jwt_expires_at = $2, jwt_renewable = $3
            WHERE id = $1
            "#,
        )
        .bind(id.as_uuid())
        .bind(jwt_expires_at)
        .bind(jwt_renewable)
        .execute(self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(StoreError::not_found("agent", id));
        }

        Ok(())
    }

    /// Soft-delete an agent (removed from listings; heartbeats ignored).
    ///
    /// Fails with [`StoreError::not_found`] if missing or wrong org, or if already deregistered.
    /// Fails with [`StoreError::Constraint`] if `running_jobs > 0`.
    pub async fn soft_delete(&self, org_id: OrganizationId, id: AgentId) -> Result<()> {
        let row: Option<(i32, Option<DateTime<Utc>>)> = sqlx::query_as(
            "SELECT running_jobs, deregistered_at FROM agents WHERE id = $1 AND org_id = $2",
        )
        .bind(id.as_uuid())
        .bind(org_id.as_uuid())
        .fetch_optional(self.pool)
        .await?;

        let Some((running_jobs, deregistered_at)) = row else {
            return Err(StoreError::not_found("agent", id));
        };

        if deregistered_at.is_some() {
            return Err(StoreError::not_found("agent", id));
        }

        if running_jobs > 0 {
            return Err(StoreError::Constraint(format!(
                "agent has {running_jobs} running job(s); finish or cancel them before removal"
            )));
        }

        sqlx::query(
            r#"
            UPDATE agents
            SET deregistered_at = NOW(), status = 'decommissioned'
            WHERE id = $1 AND org_id = $2 AND deregistered_at IS NULL
            "#,
        )
        .bind(id.as_uuid())
        .bind(org_id.as_uuid())
        .execute(self.pool)
        .await?;

        Ok(())
    }

    /// Mark an agent as approved for JWT renewal. Only agents that are online
    /// and not in a terminal state can be approved.
    pub async fn approve_renewal(&self, id: AgentId) -> Result<()> {
        let result = sqlx::query(
            r#"
            UPDATE agents
            SET jwt_renewable = true
            WHERE id = $1
                AND status NOT IN ('decommissioned', 'revoked', 'dead')
            "#,
        )
        .bind(id.as_uuid())
        .execute(self.pool)
        .await?;

        if result.rows_affected() == 0 {
            let exists = sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM agents WHERE id = $1)",
            )
            .bind(id.as_uuid())
            .fetch_one(self.pool)
            .await?;

            if exists {
                return Err(StoreError::Constraint(
                    "agent is in a terminal state and cannot be approved for renewal".into(),
                ));
            }
            return Err(StoreError::not_found("agent", id));
        }

        Ok(())
    }
}
