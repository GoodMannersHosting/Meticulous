//! JWT token management for agent authentication.

use chrono::{DateTime, Duration, Utc};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use met_core::ids::{AgentId, OrganizationId};
use serde::{Deserialize, Serialize};

use crate::error::{ControllerError, Result};

/// JWT claims for agent authentication.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentClaims {
    /// Subject (agent ID).
    pub sub: String,
    /// Organization ID.
    pub org: String,
    /// Issued at timestamp.
    pub iat: i64,
    /// Expiration timestamp.
    pub exp: i64,
    /// Whether the token is renewable.
    pub renewable: bool,
    /// Agent's pool tags.
    pub pool_tags: Vec<String>,
}

impl AgentClaims {
    /// Check if the token is expired.
    pub fn is_expired(&self) -> bool {
        Utc::now().timestamp() >= self.exp
    }

    /// Get the agent ID.
    pub fn agent_id(&self) -> std::result::Result<AgentId, met_core::ids::IdParseError> {
        self.sub.parse()
    }

    /// Get the organization ID.
    pub fn org_id(&self) -> std::result::Result<OrganizationId, met_core::ids::IdParseError> {
        self.org.parse()
    }

    /// Get the expiration time.
    pub fn expires_at(&self) -> DateTime<Utc> {
        DateTime::from_timestamp(self.exp, 0).unwrap_or_else(Utc::now)
    }
}

/// Manager for JWT token creation and validation.
#[derive(Clone)]
pub struct JwtManager {
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
    validity: Duration,
    renewable: bool,
}

impl JwtManager {
    /// Create a new JWT manager.
    pub fn new(secret: &str, validity: std::time::Duration, renewable: bool) -> Self {
        Self {
            encoding_key: EncodingKey::from_secret(secret.as_bytes()),
            decoding_key: DecodingKey::from_secret(secret.as_bytes()),
            validity: Duration::from_std(validity).unwrap_or(Duration::hours(24)),
            renewable,
        }
    }

    /// Issue a new JWT for an agent.
    pub fn issue(
        &self,
        agent_id: AgentId,
        org_id: OrganizationId,
        pool_tags: Vec<String>,
    ) -> Result<(String, DateTime<Utc>)> {
        let now = Utc::now();
        let expires_at = now + self.validity;

        let claims = AgentClaims {
            sub: agent_id.to_string(),
            org: org_id.to_string(),
            iat: now.timestamp(),
            exp: expires_at.timestamp(),
            renewable: self.renewable,
            pool_tags,
        };

        let token = encode(&Header::default(), &claims, &self.encoding_key)?;
        Ok((token, expires_at))
    }

    /// Validate and decode a JWT.
    pub fn validate(&self, token: &str) -> Result<AgentClaims> {
        let validation = Validation::default();
        let token_data = decode::<AgentClaims>(token, &self.decoding_key, &validation)
            .map_err(|e| ControllerError::InvalidJwt(e.to_string()))?;

        if token_data.claims.is_expired() {
            return Err(ControllerError::JwtExpired);
        }

        Ok(token_data.claims)
    }

    /// Check if a token needs renewal (within 10% of expiry).
    pub fn needs_renewal(&self, claims: &AgentClaims) -> bool {
        if !claims.renewable {
            return false;
        }

        let now = Utc::now().timestamp();
        let remaining = claims.exp - now;
        let threshold = (self.validity.num_seconds() as f64 * 0.1) as i64;

        remaining <= threshold
    }

    /// Renew a token if it's renewable and close to expiry.
    pub fn renew(&self, claims: &AgentClaims) -> Result<Option<(String, DateTime<Utc>)>> {
        if !self.needs_renewal(claims) {
            return Ok(None);
        }

        let agent_id = claims
            .agent_id()
            .map_err(|e| ControllerError::InvalidJwt(e.to_string()))?;
        let org_id = claims
            .org_id()
            .map_err(|e| ControllerError::InvalidJwt(e.to_string()))?;

        let (token, expires_at) = self.issue(agent_id, org_id, claims.pool_tags.clone())?;
        Ok(Some((token, expires_at)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jwt_issue_and_validate() {
        let manager = JwtManager::new(
            "test-secret-that-is-long-enough-for-testing",
            std::time::Duration::from_secs(3600),
            true,
        );

        let agent_id = AgentId::new();
        let org_id = OrganizationId::new();
        let pool_tags = vec!["linux-amd64".to_string()];

        let (token, expires_at) = manager.issue(agent_id, org_id, pool_tags.clone()).unwrap();

        let claims = manager.validate(&token).unwrap();
        assert_eq!(claims.agent_id().unwrap(), agent_id);
        assert_eq!(claims.org_id().unwrap(), org_id);
        assert_eq!(claims.pool_tags, pool_tags);
        assert!(claims.renewable);
        assert!(!claims.is_expired());
        assert!(claims.expires_at() <= expires_at + Duration::seconds(1));
    }

    #[test]
    fn test_jwt_expired() {
        let manager = JwtManager::new(
            "test-secret-that-is-long-enough-for-testing",
            std::time::Duration::from_secs(0), // Immediate expiry
            true,
        );

        let agent_id = AgentId::new();
        let org_id = OrganizationId::new();

        let (token, _) = manager.issue(agent_id, org_id, vec![]).unwrap();

        // Token should be expired immediately
        std::thread::sleep(std::time::Duration::from_millis(10));
        let result = manager.validate(&token);
        assert!(matches!(result, Err(ControllerError::JwtExpired)));
    }
}
