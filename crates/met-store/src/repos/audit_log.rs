//! Repository for the append-only audit log.
//!
//! Only supports INSERT and SELECT operations. The database trigger
//! prevents UPDATE and DELETE on the audit_log table.

use chrono::{DateTime, Utc};
use serde_json::Value as JsonValue;
use sqlx::PgPool;
use tracing::debug;
use uuid::Uuid;

/// Row from the audit_log table.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AuditLogRow {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub action: String,
    pub actor_type: String,
    pub actor_id: String,
    pub actor_name: Option<String>,
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
    pub resource_name: Option<String>,
    pub org_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    pub outcome: String,
    pub client_ip: Option<String>,
    pub user_agent: Option<String>,
    pub request_id: Option<String>,
    pub metadata: JsonValue,
    pub error_message: Option<String>,
}

/// Input for creating an audit log entry.
#[derive(Debug)]
pub struct CreateAuditLog {
    pub action: String,
    pub actor_type: String,
    pub actor_id: String,
    pub actor_name: Option<String>,
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
    pub resource_name: Option<String>,
    pub org_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    pub outcome: String,
    pub client_ip: Option<String>,
    pub user_agent: Option<String>,
    pub request_id: Option<String>,
    pub metadata: JsonValue,
    pub error_message: Option<String>,
}

/// Filter for querying audit logs.
#[derive(Debug, Default)]
pub struct AuditLogFilter {
    pub action: Option<String>,
    pub actor_type: Option<String>,
    pub actor_id: Option<String>,
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
    pub org_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    pub outcome: Option<String>,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub limit: i64,
    pub offset: i64,
}

/// Append-only audit log repository.
pub struct AuditLogRepo;

impl AuditLogRepo {
    /// Insert a new audit log entry.
    pub async fn insert(pool: &PgPool, entry: CreateAuditLog) -> Result<Uuid, sqlx::Error> {
        let row: (Uuid,) = sqlx::query_as(
            r#"
            INSERT INTO audit_log (
                action, actor_type, actor_id, actor_name,
                resource_type, resource_id, resource_name,
                org_id, project_id, outcome,
                client_ip, user_agent, request_id,
                metadata, error_message
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
                $11::inet, $12, $13, $14, $15
            )
            RETURNING id
            "#,
        )
        .bind(&entry.action)
        .bind(&entry.actor_type)
        .bind(&entry.actor_id)
        .bind(&entry.actor_name)
        .bind(&entry.resource_type)
        .bind(&entry.resource_id)
        .bind(&entry.resource_name)
        .bind(entry.org_id)
        .bind(entry.project_id)
        .bind(&entry.outcome)
        .bind(&entry.client_ip)
        .bind(&entry.user_agent)
        .bind(&entry.request_id)
        .bind(&entry.metadata)
        .bind(&entry.error_message)
        .fetch_one(pool)
        .await?;

        debug!(id = %row.0, action = %entry.action, "Audit log entry created");
        Ok(row.0)
    }

    /// Query audit logs with filters.
    pub async fn query(
        pool: &PgPool,
        filter: AuditLogFilter,
    ) -> Result<Vec<AuditLogRow>, sqlx::Error> {
        let limit = filter.limit.min(1000).max(1);
        let offset = filter.offset.max(0);

        // Build dynamic query
        let mut query = String::from("SELECT * FROM audit_log WHERE 1=1");
        let mut param_idx = 1u32;

        if filter.action.is_some() {
            query.push_str(&format!(" AND action = ${param_idx}"));
            param_idx += 1;
        }
        if filter.actor_id.is_some() {
            query.push_str(&format!(" AND actor_id = ${param_idx}"));
            param_idx += 1;
        }
        if filter.resource_type.is_some() {
            query.push_str(&format!(" AND resource_type = ${param_idx}"));
            param_idx += 1;
        }
        if filter.org_id.is_some() {
            query.push_str(&format!(" AND org_id = ${param_idx}"));
            param_idx += 1;
        }
        if filter.start_time.is_some() {
            query.push_str(&format!(" AND timestamp >= ${param_idx}"));
            param_idx += 1;
        }
        if filter.end_time.is_some() {
            query.push_str(&format!(" AND timestamp < ${param_idx}"));
            param_idx += 1;
        }

        query.push_str(&format!(" ORDER BY timestamp DESC LIMIT ${param_idx}"));
        param_idx += 1;
        query.push_str(&format!(" OFFSET ${param_idx}"));

        let mut q = sqlx::query_as::<_, AuditLogRow>(&query);

        if let Some(ref action) = filter.action {
            q = q.bind(action);
        }
        if let Some(ref actor_id) = filter.actor_id {
            q = q.bind(actor_id);
        }
        if let Some(ref rt) = filter.resource_type {
            q = q.bind(rt);
        }
        if let Some(org_id) = filter.org_id {
            q = q.bind(org_id);
        }
        if let Some(start) = filter.start_time {
            q = q.bind(start);
        }
        if let Some(end) = filter.end_time {
            q = q.bind(end);
        }

        q = q.bind(limit);
        q = q.bind(offset);

        q.fetch_all(pool).await
    }

    /// Count audit log entries matching a filter.
    pub async fn count(
        pool: &PgPool,
        action: Option<&str>,
        org_id: Option<Uuid>,
    ) -> Result<i64, sqlx::Error> {
        let row: (i64,) = match (action, org_id) {
            (Some(action), Some(org_id)) => {
                sqlx::query_as("SELECT COUNT(*) FROM audit_log WHERE action = $1 AND org_id = $2")
                    .bind(action)
                    .bind(org_id)
                    .fetch_one(pool)
                    .await?
            }
            (Some(action), None) => {
                sqlx::query_as("SELECT COUNT(*) FROM audit_log WHERE action = $1")
                    .bind(action)
                    .fetch_one(pool)
                    .await?
            }
            (None, Some(org_id)) => {
                sqlx::query_as("SELECT COUNT(*) FROM audit_log WHERE org_id = $1")
                    .bind(org_id)
                    .fetch_one(pool)
                    .await?
            }
            (None, None) => {
                sqlx::query_as("SELECT COUNT(*) FROM audit_log")
                    .fetch_one(pool)
                    .await?
            }
        };
        Ok(row.0)
    }
}
