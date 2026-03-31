//! OIDC token validation and pipeline workload identity.
//!
//! Provides:
//! - JWT/OIDC token validation with multi-issuer support and JWKS caching
//! - OIDC discovery endpoint generation (/.well-known/openid-configuration)
//! - Pipeline workload identity token issuance with key rotation

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use jsonwebtoken::{
    Algorithm, DecodingKey, EncodingKey, Header, TokenData, Validation,
};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::error::OidcError;

pub type Result<T> = std::result::Result<T, OidcError>;

/// Configuration for the OIDC validator.
#[derive(Debug, Clone)]
pub struct OidcConfig {
    pub issuers: Vec<IssuerConfig>,
    pub jwks_cache_duration: Duration,
    pub clock_skew_tolerance: Duration,
}

impl Default for OidcConfig {
    fn default() -> Self {
        Self {
            issuers: Vec::new(),
            jwks_cache_duration: Duration::from_secs(3600),
            clock_skew_tolerance: Duration::from_secs(60),
        }
    }
}

/// Configuration for a trusted token issuer.
#[derive(Debug, Clone)]
pub struct IssuerConfig {
    pub issuer: String,
    pub audience: String,
    pub jwks_uri: Option<String>,
    pub algorithms: Vec<Algorithm>,
    pub require_email_verified: bool,
}

impl Default for IssuerConfig {
    fn default() -> Self {
        Self {
            issuer: String::new(),
            audience: String::new(),
            jwks_uri: None,
            algorithms: vec![Algorithm::RS256, Algorithm::ES256],
            require_email_verified: false,
        }
    }
}

/// Validated token claims.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatedClaims {
    pub subject: String,
    pub issuer: String,
    pub audience: String,
    pub expires_at: DateTime<Utc>,
    pub issued_at: DateTime<Utc>,
    pub email: Option<String>,
    pub email_verified: Option<bool>,
    pub name: Option<String>,
    pub custom_claims: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct JwtClaims {
    sub: String,
    iss: String,
    aud: AudienceClaim,
    exp: i64,
    iat: Option<i64>,
    #[allow(dead_code)]
    nbf: Option<i64>,
    email: Option<String>,
    email_verified: Option<bool>,
    name: Option<String>,
    #[serde(flatten)]
    extra: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum AudienceClaim {
    Single(String),
    Multiple(Vec<String>),
}

impl AudienceClaim {
    fn contains(&self, audience: &str) -> bool {
        match self {
            Self::Single(s) => s == audience,
            Self::Multiple(v) => v.iter().any(|a| a == audience),
        }
    }
    fn first(&self) -> Option<&str> {
        match self {
            Self::Single(s) => Some(s),
            Self::Multiple(v) => v.first().map(String::as_str),
        }
    }
}

struct CachedJwks {
    keys: HashMap<String, DecodingKey>,
    fetched_at: Instant,
}

impl std::fmt::Debug for CachedJwks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CachedJwks")
            .field("keys", &format!("[{} keys]", self.keys.len()))
            .finish()
    }
}

/// OIDC token validator with JWKS caching.
#[derive(Debug)]
pub struct OidcValidator {
    config: OidcConfig,
    issuers: HashMap<String, IssuerConfig>,
    jwks_cache: Arc<RwLock<HashMap<String, CachedJwks>>>,
}

impl OidcValidator {
    pub async fn new(config: OidcConfig) -> Result<Self> {
        let issuers: HashMap<String, IssuerConfig> = config.issuers.iter().map(|c| (c.issuer.clone(), c.clone())).collect();
        info!(issuers = ?issuers.keys().collect::<Vec<_>>(), "OIDC validator initialized");
        Ok(Self { config, issuers, jwks_cache: Arc::new(RwLock::new(HashMap::new())) })
    }

    pub async fn validate_token(&self, token: &str) -> Result<ValidatedClaims> {
        let header = jsonwebtoken::decode_header(token).map_err(|e| OidcError::MalformedToken(e.to_string()))?;
        let kid = header.kid.as_ref().ok_or_else(|| OidcError::MalformedToken("missing kid".into()))?;
        let unvalidated = self.peek_claims(token)?;
        let issuer = &unvalidated.iss;
        let issuer_config = self.issuers.get(issuer).ok_or_else(|| OidcError::UntrustedIssuer(issuer.clone()))?;
        let decoding_key = self.get_signing_key(issuer, kid).await?;

        let mut validation = Validation::new(header.alg);
        validation.set_audience(&[&issuer_config.audience]);
        validation.set_issuer(&[issuer]);
        validation.leeway = self.config.clock_skew_tolerance.as_secs();

        let token_data: TokenData<JwtClaims> = jsonwebtoken::decode(token, &decoding_key, &validation).map_err(|e| self.map_jwt_error(e))?;
        let claims = token_data.claims;

        if !claims.aud.contains(&issuer_config.audience) {
            return Err(OidcError::InvalidAudience {
                expected: issuer_config.audience.clone(),
                actual: claims.aud.first().unwrap_or("").to_string(),
            });
        }
        if issuer_config.require_email_verified && claims.email_verified != Some(true) {
            return Err(OidcError::Validation("email not verified".into()));
        }

        Ok(ValidatedClaims {
            subject: claims.sub,
            issuer: claims.iss,
            audience: claims.aud.first().unwrap_or("").to_string(),
            expires_at: DateTime::from_timestamp(claims.exp, 0).unwrap_or_else(Utc::now),
            issued_at: DateTime::from_timestamp(claims.iat.unwrap_or(0), 0).unwrap_or_else(Utc::now),
            email: claims.email,
            email_verified: claims.email_verified,
            name: claims.name,
            custom_claims: claims.extra,
        })
    }

    pub fn is_token_expired(&self, token: &str) -> bool {
        match self.peek_claims(token) {
            Ok(claims) => claims.exp < Utc::now().timestamp(),
            Err(_) => true,
        }
    }

    fn peek_claims(&self, token: &str) -> Result<JwtClaims> {
        let parts: Vec<&str> = token.split('.').collect();
        if parts.len() != 3 { return Err(OidcError::MalformedToken("invalid format".into())); }
        let payload = base64::Engine::decode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, parts[1])
            .map_err(|e| OidcError::MalformedToken(format!("base64: {e}")))?;
        serde_json::from_slice(&payload).map_err(|e| OidcError::MalformedToken(format!("json: {e}")))
    }

    async fn get_signing_key(&self, issuer: &str, kid: &str) -> Result<DecodingKey> {
        {
            let cache = self.jwks_cache.read().await;
            if let Some(cached) = cache.get(issuer) {
                if cached.fetched_at.elapsed() < self.config.jwks_cache_duration {
                    if let Some(key) = cached.keys.get(kid) { return Ok(key.clone()); }
                }
            }
        }
        self.refresh_jwks(issuer).await?;
        let cache = self.jwks_cache.read().await;
        cache.get(issuer).and_then(|c| c.keys.get(kid).cloned()).ok_or_else(|| OidcError::KeyNotFound(kid.to_string()))
    }

    async fn refresh_jwks(&self, issuer: &str) -> Result<()> {
        let issuer_config = self.issuers.get(issuer).ok_or_else(|| OidcError::UntrustedIssuer(issuer.to_string()))?;
        let jwks_uri = issuer_config.jwks_uri.clone()
            .unwrap_or_else(|| format!("{}/.well-known/jwks.json", issuer.trim_end_matches('/')));
        debug!(issuer, uri = %jwks_uri, "Fetching JWKS");

        let resp = reqwest::get(&jwks_uri).await.map_err(|e| OidcError::JwksFetchFailed(e.to_string()))?;
        let jwks: serde_json::Value = resp.json().await.map_err(|e| OidcError::JwksFetchFailed(e.to_string()))?;

        let mut keys = HashMap::new();
        if let Some(key_array) = jwks["keys"].as_array() {
            for jwk in key_array {
                if let (Some(kid), Some(n), Some(e)) = (jwk["kid"].as_str(), jwk["n"].as_str(), jwk["e"].as_str()) {
                    if let Ok(key) = DecodingKey::from_rsa_components(n, e) {
                        keys.insert(kid.to_string(), key);
                    }
                }
            }
        }

        let mut cache = self.jwks_cache.write().await;
        cache.insert(issuer.to_string(), CachedJwks { keys, fetched_at: Instant::now() });
        Ok(())
    }

    fn map_jwt_error(&self, err: jsonwebtoken::errors::Error) -> OidcError {
        use jsonwebtoken::errors::ErrorKind;
        match err.kind() {
            ErrorKind::ExpiredSignature => OidcError::TokenExpired,
            ErrorKind::ImmatureSignature => OidcError::TokenNotYetValid,
            ErrorKind::InvalidSignature => OidcError::InvalidSignature,
            ErrorKind::InvalidAudience => OidcError::InvalidAudience { expected: "configured".into(), actual: "token".into() },
            ErrorKind::InvalidIssuer => OidcError::UntrustedIssuer("from token".into()),
            _ => OidcError::Validation(err.to_string()),
        }
    }

    pub fn add_issuer(&mut self, config: IssuerConfig) {
        self.issuers.insert(config.issuer.clone(), config);
    }

    pub fn remove_issuer(&mut self, issuer: &str) {
        self.issuers.remove(issuer);
    }

    pub fn trusted_issuers(&self) -> Vec<&str> {
        self.issuers.keys().map(String::as_str).collect()
    }
}

/// Builder for OIDC validators.
#[derive(Debug, Default)]
pub struct OidcValidatorBuilder {
    issuers: Vec<IssuerConfig>,
    jwks_cache_duration: Option<Duration>,
    clock_skew_tolerance: Option<Duration>,
}

impl OidcValidatorBuilder {
    pub fn new() -> Self { Self::default() }

    pub fn with_issuer(mut self, config: IssuerConfig) -> Self { self.issuers.push(config); self }

    pub fn with_simple_issuer(mut self, issuer: impl Into<String>, audience: impl Into<String>) -> Self {
        self.issuers.push(IssuerConfig { issuer: issuer.into(), audience: audience.into(), ..Default::default() });
        self
    }

    pub fn with_cache_duration(mut self, duration: Duration) -> Self { self.jwks_cache_duration = Some(duration); self }
    pub fn with_clock_skew(mut self, tolerance: Duration) -> Self { self.clock_skew_tolerance = Some(tolerance); self }

    pub async fn build(self) -> Result<OidcValidator> {
        OidcValidator::new(OidcConfig {
            issuers: self.issuers,
            jwks_cache_duration: self.jwks_cache_duration.unwrap_or(Duration::from_secs(3600)),
            clock_skew_tolerance: self.clock_skew_tolerance.unwrap_or(Duration::from_secs(60)),
        }).await
    }
}

// ============================================================================
// OIDC Discovery and Pipeline Workload Identity
// ============================================================================

/// OIDC Discovery document served at /.well-known/openid-configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OidcDiscoveryDocument {
    pub issuer: String,
    pub jwks_uri: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub response_types_supported: Vec<String>,
    pub subject_types_supported: Vec<String>,
    pub id_token_signing_alg_values_supported: Vec<String>,
    pub claims_supported: Vec<String>,
}

impl OidcDiscoveryDocument {
    /// Generate the discovery document for a given base URL.
    pub fn generate(base_url: &str) -> Self {
        let base = base_url.trim_end_matches('/');
        Self {
            issuer: base.to_string(),
            jwks_uri: format!("{base}/.well-known/jwks.json"),
            authorization_endpoint: format!("{base}/oauth/authorize"),
            token_endpoint: format!("{base}/oauth/token"),
            response_types_supported: vec!["id_token".into()],
            subject_types_supported: vec!["public".into()],
            id_token_signing_alg_values_supported: vec!["RS256".into(), "ES256".into()],
            claims_supported: vec![
                "sub".into(), "iss".into(), "aud".into(), "exp".into(), "iat".into(),
                "org_id".into(), "project_id".into(), "pipeline_id".into(), "run_id".into(),
                "job_id".into(), "ref".into(), "sha".into(), "trigger".into(),
                "runner_os".into(), "runner_arch".into(),
            ],
        }
    }
}

/// Claims for a pipeline workload identity token.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineIdentityClaims {
    pub sub: String,
    pub iss: String,
    pub aud: String,
    pub exp: i64,
    pub iat: i64,
    pub jti: String,
    pub org_id: String,
    pub project_id: String,
    pub pipeline_id: String,
    pub run_id: String,
    pub job_id: String,
    #[serde(rename = "ref")]
    pub git_ref: Option<String>,
    pub sha: Option<String>,
    pub trigger: Option<String>,
    pub runner_os: Option<String>,
    pub runner_arch: Option<String>,
}

/// A signing key for OIDC tokens, with rotation support.
#[derive(Clone)]
pub struct SigningKey {
    pub kid: String,
    pub algorithm: Algorithm,
    pub encoding_key: Arc<EncodingKey>,
    pub active_from: DateTime<Utc>,
    pub active_until: Option<DateTime<Utc>>,
}

impl std::fmt::Debug for SigningKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SigningKey")
            .field("kid", &self.kid)
            .field("algorithm", &self.algorithm)
            .field("encoding_key", &"<redacted>")
            .field("active_from", &self.active_from)
            .field("active_until", &self.active_until)
            .finish()
    }
}

/// Pipeline OIDC token issuer.
///
/// Issues signed OIDC JWTs for pipeline runs, enabling zero-credential access
/// to external systems (AWS Roles Anywhere, Vault JWT auth, GCP WIF).
#[derive(Debug)]
pub struct PipelineTokenIssuer {
    issuer_url: String,
    audience: String,
    signing_keys: Arc<RwLock<Vec<SigningKey>>>,
    token_lifetime: Duration,
}

impl PipelineTokenIssuer {
    pub fn new(issuer_url: String, audience: String, token_lifetime: Duration) -> Self {
        Self {
            issuer_url, audience, token_lifetime,
            signing_keys: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Add a signing key.
    pub async fn add_signing_key(&self, key: SigningKey) {
        self.signing_keys.write().await.push(key);
        info!(kid = %self.signing_keys.read().await.last().map(|k| k.kid.as_str()).unwrap_or("?"), "Added signing key");
    }

    /// Get the currently active signing key.
    async fn active_key(&self) -> Result<SigningKey> {
        let now = Utc::now();
        let keys = self.signing_keys.read().await;
        keys.iter()
            .filter(|k| k.active_from <= now && k.active_until.map_or(true, |u| u > now))
            .last()
            .cloned()
            .ok_or_else(|| OidcError::Validation("no active signing key".into()))
    }

    /// Issue a pipeline identity token.
    pub async fn issue_token(&self, claims: PipelineIdentityClaims) -> Result<String> {
        let key = self.active_key().await?;
        let mut header = Header::new(key.algorithm);
        header.kid = Some(key.kid.clone());

        let token = jsonwebtoken::encode(&header, &claims, &key.encoding_key)
            .map_err(|e| OidcError::Validation(format!("token signing failed: {e}")))?;

        debug!(sub = %claims.sub, kid = %key.kid, "Issued pipeline identity token");
        Ok(token)
    }

    /// Build claims for a pipeline run.
    pub fn build_claims(
        &self,
        org_id: &str,
        project_id: &str,
        pipeline_id: &str,
        run_id: &str,
        job_id: &str,
    ) -> PipelineIdentityClaims {
        let now = Utc::now();
        PipelineIdentityClaims {
            sub: format!("pipeline:{org_id}:{project_id}:{pipeline_id}"),
            iss: self.issuer_url.clone(),
            aud: self.audience.clone(),
            exp: (now + chrono::Duration::from_std(self.token_lifetime).unwrap_or(chrono::Duration::hours(1))).timestamp(),
            iat: now.timestamp(),
            jti: uuid::Uuid::now_v7().to_string(),
            org_id: org_id.to_string(),
            project_id: project_id.to_string(),
            pipeline_id: pipeline_id.to_string(),
            run_id: run_id.to_string(),
            job_id: job_id.to_string(),
            git_ref: None,
            sha: None,
            trigger: None,
            runner_os: None,
            runner_arch: None,
        }
    }

    /// Generate the JWKS document for public key distribution.
    pub async fn jwks_document(&self) -> serde_json::Value {
        // In production, this would serialize the public keys in JWK format.
        // For now, return an empty keyset that can be populated.
        let keys = self.signing_keys.read().await;
        let key_entries: Vec<serde_json::Value> = keys.iter().map(|k| {
            serde_json::json!({
                "kid": k.kid,
                "kty": "RSA",
                "alg": format!("{:?}", k.algorithm),
                "use": "sig",
            })
        }).collect();
        serde_json::json!({ "keys": key_entries })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_empty_validator() {
        let validator = OidcValidator::new(OidcConfig::default()).await.unwrap();
        assert!(validator.trusted_issuers().is_empty());
    }

    #[tokio::test]
    async fn test_add_issuer() {
        let mut validator = OidcValidator::new(OidcConfig::default()).await.unwrap();
        validator.add_issuer(IssuerConfig { issuer: "https://auth.example.com".into(), audience: "api".into(), ..Default::default() });
        assert_eq!(validator.trusted_issuers(), vec!["https://auth.example.com"]);
    }

    #[test]
    fn test_discovery_document() {
        let doc = OidcDiscoveryDocument::generate("https://meticulous.example.com");
        assert_eq!(doc.issuer, "https://meticulous.example.com");
        assert_eq!(doc.jwks_uri, "https://meticulous.example.com/.well-known/jwks.json");
        assert!(doc.claims_supported.contains(&"pipeline_id".to_string()));
    }

    #[tokio::test]
    async fn test_pipeline_token_issuer_build_claims() {
        let issuer = PipelineTokenIssuer::new(
            "https://ci.example.com".into(), "sts.amazonaws.com".into(), Duration::from_secs(3600),
        );
        let claims = issuer.build_claims("org1", "proj1", "pipe1", "run1", "job1");
        assert_eq!(claims.sub, "pipeline:org1:proj1:pipe1");
        assert_eq!(claims.aud, "sts.amazonaws.com");
        assert!(!claims.jti.is_empty());
    }

    #[test]
    fn test_audience_claim() {
        let single = AudienceClaim::Single("api".into());
        assert!(single.contains("api"));
        assert!(!single.contains("other"));

        let multi = AudienceClaim::Multiple(vec!["a".into(), "b".into()]);
        assert!(multi.contains("a"));
        assert!(multi.contains("b"));
        assert!(!multi.contains("c"));
    }
}
