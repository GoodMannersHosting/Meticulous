//! Short-lived ES256 OIDC workload identity JWTs (ADR-017).

use chrono::{DateTime, Utc};
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use met_core::ids::{AgentId, JobRunId, OrganizationId, PipelineId, ProjectId, RunId};
use met_secrets::{BuiltinStoredCrypto, decrypt_pkcs8_private_key, ec_private_key_pem_from_pkcs8_der};
use met_store::PgPool;
use met_store::repos::{JobRunRepo, OidcJobIdentityRow, OidcSigningKeyRepo};
use serde::Serialize;
use tonic::Status;
use uuid::Uuid;

use crate::config::ControllerConfig;

#[derive(Serialize)]
struct WorkloadIdentityClaims {
    iss: String,
    sub: String,
    aud: String,
    exp: i64,
    iat: i64,
    jti: String,
    org_id: String,
    org_slug: String,
    project_id: String,
    project_slug: String,
    pipeline_id: String,
    pipeline_name: String,
    run_id: String,
    job_run_id: String,
    #[serde(rename = "ref", skip_serializing_if = "Option::is_none")]
    git_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sha: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    environment: Option<String>,
    runner_environment: &'static str,
}

pub(crate) fn resolve_oidc_issuer_base(config: &ControllerConfig) -> String {
    if let Some(ref u) = config.oidc_issuer_url {
        let s = u.trim().trim_end_matches('/');
        if !s.is_empty() {
            return s.to_string();
        }
    }
    if let Some(ref u) = config.http_public_base_url {
        let s = u.trim().trim_end_matches('/');
        if !s.is_empty() {
            return s.to_string();
        }
    }
    config
        .http_cors_first_origin
        .as_deref()
        .map(|s| s.trim().trim_end_matches('/').to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "https://meticulous.example.com".to_string())
}

/// Build `sub` from verified DB row (ADR-017). Empty optional parts are omitted.
pub(crate) fn build_oidc_sub(row: &OidcJobIdentityRow) -> String {
    let mut s = format!(
        "org:{}:project:{}:pipeline:{}",
        row.org_slug, row.project_slug, row.pipeline_name
    );
    if let Some(ref b) = row.branch {
        if !b.is_empty() {
            s.push_str(&format!(":ref:{b}"));
        }
    }
    if let Some(ref sha) = row.commit_sha {
        if !sha.is_empty() {
            s.push_str(&format!(":sha:{sha}"));
        }
    }
    if let Some(ref env) = row.environment_name {
        if !env.is_empty() {
            s.push_str(&format!(":environment:{env}"));
        }
    }
    s
}

pub(crate) async fn mint_workload_identity_token(
    pool: &PgPool,
    crypto: &BuiltinStoredCrypto,
    config: &ControllerConfig,
    agent_id: AgentId,
    job_run_id: JobRunId,
    audience: &str,
) -> Result<(String, DateTime<Utc>), Status> {
    let repo = JobRunRepo::new(pool);
    let row = repo
        .load_for_oidc_identity_token(job_run_id, agent_id)
        .await
        .map_err(|e| Status::internal(format!("database error: {e}")))?;
    let row = row.ok_or_else(|| {
        Status::not_found(
            "no running job run for this agent_id and job_run_id (or job is not in running state)",
        )
    })?;

    let iss = resolve_oidc_issuer_base(config);
    let sub = build_oidc_sub(&row);
    let now = Utc::now();
    let ttl = chrono::Duration::from_std(config.oidc_id_token_ttl).map_err(|_| {
        Status::internal("oidc_id_token_ttl out of range for chrono::Duration")
    })?;
    let exp = now + ttl;
    let jti = Uuid::new_v4();

    let claims = WorkloadIdentityClaims {
        iss,
        sub,
        aud: audience.to_string(),
        exp: exp.timestamp(),
        iat: now.timestamp(),
        jti: jti.to_string(),
        org_id: OrganizationId::from_uuid(row.org_id).to_string(),
        org_slug: row.org_slug.clone(),
        project_id: ProjectId::from_uuid(row.project_id).to_string(),
        project_slug: row.project_slug.clone(),
        pipeline_id: PipelineId::from_uuid(row.pipeline_id).to_string(),
        pipeline_name: row.pipeline_name.clone(),
        run_id: RunId::from_uuid(row.run_id).to_string(),
        job_run_id: JobRunId::from_uuid(row.job_run_id).to_string(),
        git_ref: row.branch.clone(),
        sha: row.commit_sha.clone(),
        environment: row.environment_name.clone(),
        runner_environment: "self-hosted",
    };

    let signing_repo = OidcSigningKeyRepo::new(pool);
    let key_row = signing_repo
        .active_key()
        .await
        .map_err(|e| Status::internal(format!("oidc signing key: {e}")))?;
    let key_row = key_row.ok_or_else(|| {
        Status::failed_precondition(
            "no active OIDC signing key; ensure MET_BUILTIN_SECRETS_MASTER_KEY is set and bootstrap succeeded",
        )
    })?;

    let pkcs8 = decrypt_pkcs8_private_key(crypto, &key_row.private_key_enc)
        .map_err(|e| Status::internal(format!("decrypt oidc signing key: {e}")))?;
    let pem = ec_private_key_pem_from_pkcs8_der(&pkcs8)
        .map_err(|e| Status::internal(format!("pkcs8 to pem: {e}")))?;
    let encoding_key = EncodingKey::from_ec_pem(pem.as_bytes())
        .map_err(|e| Status::internal(format!("encoding key: {e}")))?;

    let mut header = Header::new(Algorithm::ES256);
    header.kid = Some(key_row.kid.clone());

    let token = encode(&header, &claims, &encoding_key)
        .map_err(|e| Status::internal(format!("jwt encode: {e}")))?;

    signing_repo
        .audit_token(
            job_run_id.as_uuid(),
            agent_id.as_uuid(),
            audience,
            &key_row.kid,
            jti,
            exp,
        )
        .await
        .map_err(|e| Status::internal(format!("audit token: {e}")))?;

    Ok((token, exp))
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn sub_includes_optional_segments() {
        let row = OidcJobIdentityRow {
            org_id: Uuid::nil(),
            org_slug: "acme".into(),
            project_id: Uuid::nil(),
            project_slug: "api".into(),
            pipeline_id: Uuid::nil(),
            pipeline_name: "deploy".into(),
            run_id: Uuid::nil(),
            job_run_id: Uuid::nil(),
            branch: Some("refs/heads/main".into()),
            commit_sha: Some("abc".into()),
            environment_name: Some("production".into()),
        };
        assert_eq!(
            build_oidc_sub(&row),
            "org:acme:project:api:pipeline:deploy:ref:refs/heads/main:sha:abc:environment:production"
        );
    }

    #[test]
    fn sub_omits_empty_optionals() {
        let row = OidcJobIdentityRow {
            org_id: Uuid::nil(),
            org_slug: "o".into(),
            project_id: Uuid::nil(),
            project_slug: "p".into(),
            pipeline_id: Uuid::nil(),
            pipeline_name: "pipe".into(),
            run_id: Uuid::nil(),
            job_run_id: Uuid::nil(),
            branch: None,
            commit_sha: None,
            environment_name: None,
        };
        assert_eq!(build_oidc_sub(&row), "org:o:project:p:pipeline:pipe");
    }
}
