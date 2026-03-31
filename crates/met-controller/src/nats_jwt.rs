//! Issue per-agent NATS user JWTs (decentralized JWT auth).

use std::time::Duration as StdDuration;

use chrono::{Duration, Utc};
use met_core::ids::{AgentId, OrganizationId};
use nats_io_jwt::{Claims, Permission, Token, User};
use nkeys::KeyPair;

use crate::error::{ControllerError, Result};

/// Build a NATS user JWT and NKey seed for an agent, scoped to org job subjects and JetStream APIs.
pub fn issue_agent_nats_credentials(
    org_id: OrganizationId,
    pool_tags: &[String],
    agent_id: AgentId,
    account_signing_seed: &str,
    issuer_account_pubkey: Option<&str>,
    ttl: StdDuration,
) -> Result<(String, String)> {
    let account_kp = KeyPair::from_seed(account_signing_seed.trim())
        .map_err(|e| ControllerError::Internal(format!("invalid MET_NATS_ACCOUNT_SIGNING_SEED: {e}")))?;

    let user_kp = KeyPair::new_user();

    let org = org_id.as_uuid().to_string();
    let mut allow_sub: Vec<String> = vec![
        "_INBOX.>".to_string(),
        "$JS.API.>".to_string(),
        "$JS.ACK.>".to_string(),
        format!("met.broadcast.{org}"),
    ];
    for tag in pool_tags {
        allow_sub.push(format!("met.jobs.{org}.{tag}"));
    }

    let allow_pub = vec![
        "$JS.API.>".to_string(),
        "$JS.ACK.>".to_string(),
    ];

    let mut user_builder = User::builder()
        .bearer_token(false)
        .sub(Permission::from(allow_sub))
        .pub_(Permission::from(allow_pub));

    if let Some(pubkey) = issuer_account_pubkey.filter(|s| !s.is_empty()) {
        user_builder = user_builder.issuer_account(Some(pubkey.to_string()));
    }

    let user: User = user_builder
        .try_into()
        .map_err(|e: nats_io_jwt::error::ConversionError| {
            ControllerError::Internal(format!("NATS user JWT claims invalid: {e}"))
        })?;

    let chrono_ttl = Duration::from_std(ttl).map_err(|_| {
        ControllerError::Internal("NATS agent JWT ttl out of range for timestamp".into())
    })?;
    let exp = (Utc::now() + chrono_ttl).timestamp();
    let jwt = Token::new(user_kp.public_key())
        .name(format!("agent-{}", agent_id.as_uuid()))
        .claims(Claims::from(user))
        .expires(exp)
        .sign(&account_kp);

    let seed = user_kp
        .seed()
        .map_err(|e| ControllerError::Internal(format!("NKey seed export failed: {e}")))?;

    Ok((jwt, seed))
}
