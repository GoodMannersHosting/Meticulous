//! Secrets provider implementations.
//!
//! This module contains implementations of the [`SecretsProvider`](crate::SecretsProvider)
//! trait for various secret management backends.

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
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use crate::error::{Result, SecretsError};
use crate::traits::{SecretRef, SecretsBroker, SecretsProvider};
use crate::types::{ProviderType, SecretValue};

/// Circuit breaker state for a provider.
#[derive(Debug)]
struct CircuitBreaker {
    failure_count: AtomicU64,
    is_open: AtomicBool,
    last_failure: RwLock<Option<Instant>>,
    threshold: u64,
    recovery_timeout: Duration,
}

impl CircuitBreaker {
    fn new(threshold: u64, recovery_timeout: Duration) -> Self {
        Self {
            failure_count: AtomicU64::new(0),
            is_open: AtomicBool::new(false),
            last_failure: RwLock::new(None),
            threshold,
            recovery_timeout,
        }
    }

    fn record_success(&self) {
        self.failure_count.store(0, Ordering::SeqCst);
        self.is_open.store(false, Ordering::SeqCst);
    }

    async fn record_failure(&self) {
        let count = self.failure_count.fetch_add(1, Ordering::SeqCst) + 1;
        *self.last_failure.write().await = Some(Instant::now());
        if count >= self.threshold {
            self.is_open.store(true, Ordering::SeqCst);
        }
    }

    async fn is_available(&self) -> bool {
        if !self.is_open.load(Ordering::SeqCst) {
            return true;
        }
        let last = self.last_failure.read().await;
        if let Some(last_failure) = *last {
            if last_failure.elapsed() > self.recovery_timeout {
                return true; // Half-open: allow a trial request
            }
        }
        false
    }
}

/// Configuration for the multi-provider broker.
#[derive(Debug, Clone)]
pub struct BrokerConfig {
    /// Maximum retry attempts for retryable errors.
    pub max_retries: u32,
    /// Base delay between retries (exponential backoff).
    pub retry_base_delay: Duration,
    /// Circuit breaker failure threshold.
    pub circuit_breaker_threshold: u64,
    /// Circuit breaker recovery timeout.
    pub circuit_breaker_recovery: Duration,
    /// Request timeout per provider call.
    pub request_timeout: Duration,
}

impl Default for BrokerConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            retry_base_delay: Duration::from_millis(200),
            circuit_breaker_threshold: 5,
            circuit_breaker_recovery: Duration::from_secs(30),
            request_timeout: Duration::from_secs(10),
        }
    }
}

/// A multi-provider secrets broker with retry logic and circuit breakers.
#[derive(Debug)]
pub struct MultiProviderBroker {
    providers: HashMap<ProviderType, Arc<dyn SecretsProvider>>,
    circuit_breakers: HashMap<ProviderType, Arc<CircuitBreaker>>,
    default_provider: Option<ProviderType>,
    config: BrokerConfig,
}

impl MultiProviderBroker {
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
            circuit_breakers: HashMap::new(),
            default_provider: None,
            config: BrokerConfig::default(),
        }
    }

    pub fn with_config(config: BrokerConfig) -> Self {
        Self {
            providers: HashMap::new(),
            circuit_breakers: HashMap::new(),
            default_provider: None,
            config,
        }
    }

    pub fn register_provider(&mut self, provider: Arc<dyn SecretsProvider>) {
        let provider_type = provider.provider_type();
        let cb = Arc::new(CircuitBreaker::new(
            self.config.circuit_breaker_threshold,
            self.config.circuit_breaker_recovery,
        ));
        self.circuit_breakers.insert(provider_type, cb);
        self.providers.insert(provider_type, provider);
    }

    pub fn set_default_provider(&mut self, provider_type: ProviderType) {
        self.default_provider = Some(provider_type);
    }

    pub fn get_provider(&self, provider_type: ProviderType) -> Option<&Arc<dyn SecretsProvider>> {
        self.providers.get(&provider_type)
    }

    pub fn builder() -> MultiProviderBrokerBuilder {
        MultiProviderBrokerBuilder::new()
    }

    /// Pre-flight validation: check that all secret references can be resolved.
    ///
    /// This runs before job dispatch to fail fast if any secrets are unresolvable.
    pub async fn preflight_check(&self, refs: &[SecretRef]) -> Result<()> {
        for secret_ref in refs {
            let provider_type = secret_ref
                .provider
                .or(self.default_provider)
                .ok_or_else(|| {
                    SecretsError::Configuration(format!(
                        "no provider for secret '{}' and no default configured",
                        secret_ref.env_name
                    ))
                })?;

            if !self.providers.contains_key(&provider_type) {
                return Err(SecretsError::provider_unavailable(
                    provider_type.as_str(),
                    format!(
                        "provider not configured for secret '{}'",
                        secret_ref.env_name
                    ),
                ));
            }

            let cb = self.circuit_breakers.get(&provider_type);
            if let Some(cb) = cb {
                if !cb.is_available().await {
                    return Err(SecretsError::provider_unavailable(
                        provider_type.as_str(),
                        "circuit breaker open",
                    ));
                }
            }

            let provider = &self.providers[&provider_type];
            match provider.get_secret_metadata(&secret_ref.path).await {
                Ok(_) => debug!(env = %secret_ref.env_name, "preflight: secret exists"),
                Err(SecretsError::NotFound { .. }) => {
                    return Err(SecretsError::NotFound {
                        path: format!("{}:{}", provider_type, secret_ref.path),
                    });
                }
                Err(e) if e.is_retryable() => {
                    warn!(
                        env = %secret_ref.env_name,
                        error = %e,
                        "preflight: retryable error checking secret, proceeding"
                    );
                }
                Err(e) => return Err(e),
            }
        }

        info!(count = refs.len(), "preflight check passed for all secrets");
        Ok(())
    }

    async fn get_with_retry(
        &self,
        provider: &Arc<dyn SecretsProvider>,
        cb: &CircuitBreaker,
        path: &str,
    ) -> Result<SecretValue> {
        let mut last_error = None;

        for attempt in 0..=self.config.max_retries {
            if attempt > 0 {
                let delay = self.config.retry_base_delay * 2u32.saturating_pow(attempt - 1);
                debug!(
                    attempt,
                    delay_ms = delay.as_millis(),
                    path,
                    "retrying secret fetch"
                );
                tokio::time::sleep(delay).await;
            }

            if !cb.is_available().await {
                return Err(SecretsError::provider_unavailable(
                    provider.provider_name(),
                    "circuit breaker open",
                ));
            }

            match tokio::time::timeout(self.config.request_timeout, provider.get_secret(path)).await
            {
                Ok(Ok(value)) => {
                    cb.record_success();
                    return Ok(value);
                }
                Ok(Err(e)) if e.is_retryable() && attempt < self.config.max_retries => {
                    cb.record_failure().await;
                    warn!(attempt, error = %e, path, "retryable error fetching secret");
                    last_error = Some(e);
                }
                Ok(Err(e)) => {
                    cb.record_failure().await;
                    return Err(e);
                }
                Err(_) => {
                    cb.record_failure().await;
                    let err = SecretsError::Timeout {
                        provider: provider.provider_name().to_string(),
                        timeout_secs: self.config.request_timeout.as_secs(),
                    };
                    if attempt < self.config.max_retries {
                        last_error = Some(err);
                    } else {
                        return Err(err);
                    }
                }
            }
        }

        Err(last_error.unwrap_or_else(|| SecretsError::ProviderError {
            provider: "unknown".into(),
            message: "retry exhausted".into(),
        }))
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
        let provider_type = self
            .default_provider
            .ok_or_else(|| SecretsError::Configuration("no default provider configured".into()))?;
        self.get_secret_from(provider_type, path).await
    }

    async fn get_secret_from(&self, provider: ProviderType, path: &str) -> Result<SecretValue> {
        let provider_arc = self.providers.get(&provider).ok_or_else(|| {
            SecretsError::provider_unavailable(provider.as_str(), "provider not configured")
        })?;
        let cb = self.circuit_breakers.get(&provider).ok_or_else(|| {
            SecretsError::provider_unavailable(provider.as_str(), "no circuit breaker")
        })?;
        self.get_with_retry(provider_arc, cb, path).await
    }

    fn available_providers(&self) -> Vec<ProviderType> {
        self.providers.keys().copied().collect()
    }

    async fn is_provider_available(&self, provider: ProviderType) -> bool {
        if let Some(p) = self.providers.get(&provider) {
            if let Some(cb) = self.circuit_breakers.get(&provider) {
                cb.is_available().await && p.health_check().await.is_ok()
            } else {
                p.health_check().await.is_ok()
            }
        } else {
            false
        }
    }

    async fn resolve_secrets(&self, refs: &[SecretRef]) -> Result<HashMap<String, SecretValue>> {
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

            let value = self
                .get_secret_from(provider_type, &secret_ref.path)
                .await?;

            if let Some(key) = &secret_ref.key {
                let json_val: serde_json::Value = serde_json::from_str(value.expose_secret())
                    .map_err(|e| SecretsError::InvalidFormat {
                        message: format!("secret is not valid JSON for key extraction: {e}"),
                    })?;
                let extracted = json_val.get(key).ok_or_else(|| SecretsError::NotFound {
                    path: format!("{}#{}", secret_ref.path, key),
                })?;
                let extracted_str = match extracted {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                results.insert(secret_ref.env_name.clone(), SecretValue::new(extracted_str));
            } else {
                results.insert(secret_ref.env_name.clone(), value);
            }
        }

        Ok(results)
    }
}

/// Builder for [`MultiProviderBroker`].
#[derive(Debug, Default)]
pub struct MultiProviderBrokerBuilder {
    providers: Vec<Arc<dyn SecretsProvider>>,
    default_provider: Option<ProviderType>,
    config: Option<BrokerConfig>,
}

impl MultiProviderBrokerBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_provider(mut self, provider: Arc<dyn SecretsProvider>) -> Self {
        self.providers.push(provider);
        self
    }

    pub fn with_default(mut self, provider_type: ProviderType) -> Self {
        self.default_provider = Some(provider_type);
        self
    }

    pub fn with_config(mut self, config: BrokerConfig) -> Self {
        self.config = Some(config);
        self
    }

    pub fn build(self) -> MultiProviderBroker {
        let mut broker = match self.config {
            Some(config) => MultiProviderBroker::with_config(config),
            None => MultiProviderBroker::new(),
        };
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

    #[tokio::test]
    async fn test_circuit_breaker_basics() {
        let cb = CircuitBreaker::new(3, Duration::from_millis(100));
        assert!(cb.is_available().await);

        cb.record_failure().await;
        cb.record_failure().await;
        assert!(cb.is_available().await);

        cb.record_failure().await;
        assert!(!cb.is_available().await);

        cb.record_success();
        assert!(cb.is_available().await);
    }

    #[tokio::test]
    async fn test_preflight_no_provider() {
        let broker = MultiProviderBroker::new();
        let refs =
            vec![SecretRef::new("MY_SECRET", "secret/path").with_provider(ProviderType::Vault)];
        let result = broker.preflight_check(&refs).await;
        assert!(result.is_err());
    }
}
