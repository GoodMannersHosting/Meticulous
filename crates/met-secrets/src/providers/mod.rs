//! Secrets provider implementations.
//!
//! This module contains implementations of the [`SecretsProvider`](crate::SecretsProvider)
//! trait for various secret management backends.
//!
//! # Available Providers
//!
//! - [`vault::VaultProvider`] - HashiCorp Vault / OpenBao
//! - [`aws::AwsSecretsProvider`] - AWS Secrets Manager
//! - [`k8s::KubernetesSecretsProvider`] - Kubernetes secrets
//! - [`builtin::BuiltinSecretsProvider`] - Built-in encrypted storage

pub mod aws;
pub mod builtin;
pub mod k8s;
pub mod vault;

pub use aws::AwsSecretsProvider;
pub use builtin::BuiltinSecretsProvider;
pub use k8s::KubernetesSecretsProvider;
pub use vault::VaultProvider;

use std::collections::HashMap;
use std::sync::Arc;

use crate::error::{Result, SecretsError};
use crate::traits::{SecretRef, SecretsBroker, SecretsProvider};
use crate::types::{ProviderType, SecretValue};

/// A multi-provider secrets broker.
///
/// Routes secret requests to the appropriate provider based on configuration
/// or explicit provider selection.
#[derive(Debug)]
pub struct MultiProviderBroker {
    providers: HashMap<ProviderType, Arc<dyn SecretsProvider>>,
    default_provider: Option<ProviderType>,
}

impl MultiProviderBroker {
    /// Create a new broker with no providers configured.
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
            default_provider: None,
        }
    }

    /// Register a provider.
    pub fn register_provider(
        &mut self,
        provider: Arc<dyn SecretsProvider>,
    ) {
        let provider_type = provider.provider_type();
        self.providers.insert(provider_type, provider);
    }

    /// Set the default provider for requests without explicit provider.
    pub fn set_default_provider(&mut self, provider_type: ProviderType) {
        self.default_provider = Some(provider_type);
    }

    /// Get a provider by type.
    pub fn get_provider(&self, provider_type: ProviderType) -> Option<&Arc<dyn SecretsProvider>> {
        self.providers.get(&provider_type)
    }

    /// Create a builder for configuring the broker.
    pub fn builder() -> MultiProviderBrokerBuilder {
        MultiProviderBrokerBuilder::new()
    }
}

impl Default for MultiProviderBroker {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl SecretsBroker for MultiProviderBroker {
    async fn get_secret(&self, path: &str) -> Result<SecretValue> {
        let provider_type = self.default_provider.ok_or_else(|| {
            SecretsError::Configuration("no default provider configured".into())
        })?;
        self.get_secret_from(provider_type, path).await
    }

    async fn get_secret_from(&self, provider: ProviderType, path: &str) -> Result<SecretValue> {
        let provider = self.providers.get(&provider).ok_or_else(|| {
            SecretsError::provider_unavailable(provider.as_str(), "provider not configured")
        })?;
        provider.get_secret(path).await
    }

    fn available_providers(&self) -> Vec<ProviderType> {
        self.providers.keys().copied().collect()
    }

    async fn is_provider_available(&self, provider: ProviderType) -> bool {
        if let Some(p) = self.providers.get(&provider) {
            p.health_check().await.is_ok()
        } else {
            false
        }
    }

    async fn resolve_secrets(
        &self,
        refs: &[SecretRef],
    ) -> Result<HashMap<String, SecretValue>> {
        let mut results = HashMap::with_capacity(refs.len());

        for secret_ref in refs {
            let provider_type = secret_ref
                .provider
                .or(self.default_provider)
                .ok_or_else(|| {
                    SecretsError::Configuration(format!(
                        "no provider specified for {} and no default configured",
                        secret_ref.env_name
                    ))
                })?;

            let provider = self.providers.get(&provider_type).ok_or_else(|| {
                SecretsError::provider_unavailable(
                    provider_type.as_str(),
                    "provider not configured",
                )
            })?;

            let secret = provider.get_secret(&secret_ref.path).await?;

            // If a key is specified, we'd extract it from a JSON value
            // For now, we just use the full value
            // TODO: Implement JSON key extraction
            if secret_ref.key.is_some() {
                tracing::warn!(
                    env = %secret_ref.env_name,
                    "JSON key extraction not yet implemented, using full secret value"
                );
            }

            results.insert(secret_ref.env_name.clone(), secret);
        }

        Ok(results)
    }
}

/// Builder for [`MultiProviderBroker`].
#[derive(Debug, Default)]
pub struct MultiProviderBrokerBuilder {
    providers: Vec<Arc<dyn SecretsProvider>>,
    default_provider: Option<ProviderType>,
}

impl MultiProviderBrokerBuilder {
    /// Create a new builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a provider.
    pub fn with_provider(mut self, provider: Arc<dyn SecretsProvider>) -> Self {
        self.providers.push(provider);
        self
    }

    /// Set the default provider type.
    pub fn with_default(mut self, provider_type: ProviderType) -> Self {
        self.default_provider = Some(provider_type);
        self
    }

    /// Build the broker.
    pub fn build(self) -> MultiProviderBroker {
        let mut broker = MultiProviderBroker::new();
        for provider in self.providers {
            broker.register_provider(provider);
        }
        if let Some(default) = self.default_provider {
            broker.set_default_provider(default);
        }
        broker
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_broker_no_providers() {
        let broker = MultiProviderBroker::new();
        assert!(broker.available_providers().is_empty());

        let result = broker.get_secret("some/path").await;
        assert!(result.is_err());
    }
}
