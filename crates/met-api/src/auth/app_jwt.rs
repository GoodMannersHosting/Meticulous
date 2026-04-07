//! Validation of Meticulous App–issued JWTs (RS256 / ES256), separate from user session JWTs.

use crate::config::JwtConfig;
use crate::error::ApiError;
use base64::Engine;
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};
use met_controller::nats::subjects;
use met_core::ids::{AppInstallationId, MeticulousAppId, ProjectId};
use met_store::PgPool;
use met_store::repos::MeticulousAppRepo;
use serde::{Deserialize, Deserializer, Serialize};
use std::fmt;
use std::str::FromStr;
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
    /// Subject: installation id as a raw UUID or prefixed `appi_<uuid>` (public id form).
    pub sub: String,
    /// `aud` may be a string or (for some JWT libraries) a single-element array.
    #[serde(deserialize_with = "deserialize_aud_claim")]
    pub aud: String,
    #[serde(deserialize_with = "deserialize_unix_ts")]
    pub exp: i64,
    #[serde(deserialize_with = "deserialize_unix_ts")]
    pub iat: i64,
    #[serde(default)]
    pub permissions: Vec<String>,
}

fn deserialize_aud_claim<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::{self, IgnoredAny, Visitor};

    struct AudVisitor;

    impl<'de> Visitor<'de> for AudVisitor {
        type Value = String;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a string or single-element string array for `aud`")
        }

        fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
            Ok(value.to_owned())
        }

        fn visit_string<E: de::Error>(self, value: String) -> Result<Self::Value, E> {
            Ok(value)
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: de::SeqAccess<'de>,
        {
            let first: Option<String> = seq.next_element()?;
            let s = first.ok_or_else(|| de::Error::custom("`aud` array is empty"))?;
            if seq.next_element::<IgnoredAny>()?.is_some() {
                return Err(de::Error::custom(
                    "`aud` array must contain exactly one string for integration JWTs",
                ));
            }
            Ok(s)
        }
    }

    deserializer.deserialize_any(AudVisitor)
}

fn deserialize_unix_ts<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::{self, Visitor};

    struct TsVisitor;

    impl<'de> Visitor<'de> for TsVisitor {
        type Value = i64;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a unix timestamp (integer, float, or decimal string)")
        }

        fn visit_i64<E: de::Error>(self, v: i64) -> Result<i64, E> {
            Ok(v)
        }

        fn visit_u64<E: de::Error>(self, v: u64) -> Result<i64, E> {
            i64::try_from(v).map_err(|_| de::Error::custom("timestamp out of i64 range"))
        }

        fn visit_f64<E: de::Error>(self, v: f64) -> Result<i64, E> {
            if !v.is_finite() {
                return Err(de::Error::custom("non-finite timestamp"));
            }
            Ok(v as i64)
        }

        fn visit_str<E: de::Error>(self, v: &str) -> Result<i64, E> {
            v.parse::<i64>().map_err(de::Error::custom)
        }
    }

    deserializer.deserialize_any(TsVisitor)
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
    let payload_b64 = token.split('.').nth(1).ok_or_else(|| {
        AppJwtError::Jwt(jsonwebtoken::errors::Error::from(
            jsonwebtoken::errors::ErrorKind::InvalidToken,
        ))
    })?;
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload_b64.as_bytes())
        .map_err(|_| {
            AppJwtError::Jwt(jsonwebtoken::errors::Error::from(
                jsonwebtoken::errors::ErrorKind::InvalidToken,
            ))
        })
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
            return Err(ApiError::unauthorized(
                AppJwtError::WeakAlgorithm.to_string(),
            ));
        }
        _ => {
            return Err(ApiError::unauthorized(
                AppJwtError::WeakAlgorithm.to_string(),
            ));
        }
    };

    let kid = header
        .kid
        .as_deref()
        .ok_or_else(|| ApiError::unauthorized(AppJwtError::MissingKid.to_string()))?;

    let payload_bytes =
        jwt_payload_json(token).map_err(|e| ApiError::unauthorized(e.to_string()))?;
    let preview: AppJwtClaims = serde_json::from_slice(&payload_bytes)
        .map_err(|_| ApiError::unauthorized(AppJwtError::InvalidPayload.to_string()))?;

    let repo = MeticulousAppRepo::new(db);
    let (app_row_id, public_pem) = repo
        .get_active_public_key_pem(&preview.iss, kid)
        .await
        .map_err(|_| ApiError::unauthorized(AppJwtError::UnknownKey.to_string()))?;

    let decoding_key = match alg {
        Algorithm::RS256 => DecodingKey::from_rsa_pem(public_pem.as_bytes())
            .map_err(|e| ApiError::internal(format!("invalid stored public key: {e}")))?,
        Algorithm::ES256 => DecodingKey::from_ec_pem(public_pem.as_bytes())
            .map_err(|e| ApiError::internal(format!("invalid stored public key: {e}")))?,
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

    let installation_id = AppInstallationId::from_str(verified.sub.trim())
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
