//! Traits for secrets providers.
//!
//! This module defines the core trait that all secrets providers must implement,
//! enabling a pluggable architecture for integrating with different secret backends.

use async_trait::async_trait;
use std::fmt::Debug;

use crate::error::{Result, SecretsError};
use crate::types::{ProviderType, SecretMetadata, SecretValue};

/// A provider for retrieving secrets from an external backend.
///
/// Implementations of this trait connect to specific secret management systems
/// like HashiCorp Vault, AWS Secrets Manager, or Kubernetes secrets.
///
/// # Thread Safety
///
/// Providers must be `Send + Sync` to allow concurrent access from multiple
/// async tasks. Implementations should handle their own internal synchronization.
///
/// # Example
///
/// ```ignore
/// let provider = VaultProvider::new(config).await?;
/// let secret = provider.get_secret("secret/myapp/api-key").await?;
/// println!("Got secret with {} bytes", secret.len());
/// ```
#[async_trait]
pub trait SecretsProvider: Send + Sync + Debug {
    /// Retrieve a secret by its path.
    ///
    /// The path format is provider-specific:
    /// - Vault: `secret/data/myapp/config` or `secret/myapp/config`
    /// - AWS SM: `myapp/production/db-password`
    /// - K8s: `namespace/secret-name/key`
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The secret doesn't exist ([`SecretsError::NotFound`])
    /// - Access is denied ([`SecretsError::AccessDenied`])
    /// - The provider is unavailable ([`SecretsError::ProviderUnavailable`])
    async fn get_secret(&self, path: &str) -> Result<SecretValue>;

    /// Retrieve a specific version of a secret.
    ///
    /// Not all providers support versioning. If the provider doesn't support
    /// versions, this method should ignore the version parameter and return
    /// the current value.
    async fn get_secret_version(&self, path: &str, version: &str) -> Result<SecretValue> {
        // Default implementation ignores version
        let _ = version;
        self.get_secret(path).await
    }

    /// List secret paths under a given prefix.
    ///
    /// Returns a list of secret paths (not values) that match the prefix.
    /// This is useful for discovering available secrets.
    ///
    /// # Arguments
    ///
    /// * `prefix` - The path prefix to list under (e.g., `secret/myapp/`)
    ///
    /// # Errors
    ///
    /// Returns an error if listing is not supported or access is denied.
    async fn list_secrets(&self, prefix: &str) -> Result<Vec<String>>;

    /// Get the type of this provider.
    fn provider_type(&self) -> ProviderType;

    /// Get the provider's name/identifier for logging and debugging.
    fn provider_name(&self) -> &str {
        self.provider_type().as_str()
    }

    /// Check if the provider is healthy and can serve requests.
    ///
    /// This can be used for health checks and readiness probes.
    async fn health_check(&self) -> Result<()> {
        // Default implementation just returns Ok
        Ok(())
    }

    /// Get metadata about a secret without retrieving its value.
    ///
    /// This is useful for checking version information or expiration
    /// without loading the actual secret data.
    async fn get_secret_metadata(&self, path: &str) -> Result<SecretMetadata> {
        // Default implementation fetches the secret and returns its metadata
        let secret = self.get_secret(path).await?;
        Ok(secret.metadata().clone())
    }
}

/// Extension trait for providers that support writing secrets.
///
/// Not all providers allow writing (e.g., read-only Kubernetes access).
/// This trait is separate to allow for read-only provider implementations.
#[async_trait]
pub trait SecretsWriter: SecretsProvider {
    /// Store a secret at the given path.
    ///
    /// If a secret already exists at this path, it will be updated
    /// (and a new version created if the provider supports versioning).
    ///
    /// # Arguments
    ///
    /// * `path` - Where to store the secret
    /// * `value` - The secret value to store
    ///
    /// # Returns
    ///
    /// The metadata of the newly created secret (including version if applicable).
    async fn put_secret(&self, path: &str, value: &SecretValue) -> Result<SecretMetadata>;

    /// Delete a secret at the given path.
    ///
    /// Some providers support soft-delete with recovery, others permanently
    /// delete. Check provider documentation for specific behavior.
    async fn delete_secret(&self, path: &str) -> Result<()>;
}

/// A broker that routes secret requests to the appropriate provider.
///
/// The secrets broker maintains a registry of configured providers and
/// routes requests based on the secret path or explicit provider selection.
#[async_trait]
pub trait SecretsBroker: Send + Sync {
    /// Get a secret using the default provider routing.
    async fn get_secret(&self, path: &str) -> Result<SecretValue>;

    /// Get a secret from a specific provider.
    async fn get_secret_from(&self, provider: ProviderType, path: &str) -> Result<SecretValue>;

    /// List available providers.
    fn available_providers(&self) -> Vec<ProviderType>;

    /// Check if a specific provider is configured and healthy.
    async fn is_provider_available(&self, provider: ProviderType) -> bool;

    /// Resolve secrets for a job execution context.
    ///
    /// Given a list of secret references, fetch all values and return them
    /// as a map. This is the main entry point for job execution.
    async fn resolve_secrets(
        &self,
        refs: &[SecretRef],
    ) -> Result<std::collections::HashMap<String, SecretValue>>;
}

/// A reference to a secret that needs to be resolved.
///
/// This is used when requesting multiple secrets at once, such as
/// during job execution setup.
#[derive(Debug, Clone)]
pub struct SecretRef {
    /// The environment variable name to expose the secret as.
    pub env_name: String,
    /// The provider to fetch from (None for default routing).
    pub provider: Option<ProviderType>,
    /// The path within the provider.
    pub path: String,
    /// Optional key within the secret (for JSON/map secrets).
    pub key: Option<String>,
}

impl SecretRef {
    /// Create a new secret reference.
    pub fn new(env_name: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            env_name: env_name.into(),
            provider: None,
            path: path.into(),
            key: None,
        }
    }

    /// Specify the provider to use.
    pub fn with_provider(mut self, provider: ProviderType) -> Self {
        self.provider = Some(provider);
        self
    }

    /// Specify a key within the secret.
    pub fn with_key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }
}

/// Configuration for creating a secrets provider.
#[derive(Debug, Clone)]
pub struct ProviderConfig {
    /// The type of provider.
    pub provider_type: ProviderType,
    /// Provider-specific configuration as key-value pairs.
    pub settings: std::collections::HashMap<String, String>,
}

impl ProviderConfig {
    /// Create a new provider configuration.
    pub fn new(provider_type: ProviderType) -> Self {
        Self {
            provider_type,
            settings: std::collections::HashMap::new(),
        }
    }

    /// Add a configuration setting.
    pub fn with_setting(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.settings.insert(key.into(), value.into());
        self
    }

    /// Get a configuration setting.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.settings.get(key).map(String::as_str)
    }

    /// Get a required configuration setting.
    pub fn require(&self, key: &str) -> Result<&str> {
        self.get(key)
            .ok_or_else(|| SecretsError::Configuration(format!("missing required config: {key}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secret_ref_builder() {
        let secret_ref = SecretRef::new("DATABASE_URL", "secret/myapp/db")
            .with_provider(ProviderType::Vault)
            .with_key("connection_string");

        assert_eq!(secret_ref.env_name, "DATABASE_URL");
        assert_eq!(secret_ref.provider, Some(ProviderType::Vault));
        assert_eq!(secret_ref.path, "secret/myapp/db");
        assert_eq!(secret_ref.key.as_deref(), Some("connection_string"));
    }

    #[test]
    fn test_provider_config() {
        let config = ProviderConfig::new(ProviderType::Vault)
            .with_setting("address", "https://vault.example.com")
            .with_setting("namespace", "admin");

        assert_eq!(config.provider_type, ProviderType::Vault);
        assert_eq!(config.get("address"), Some("https://vault.example.com"));
        assert_eq!(config.get("namespace"), Some("admin"));
        assert!(config.get("nonexistent").is_none());
    }
}
