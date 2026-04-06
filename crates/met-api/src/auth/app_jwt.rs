//! Validation of Meticulous App–issued JWTs (RS256 / ES256), separate from user session JWTs.

use base64::Engine;
use crate::config::JwtConfig;
use crate::error::ApiError;
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};
use met_controller::nats::subjects;
use met_core::ids::{AppInstallationId, MeticulousAppId, ProjectId};
use met_store::PgPool;
use met_store::repos::MeticulousAppRepo;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Authenticated Meticulous App installation after JWT verification.
#[derive(Debug, Clone)]
pub struct AppInstallationPrincipal {
    pub application_id: String,
    pub app_id: MeticulousAppId,
    pub installation_id: AppInstallationId,
    pub project_id: ProjectId,
    /// Permission allowlist from the installation row (source of truth).
    pub permissions: Vec<String>,
}

/// JWT claims for app integrations.
#[derive(Debug, Serialize, Deserialize)]
pub struct AppJwtClaims {
    /// Issuer: `application_id` (public app id string).
    pub iss: String,
    /// Subject: `installation_id` (UUID string).
    pub sub: String,
    pub aud: String,
    pub exp: i64,
    pub iat: i64,
    #[serde(default)]
    pub permissions: Vec<String>,
}

#[derive(Debug, Error)]
pub enum AppJwtError {
    #[error("invalid JWT: {0}")]
    Jwt(#[from] jsonwebtoken::errors::Error),

    #[error("JWT uses a disallowed algorithm (only RS256 and ES256 are accepted)")]
    WeakAlgorithm,

    #[error("JWT is missing `kid` header")]
    MissingKid,

    #[error("invalid installation id in `sub`")]
    BadSubject,

    #[error("token TTL exceeds maximum allowed for app JWTs")]
    TtlTooLong,

    #[error("installation is revoked or unknown")]
    InstallationInactive,

    #[error("issuer does not match installed application")]
    IssuerMismatch,

    #[error("invalid JWT payload")]
    InvalidPayload,

    #[error("unknown app or signing key")]
    UnknownKey,
}

/// Accepts the legacy audience (`jwt.audience`) and the integration default
/// `{audience}.{JOBS_STREAM}` derived from the pipeline job queue (JetStream `JOBS`).
pub fn integration_jwt_audience_candidates(jwt_config: &JwtConfig) -> Vec<String> {
    vec![
        jwt_config.audience.clone(),
        format!("{}.{}", jwt_config.audience, subjects::JOBS_STREAM),
    ]
}

/// Audience value advertised to external integrators (e.g. Kubernetes operator) for new tokens.
pub fn integration_jwt_audience_for_ingress(jwt_config: &JwtConfig) -> String {
    format!("{}.{}", jwt_config.audience, subjects::JOBS_STREAM)
}

fn jwt_payload_json(token: &str) -> Result<Vec<u8>, AppJwtError> {
    let payload_b64 = token
        .split('.')
        .nth(1)
        .ok_or_else(|| AppJwtError::Jwt(jsonwebtoken::errors::Error::from(jsonwebtoken::errors::ErrorKind::InvalidToken)))?;
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload_b64.as_bytes())
        .map_err(|_| AppJwtError::Jwt(jsonwebtoken::errors::Error::from(jsonwebtoken::errors::ErrorKind::InvalidToken)))
}

/// Verify a bearer token as a Meticulous App JWT and resolve the active installation.
pub async fn verify_app_installation_jwt(
    token: &str,
    jwt_config: &JwtConfig,
    db: &PgPool,
) -> Result<AppInstallationPrincipal, ApiError> {
    let header = decode_header(token).map_err(|e| ApiError::unauthorized(e.to_string()))?;

    let alg = match header.alg {
        Algorithm::RS256 | Algorithm::ES256 => header.alg,
        Algorithm::HS256 | Algorithm::HS384 | Algorithm::HS512 => {
            return Err(ApiError::unauthorized(AppJwtError::WeakAlgorithm.to_string()));
        }
        _ => {
            return Err(ApiError::unauthorized(AppJwtError::WeakAlgorithm.to_string()));
        }
    };

    let kid = header
        .kid
        .as_deref()
        .ok_or_else(|| ApiError::unauthorized(AppJwtError::MissingKid.to_string()))?;

    let payload_bytes = jwt_payload_json(token).map_err(|e| ApiError::unauthorized(e.to_string()))?;
    let preview: AppJwtClaims =
        serde_json::from_slice(&payload_bytes).map_err(|_| ApiError::unauthorized(AppJwtError::InvalidPayload.to_string()))?;

    let repo = MeticulousAppRepo::new(db);
    let (app_row_id, public_pem) = repo
        .get_active_public_key_pem(&preview.iss, kid)
        .await
        .map_err(|_| ApiError::unauthorized(AppJwtError::UnknownKey.to_string()))?;

    let decoding_key = match alg {
        Algorithm::RS256 => DecodingKey::from_rsa_pem(public_pem.as_bytes()).map_err(|e| {
            ApiError::internal(format!("invalid stored public key: {e}"))
        })?,
        Algorithm::ES256 => DecodingKey::from_ec_pem(public_pem.as_bytes()).map_err(|e| {
            ApiError::internal(format!("invalid stored public key: {e}"))
        })?,
        _ => unreachable!(),
    };

    let allowed = integration_jwt_audience_candidates(jwt_config);
    let allowed_refs: Vec<&str> = allowed.iter().map(|s| s.as_str()).collect();

    let mut validation = Validation::new(alg);
    validation.set_audience(&allowed_refs);
    validation.leeway = jwt_config.app_leeway_secs;
    validation.validate_exp = true;

    let verified = decode::<AppJwtClaims>(token, &decoding_key, &validation)
        .map_err(|e| ApiError::unauthorized(e.to_string()))?
        .claims;

    let ttl = verified.exp.saturating_sub(verified.iat).max(0) as u64;
    if ttl > jwt_config.app_max_ttl_secs {
        return Err(ApiError::unauthorized(AppJwtError::TtlTooLong.to_string()));
    }

    let installation_id: AppInstallationId = uuid::Uuid::parse_str(&verified.sub)
        .map(AppInstallationId::from_uuid)
        .map_err(|_| ApiError::unauthorized(AppJwtError::BadSubject.to_string()))?;

    let inst = repo
        .get_installation(installation_id)
        .await
        .map_err(|_| ApiError::unauthorized(AppJwtError::InstallationInactive.to_string()))?;

    if inst.revoked_at.is_some() {
        return Err(ApiError::unauthorized(
            AppJwtError::InstallationInactive.to_string(),
        ));
    }

    if inst.app_id != app_row_id {
        return Err(ApiError::unauthorized(
            AppJwtError::IssuerMismatch.to_string(),
        ));
    }

    let app = repo
        .get_by_id(inst.app_id)
        .await
        .map_err(|_| ApiError::unauthorized("app not found"))?;

    if app.application_id != verified.iss {
        return Err(ApiError::unauthorized(
            AppJwtError::IssuerMismatch.to_string(),
        ));
    }

    Ok(AppInstallationPrincipal {
        application_id: app.application_id.clone(),
        app_id: inst.app_id,
        installation_id: inst.id,
        project_id: inst.project_id,
        permissions: inst.permissions.clone(),
    })
}
