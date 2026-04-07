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

/// Re-register an agent that was previously enrolled with this token, without consuming another use.
///
/// Requires the join token to be exhausted (`current_uses >= max_uses`), not revoked, not expired,
/// and the caller-supplied [`Agent::id`] must match [`JoinToken::consumed_by_agent_id`]. The
/// stored [`Agent::last_security_bundle`] must contain the same non-empty `machine_id` as
/// `incoming_machine_id` (trimmed compare).
pub async fn reenroll_agent_with_exhausted_join_token(
    pool: &PgPool,
    token_hash: &str,
    incoming_machine_id: &str,
    agent: &Agent,
) -> Result<(Agent, JoinToken)> {
    let incoming = incoming_machine_id.trim();
    if incoming.is_empty() {
        return Err(StoreError::Constraint(
            "machine_id required to re-register with an exhausted join token".into(),
        ));
    }

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
    if join_token.current_uses < join_token.max_uses {
        tx.rollback().await.ok();
        return Err(StoreError::Constraint(
            "join token is not exhausted; use normal registration".into(),
        ));
    }

    let Some(consumed_id) = join_token.consumed_by_agent_id else {
        tx.rollback().await.ok();
        return Err(StoreError::Constraint(
            "join token has no consuming agent for re-registration".into(),
        ));
    };

    if consumed_id != agent.id {
        tx.rollback().await.ok();
        return Err(StoreError::Constraint(
            "re-registration agent id does not match token consumer".into(),
        ));
    }

    if agent.join_token_id != Some(join_token.id) {
        tx.rollback().await.ok();
        return Err(StoreError::Constraint(
            "agent join_token_id does not match this join token".into(),
        ));
    }

    let existing: Option<Agent> = sqlx::query_as(&format!(
        r#"SELECT {AGENT_ROW_SELECT} FROM agents WHERE id = $1 FOR UPDATE"#,
        AGENT_ROW_SELECT = AGENT_ROW_SELECT
    ))
    .bind(agent.id.as_uuid())
    .fetch_optional(&mut *tx)
    .await?;

    let existing = match existing {
        Some(row) => row,
        None => {
            tx.rollback().await.ok();
            return Err(StoreError::not_found("agent", agent.id));
        }
    };

    let stored_mid = existing
        .last_security_bundle
        .as_ref()
        .and_then(|snap| snap.get("machine_id"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let Some(stored_mid) = stored_mid else {
        tx.rollback().await.ok();
        return Err(StoreError::Constraint(
            "cannot re-register: enrolled agent has no machine_id on record".into(),
        ));
    };

    if stored_mid != incoming {
        tx.rollback().await.ok();
        return Err(StoreError::Constraint(
            "join token cannot be re-used from this host".into(),
        ));
    }

    let updated = update_agent_registration_tx(&mut tx, agent).await?;

    let updated_token: JoinToken = sqlx::query_as(&format!(
        r#"SELECT {JOIN_TOKEN_ROW} FROM join_tokens WHERE id = $1"#,
    ))
    .bind(join_token.id.as_uuid())
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok((updated, updated_token))
}

async fn update_agent_registration_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    agent: &Agent,
) -> Result<Agent> {
    let sql = format!(
        r#"
            UPDATE agents SET
                org_id = $2,
                name = $3,
                status = $4,
                pool = $5,
                pool_tags = $6,
                tags = $7,
                capabilities = $8,
                os = $9,
                arch = $10,
                version = $11,
                ip_address = $12,
                max_jobs = $13,
                running_jobs = $14,
                last_heartbeat_at = $15,
                environment_type = $16,
                kernel_version = $17,
                public_ips = $18,
                private_ips = $19,
                ntp_synchronized = $20,
                container_runtime = $21,
                container_runtime_version = $22,
                x509_public_key = $23,
                join_token_id = $24,
                jwt_expires_at = $25,
                jwt_renewable = $26,
                drain_missed_heartbeats = $27,
                deregistered_at = $28,
                last_security_bundle = $29
            WHERE id = $1 AND deregistered_at IS NULL
            RETURNING {AGENT_ROW_SELECT}
            "#,
        AGENT_ROW_SELECT = AGENT_ROW_SELECT
    );

    let row = sqlx::query_as::<_, Agent>(&sql)
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
        .fetch_optional(&mut **tx)
        .await?;

    match row {
        Some(a) => Ok(a),
        None => Err(StoreError::not_found("agent", agent.id)),
    }
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
