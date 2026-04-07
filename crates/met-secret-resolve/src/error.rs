//! Resolver errors (no secret values).

use thiserror::Error;

/// Secret validation / resolution failures.
#[derive(Debug, Error)]
pub enum ResolveError {
    #[error("pipeline definition could not be parsed: {0}")]
    Parse(String),

    #[error("missing stored secrets: {0:?}")]
    MissingSecrets(Vec<String>),

    #[error("project_id is required for secret resolution")]
    MissingProjectId,

    #[error("stored secret master key is not configured")]
    MissingMasterKey,

    #[error("AWS/Vault secret resolution is not configured for this deployment: {0}")]
    ExternalNotConfigured(String),

    #[error("invalid nonce length for stored secret ciphertext")]
    BadNonce,

    #[error("database error: {0}")]
    Database(#[from] met_store::StoreError),

    #[error("crypto error: {0}")]
    Crypto(String),
}

impl From<met_secrets::SecretsError> for ResolveError {
    fn from(e: met_secrets::SecretsError) -> Self {
        Self::Crypto(e.to_string())
    }
}
