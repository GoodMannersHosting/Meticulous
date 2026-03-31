//! OIDC token validation.
//!
//! This module provides JWT/OIDC token validation with support for multiple
//! issuers and automatic JWKS caching.
//!
//! # Features
//!
//! - Multi-issuer support for federated authentication
//! - Automatic JWKS discovery and caching
//! - Standard claim validation (exp, nbf, iss, aud)
//! - Custom claim extraction
//!
//! # Example
//!
//! ```ignore
//! use met_secrets::oidc::{OidcValidator, OidcConfig, IssuerConfig};
//!
//! let validator = OidcValidator::new(OidcConfig {
//!     issuers: vec![
//!         IssuerConfig {
//!             issuer: "https://auth.example.com".into(),
//!             audience: "meticulous-api".into(),
//!             ..Default::default()
//!         },
//!     ],
//!     ..Default::default()
//! }).await?;
//!
//! let claims = validator.validate_token(token).await?;
//! println!("User: {}", claims.subject);
//! ```

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use jsonwebtoken::{Algorithm, DecodingKey, TokenData, Validation};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::error::OidcError;

/// Result type for OIDC operations.
pub type Result<T> = std::result::Result<T, OidcError>;

/// Configuration for the OIDC validator.
#[derive(Debug, Clone)]
pub struct OidcConfig {
    /// Trusted token issuers.
    pub issuers: Vec<IssuerConfig>,
    /// How long to cache JWKS before refreshing.
    pub jwks_cache_duration: Duration,
    /// Clock skew tolerance for token validation.
    pub clock_skew_tolerance: Duration,
}

impl Default for OidcConfig {
    fn default() -> Self {
        Self {
            issuers: Vec::new(),
            jwks_cache_duration: Duration::from_secs(3600), // 1 hour
            clock_skew_tolerance: Duration::from_secs(60),  // 1 minute
        }
    }
}

/// Configuration for a trusted token issuer.
#[derive(Debug, Clone)]
pub struct IssuerConfig {
    /// The issuer URL (iss claim value).
    pub issuer: String,
    /// Expected audience value.
    pub audience: String,
    /// JWKS URI (auto-discovered if not set).
    pub jwks_uri: Option<String>,
    /// Allowed signing algorithms.
    pub algorithms: Vec<Algorithm>,
    /// Whether to require email verification.
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
    /// Subject (user ID).
    pub subject: String,
    /// Issuer URL.
    pub issuer: String,
    /// Audience.
    pub audience: String,
    /// Token expiration time.
    pub expires_at: DateTime<Utc>,
    /// Token issued at time.
    pub issued_at: DateTime<Utc>,
    /// Email address (if present).
    pub email: Option<String>,
    /// Whether email is verified.
    pub email_verified: Option<bool>,
    /// Name (if present).
    pub name: Option<String>,
    /// Additional custom claims.
    pub custom_claims: HashMap<String, serde_json::Value>,
}

/// Standard JWT claims structure.
#[derive(Debug, Deserialize)]
struct JwtClaims {
    sub: String,
    iss: String,
    aud: AudienceClaim,
    exp: i64,
    iat: Option<i64>,
    nbf: Option<i64>,
    email: Option<String>,
    email_verified: Option<bool>,
    name: Option<String>,
    #[serde(flatten)]
    extra: HashMap<String, serde_json::Value>,
}

/// Audience can be a single string or array of strings.
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

/// Cached JWKS data.
struct CachedJwks {
    keys: HashMap<String, DecodingKey>,
    fetched_at: Instant,
}

impl std::fmt::Debug for CachedJwks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CachedJwks")
            .field("keys", &format!("[{} keys]", self.keys.len()))
            .field("fetched_at", &self.fetched_at)
            .finish()
    }
}

/// OIDC token validator with JWKS caching.
///
/// Validates JWT tokens against configured trusted issuers.
#[derive(Debug)]
pub struct OidcValidator {
    config: OidcConfig,
    /// Issuer configs indexed by issuer URL.
    issuers: HashMap<String, IssuerConfig>,
    /// JWKS cache per issuer.
    jwks_cache: Arc<RwLock<HashMap<String, CachedJwks>>>,
}

impl OidcValidator {
    /// Create a new OIDC validator.
    pub async fn new(config: OidcConfig) -> Result<Self> {
        let issuers: HashMap<String, IssuerConfig> = config
            .issuers
            .iter()
            .map(|c| (c.issuer.clone(), c.clone()))
            .collect();

        tracing::info!(
            issuers = ?issuers.keys().collect::<Vec<_>>(),
            "Initializing OIDC validator"
        );

        Ok(Self {
            config,
            issuers,
            jwks_cache: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Validate a JWT token and extract claims.
    pub async fn validate_token(&self, token: &str) -> Result<ValidatedClaims> {
        // Decode header to get issuer hint and key ID
        let header = jsonwebtoken::decode_header(token)
            .map_err(|e| OidcError::MalformedToken(e.to_string()))?;

        let kid = header
            .kid
            .as_ref()
            .ok_or_else(|| OidcError::MalformedToken("missing kid in header".into()))?;

        // Peek at claims to get issuer (without validation)
        let unvalidated = self.peek_claims(token)?;
        let issuer = &unvalidated.iss;

        // Look up issuer configuration
        let issuer_config = self
            .issuers
            .get(issuer)
            .ok_or_else(|| OidcError::UntrustedIssuer(issuer.clone()))?;

        // Get the signing key
        let decoding_key = self.get_signing_key(issuer, kid).await?;

        // Set up validation
        let mut validation = Validation::new(header.alg);
        validation.set_audience(&[&issuer_config.audience]);
        validation.set_issuer(&[issuer]);
        validation.leeway = self.config.clock_skew_tolerance.as_secs();

        // Validate and decode
        let token_data: TokenData<JwtClaims> =
            jsonwebtoken::decode(token, &decoding_key, &validation)
                .map_err(|e| self.map_jwt_error(e))?;

        let claims = token_data.claims;

        // Verify audience
        if !claims.aud.contains(&issuer_config.audience) {
            return Err(OidcError::InvalidAudience {
                expected: issuer_config.audience.clone(),
                actual: claims.aud.first().unwrap_or("").to_string(),
            });
        }

        // Check email verification if required
        if issuer_config.require_email_verified {
            if claims.email_verified != Some(true) {
                return Err(OidcError::Validation("email not verified".into()));
            }
        }

        // Build validated claims
        let validated = ValidatedClaims {
            subject: claims.sub,
            issuer: claims.iss,
            audience: claims.aud.first().unwrap_or("").to_string(),
            expires_at: DateTime::from_timestamp(claims.exp, 0)
                .unwrap_or_else(Utc::now),
            issued_at: DateTime::from_timestamp(claims.iat.unwrap_or(0), 0)
                .unwrap_or_else(Utc::now),
            email: claims.email,
            email_verified: claims.email_verified,
            name: claims.name,
            custom_claims: claims.extra,
        };

        tracing::debug!(
            subject = %validated.subject,
            issuer = %validated.issuer,
            "Token validated successfully"
        );

        Ok(validated)
    }

    /// Check if a token is valid without full validation.
    ///
    /// This is faster than full validation but doesn't verify signatures.
    /// Use for quick expiration checks only.
    pub fn is_token_expired(&self, token: &str) -> bool {
        match self.peek_claims(token) {
            Ok(claims) => {
                let now = Utc::now().timestamp();
                claims.exp < now
            }
            Err(_) => true, // Treat invalid tokens as expired
        }
    }

    /// Get claims from a token without signature verification.
    fn peek_claims(&self, token: &str) -> Result<JwtClaims> {
        let parts: Vec<&str> = token.split('.').collect();
        if parts.len() != 3 {
            return Err(OidcError::MalformedToken("invalid token format".into()));
        }

        let payload = base64::Engine::decode(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD,
            parts[1],
        )
        .map_err(|e| OidcError::MalformedToken(format!("invalid base64: {e}")))?;

        serde_json::from_slice(&payload)
            .map_err(|e| OidcError::MalformedToken(format!("invalid JSON: {e}")))
    }

    /// Get a signing key for the given issuer and key ID.
    async fn get_signing_key(&self, issuer: &str, kid: &str) -> Result<DecodingKey> {
        // Check cache first
        {
            let cache = self.jwks_cache.read().await;
            if let Some(cached) = cache.get(issuer) {
                if cached.fetched_at.elapsed() < self.config.jwks_cache_duration {
                    if let Some(key) = cached.keys.get(kid) {
                        return Ok(key.clone());
                    }
                }
            }
        }

        // Fetch fresh JWKS
        self.refresh_jwks(issuer).await?;

        // Try again
        let cache = self.jwks_cache.read().await;
        cache
            .get(issuer)
            .and_then(|c| c.keys.get(kid).cloned())
            .ok_or_else(|| OidcError::KeyNotFound(kid.to_string()))
    }

    /// Refresh the JWKS cache for an issuer.
    async fn refresh_jwks(&self, issuer: &str) -> Result<()> {
        let issuer_config = self
            .issuers
            .get(issuer)
            .ok_or_else(|| OidcError::UntrustedIssuer(issuer.to_string()))?;

        // Determine JWKS URI
        let jwks_uri = issuer_config
            .jwks_uri
            .clone()
            .unwrap_or_else(|| format!("{}/.well-known/jwks.json", issuer.trim_end_matches('/')));

        tracing::debug!(issuer = %issuer, uri = %jwks_uri, "Fetching JWKS");

        // TODO: Implement actual JWKS fetching
        // Real implementation would:
        // 1. Make HTTP request to jwks_uri
        // 2. Parse JWKS response
        // 3. Convert JWKs to DecodingKeys
        // 4. Cache the results

        // For now, return an error indicating not implemented
        Err(OidcError::JwksFetchFailed(format!(
            "JWKS fetching not yet implemented for {jwks_uri}"
        )))
    }

    /// Map jsonwebtoken errors to our error types.
    fn map_jwt_error(&self, err: jsonwebtoken::errors::Error) -> OidcError {
        use jsonwebtoken::errors::ErrorKind;
        match err.kind() {
            ErrorKind::ExpiredSignature => OidcError::TokenExpired,
            ErrorKind::ImmatureSignature => OidcError::TokenNotYetValid,
            ErrorKind::InvalidSignature => OidcError::InvalidSignature,
            ErrorKind::InvalidAudience => OidcError::InvalidAudience {
                expected: "configured".into(),
                actual: "token".into(),
            },
            ErrorKind::InvalidIssuer => {
                OidcError::UntrustedIssuer("from token".into())
            }
            _ => OidcError::Validation(err.to_string()),
        }
    }

    /// Add a trusted issuer at runtime.
    pub fn add_issuer(&mut self, config: IssuerConfig) {
        self.issuers.insert(config.issuer.clone(), config);
    }

    /// Remove a trusted issuer.
    pub fn remove_issuer(&mut self, issuer: &str) {
        self.issuers.remove(issuer);
    }

    /// Get the list of trusted issuers.
    pub fn trusted_issuers(&self) -> Vec<&str> {
        self.issuers.keys().map(String::as_str).collect()
    }
}

/// Builder for creating OIDC validators.
#[derive(Debug, Default)]
pub struct OidcValidatorBuilder {
    issuers: Vec<IssuerConfig>,
    jwks_cache_duration: Option<Duration>,
    clock_skew_tolerance: Option<Duration>,
}

impl OidcValidatorBuilder {
    /// Create a new builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a trusted issuer.
    pub fn with_issuer(mut self, config: IssuerConfig) -> Self {
        self.issuers.push(config);
        self
    }

    /// Add a simple issuer with just URL and audience.
    pub fn with_simple_issuer(mut self, issuer: impl Into<String>, audience: impl Into<String>) -> Self {
        self.issuers.push(IssuerConfig {
            issuer: issuer.into(),
            audience: audience.into(),
            ..Default::default()
        });
        self
    }

    /// Set the JWKS cache duration.
    pub fn with_cache_duration(mut self, duration: Duration) -> Self {
        self.jwks_cache_duration = Some(duration);
        self
    }

    /// Set the clock skew tolerance.
    pub fn with_clock_skew(mut self, tolerance: Duration) -> Self {
        self.clock_skew_tolerance = Some(tolerance);
        self
    }

    /// Build the validator.
    pub async fn build(self) -> Result<OidcValidator> {
        let config = OidcConfig {
            issuers: self.issuers,
            jwks_cache_duration: self.jwks_cache_duration.unwrap_or(Duration::from_secs(3600)),
            clock_skew_tolerance: self.clock_skew_tolerance.unwrap_or(Duration::from_secs(60)),
        };
        OidcValidator::new(config).await
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
        validator.add_issuer(IssuerConfig {
            issuer: "https://auth.example.com".into(),
            audience: "my-api".into(),
            ..Default::default()
        });
        assert_eq!(validator.trusted_issuers(), vec!["https://auth.example.com"]);
    }

    #[tokio::test]
    async fn test_builder() {
        let validator = OidcValidatorBuilder::new()
            .with_simple_issuer("https://auth.example.com", "my-api")
            .with_cache_duration(Duration::from_secs(7200))
            .build()
            .await
            .unwrap();

        assert!(validator.trusted_issuers().contains(&"https://auth.example.com"));
    }

    #[test]
    fn test_audience_claim_single() {
        let aud = AudienceClaim::Single("my-api".into());
        assert!(aud.contains("my-api"));
        assert!(!aud.contains("other"));
        assert_eq!(aud.first(), Some("my-api"));
    }

    #[test]
    fn test_audience_claim_multiple() {
        let aud = AudienceClaim::Multiple(vec!["api1".into(), "api2".into()]);
        assert!(aud.contains("api1"));
        assert!(aud.contains("api2"));
        assert!(!aud.contains("api3"));
        assert_eq!(aud.first(), Some("api1"));
    }
}
