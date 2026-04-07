//! Issue per-agent NATS user JWTs (decentralized JWT auth).

use std::time::Duration as StdDuration;

use chrono::{Duration, Utc};
use met_core::ids::{AgentId, OrganizationId};
use nats_io_jwt::{Claims, Permission, Token, User};
use nkeys::KeyPair;
use tracing::warn;

use crate::error::{ControllerError, Result};

/// `nats-io-jwt` `Token::sign` uses `expect`/`unwrap` internally; a bad signing key or malformed
/// claims can panic and tear down the gRPC connection (client sees `BrokenPipe`). Convert that
/// into a normal [`ControllerError`] instead.
fn sign_agent_user_jwt(token: Token, account_kp: &KeyPair) -> Result<String> {
    use std::panic::{AssertUnwindSafe, catch_unwind};

    match catch_unwind(AssertUnwindSafe(|| token.sign(account_kp))) {
        Ok(jwt) => Ok(jwt),
        Err(_) => {
            warn!(
                "NATS user JWT signing failed with a panic inside nats-io-jwt; \
                 verify MET_NATS_ACCOUNT_SIGNING_SEED is a valid account signing seed (SU… from nsc), \
                 not a user, operator, or account public key"
            );
            Err(ControllerError::Internal(
                "NATS user JWT signing failed; check MET_NATS_ACCOUNT_SIGNING_SEED".into(),
            ))
        }
    }
}

/// Build a NATS user JWT and NKey seed for an agent, scoped to org job subjects and JetStream APIs.
pub fn issue_agent_nats_credentials(
    org_id: OrganizationId,
    _pool_tags: &[String],
    agent_id: AgentId,
    account_signing_seed: &str,
    issuer_account_pubkey: Option<&str>,
    ttl: StdDuration,
) -> Result<(String, String)> {
    let account_kp = KeyPair::from_seed(account_signing_seed.trim()).map_err(|e| {
        ControllerError::Internal(format!("invalid MET_NATS_ACCOUNT_SIGNING_SEED: {e}"))
    })?;

    let user_kp = KeyPair::new_user();

    let org = org_id.as_uuid().to_string();
    let agent = agent_id.as_uuid().to_string();
    let allow_sub: Vec<String> = vec![
        "_INBOX.>".to_string(),
        "$JS.API.>".to_string(),
        "$JS.ACK.>".to_string(),
        format!("met.broadcast.{org}"),
        // Per-agent job inbox (`*` = pool tag). WorkQueue streams require non-overlapping filters.
        format!("met.jobs.{org}.*.{agent}"),
    ];

    let allow_pub = vec!["$JS.API.>".to_string(), "$JS.ACK.>".to_string()];

    let mut user_builder = User::builder()
        .bearer_token(false)
        .sub(Permission::from(allow_sub))
        .pub_(Permission::from(allow_pub));

    if let Some(pubkey) = issuer_account_pubkey.filter(|s| !s.is_empty()) {
        user_builder = user_builder.issuer_account(Some(pubkey.to_string()));
    }

    let user: User =
        user_builder
            .try_into()
            .map_err(|e: nats_io_jwt::error::ConversionError| {
                ControllerError::Internal(format!("NATS user JWT claims invalid: {e}"))
            })?;

    let chrono_ttl = Duration::from_std(ttl).map_err(|_| {
        ControllerError::Internal("NATS agent JWT ttl out of range for timestamp".into())
    })?;
    let exp = (Utc::now() + chrono_ttl).timestamp();
    let token = Token::new(user_kp.public_key())
        .name(format!("agent-{}", agent_id.as_uuid()))
        .claims(Claims::from(user))
        .expires(exp);
    let jwt = sign_agent_user_jwt(token, &account_kp)?;

    let seed = user_kp
        .seed()
        .map_err(|e| ControllerError::Internal(format!("NKey seed export failed: {e}")))?;

    Ok((jwt, seed))
}
