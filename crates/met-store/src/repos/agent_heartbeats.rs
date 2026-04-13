//! Agent heartbeat repository.

use chrono::{DateTime, Utc};
use met_core::ids::AgentId;
use met_core::models::{AgentHeartbeat, AgentStatus};
use sqlx::PgPool;

use crate::error::Result;

/// Repository for agent heartbeat operations.
pub struct AgentHeartbeatRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> AgentHeartbeatRepo<'a> {
    /// Create a new agent heartbeat repository.
    #[must_use]
    pub const fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Record a new heartbeat.
    pub async fn record(&self, heartbeat: &AgentHeartbeat) -> Result<AgentHeartbeat> {
        let recorded = sqlx::query_as::<_, AgentHeartbeat>(
            r#"
            INSERT INTO agent_heartbeats (id, agent_id, status, cpu_percent, memory_percent, disk_percent, current_job_id, recorded_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING id, agent_id, status, cpu_percent, memory_percent, disk_percent, current_job_id, recorded_at
            "#,
        )
        .bind(heartbeat.id.as_uuid())
        .bind(heartbeat.agent_id.as_uuid())
        .bind(&heartbeat.status)
        .bind(heartbeat.cpu_percent)
        .bind(heartbeat.memory_percent)
        .bind(heartbeat.disk_percent)
        .bind(heartbeat.current_job_id)
        .bind(heartbeat.recorded_at)
        .fetch_one(self.pool)
        .await?;

        Ok(recorded)
    }

    /// Get recent heartbeats for an agent.
    pub async fn get_recent(&self, agent_id: AgentId, limit: i64) -> Result<Vec<AgentHeartbeat>> {
        let heartbeats = sqlx::query_as::<_, AgentHeartbeat>(
            r#"
            SELECT id, agent_id, status, cpu_percent, memory_percent, disk_percent, current_job_id, recorded_at
            FROM agent_heartbeats
            WHERE agent_id = $1
            ORDER BY recorded_at DESC
            LIMIT $2
            "#,
        )
        .bind(agent_id.as_uuid())
        .bind(limit)
        .fetch_all(self.pool)
        .await?;

        Ok(heartbeats)
    }

    /// Get heartbeats for an agent in a time range.
    pub async fn get_in_range(
        &self,
        agent_id: AgentId,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<AgentHeartbeat>> {
        let heartbeats = sqlx::query_as::<_, AgentHeartbeat>(
            r#"
            SELECT id, agent_id, status, cpu_percent, memory_percent, disk_percent, current_job_id, recorded_at
            FROM agent_heartbeats
            WHERE agent_id = $1 AND recorded_at >= $2 AND recorded_at <= $3
            ORDER BY recorded_at DESC
            "#,
        )
        .bind(agent_id.as_uuid())
        .bind(start)
        .bind(end)
        .fetch_all(self.pool)
        .await?;

        Ok(heartbeats)
    }

    /// Get the last heartbeat for an agent.
    pub async fn get_last(&self, agent_id: AgentId) -> Result<Option<AgentHeartbeat>> {
        let heartbeat = sqlx::query_as::<_, AgentHeartbeat>(
            r#"
            SELECT id, agent_id, status, cpu_percent, memory_percent, disk_percent, current_job_id, recorded_at
            FROM agent_heartbeats
            WHERE agent_id = $1
            ORDER BY recorded_at DESC
            LIMIT 1
            "#,
        )
        .bind(agent_id.as_uuid())
        .fetch_optional(self.pool)
        .await?;

        Ok(heartbeat)
    }

    /// Delete old heartbeats (retention cleanup).
    pub async fn delete_older_than(&self, before: DateTime<Utc>) -> Result<u64> {
        let result = sqlx::query(
            r#"
            DELETE FROM agent_heartbeats
            WHERE recorded_at < $1
            "#,
        )
        .bind(before)
        .execute(self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    /// Get aggregate stats for an agent over a time period.
    pub async fn get_stats(
        &self,
        agent_id: AgentId,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<HeartbeatStats> {
        let stats = sqlx::query_as::<_, HeartbeatStats>(
            r#"
            SELECT 
                COUNT(*) as count,
                AVG(cpu_percent) as avg_cpu,
                AVG(memory_percent) as avg_memory,
                AVG(disk_percent) as avg_disk,
                MAX(cpu_percent) as max_cpu,
                MAX(memory_percent) as max_memory,
                MAX(disk_percent) as max_disk
            FROM agent_heartbeats
            WHERE agent_id = $1 AND recorded_at >= $2 AND recorded_at <= $3
            "#,
        )
        .bind(agent_id.as_uuid())
        .bind(start)
        .bind(end)
        .fetch_one(self.pool)
        .await?;

        Ok(stats)
    }

    /// Count heartbeats by status for an agent.
    pub async fn count_by_status(&self, agent_id: AgentId, status: AgentStatus) -> Result<i64> {
        let (count,): (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*)
            FROM agent_heartbeats
            WHERE agent_id = $1 AND status = $2
            "#,
        )
        .bind(agent_id.as_uuid())
        .bind(status)
        .fetch_one(self.pool)
        .await?;

        Ok(count)
    }
}

/// Aggregate statistics for heartbeats.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct HeartbeatStats {
    /// Number of heartbeats in the period.
    pub count: i64,
    /// Average CPU utilization.
    pub avg_cpu: Option<f64>,
    /// Average memory utilization.
    pub avg_memory: Option<f64>,
    /// Average disk utilization.
    pub avg_disk: Option<f64>,
    /// Maximum CPU utilization.
    pub max_cpu: Option<f32>,
    /// Maximum memory utilization.
    pub max_memory: Option<f32>,
    /// Maximum disk utilization.
    pub max_disk: Option<f32>,
}
