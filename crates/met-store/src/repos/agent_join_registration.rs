//! Atomically register an agent and record join token consumption.

use met_core::models::{Agent, JoinToken};
use sqlx::PgPool;

use crate::error::{Result, StoreError};
use crate::repos::agents::AGENT_ROW_SELECT;

const JOIN_TOKEN_ROW: &str = r#"
    id, token_hash, scope, scope_id, description, org_id, max_uses, current_uses,
    labels, pool_tags, expires_at, revoked, created_by, created_at, updated_at,
    consumed_by_agent_id, consumed_at
"#;

/// Insert the agent and consume the join token in one transaction.
pub async fn register_agent_with_join_token(
    pool: &PgPool,
    token_hash: &str,
    agent: &Agent,
) -> Result<(Agent, JoinToken)> {
    let mut tx = pool.begin().await?;

    let token: Option<JoinToken> = sqlx::query_as(&format!(
        r#"SELECT {JOIN_TOKEN_ROW} FROM join_tokens WHERE token_hash = $1 FOR UPDATE"#,
    ))
    .bind(token_hash)
    .fetch_optional(&mut *tx)
    .await?;

    let join_token = match token {
        Some(t) => t,
        None => {
            tx.rollback().await.ok();
            return Err(StoreError::not_found("join_token", token_hash));
        }
    };

    if join_token.revoked {
        tx.rollback().await.ok();
        return Err(StoreError::Constraint(
            "join token is expired, revoked, or has reached max uses".into(),
        ));
    }
    if let Some(exp) = join_token.expires_at
        && exp < chrono::Utc::now()
    {
        tx.rollback().await.ok();
        return Err(StoreError::Constraint(
            "join token is expired, revoked, or has reached max uses".into(),
        ));
    }
    if join_token.current_uses >= join_token.max_uses {
        tx.rollback().await.ok();
        return Err(StoreError::Constraint(
            "join token is expired, revoked, or has reached max uses".into(),
        ));
    }

    let registered = insert_agent_tx(&mut tx, agent).await?;

    let update = sqlx::query(
        r#"
        UPDATE join_tokens
        SET current_uses = current_uses + 1,
            consumed_by_agent_id = $1,
            consumed_at = NOW(),
            updated_at = NOW()
        WHERE id = $2
        "#,
    )
    .bind(agent.id.as_uuid())
    .bind(join_token.id.as_uuid())
    .execute(&mut *tx)
    .await?;

    if update.rows_affected() != 1 {
        tx.rollback().await.ok();
        return Err(StoreError::Constraint(
            "internal error: join token update affected unexpected row count".into(),
        ));
    }

    let updated_token: JoinToken = sqlx::query_as(&format!(
        r#"SELECT {JOIN_TOKEN_ROW} FROM join_tokens WHERE id = $1"#,
    ))
    .bind(join_token.id.as_uuid())
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok((registered, updated_token))
}

async fn insert_agent_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    agent: &Agent,
) -> Result<Agent> {
    let sql = format!(
        r#"
            INSERT INTO agents (
                id, org_id, name, status, pool, pool_tags, tags, capabilities, os, arch, version, ip_address,
                max_jobs, running_jobs, last_heartbeat_at, created_at,
                environment_type, kernel_version, public_ips, private_ips, ntp_synchronized,
                container_runtime, container_runtime_version, x509_public_key, join_token_id,
                jwt_expires_at, jwt_renewable, drain_missed_heartbeats, deregistered_at,
                last_security_bundle
            )
            VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16,
                $17, $18, $19, $20, $21, $22, $23, $24, $25, $26, $27, $28, $29, $30
            )
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
        .bind(&agent.pool_tags)
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
        .bind(agent.drain_missed_heartbeats)
        .bind(agent.deregistered_at)
        .bind(&agent.last_security_bundle)
        .fetch_one(&mut **tx)
        .await?;

    Ok(registered)
}
