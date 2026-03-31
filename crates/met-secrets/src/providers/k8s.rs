//! Kubernetes secrets provider using the K8s API.

use async_trait::async_trait;
use tracing::{debug, info};

use crate::error::{Result, SecretsError};
use crate::traits::{ProviderConfig, SecretsProvider};
use crate::types::{ProviderType, SecretValue};

/// Configuration for the Kubernetes secrets provider.
#[derive(Debug, Clone)]
pub struct KubernetesConfig {
    pub kubeconfig: Option<String>,
    pub context: Option<String>,
    pub namespace: Option<String>,
    pub api_server_url: Option<String>,
    pub service_account_token_path: Option<String>,
    pub timeout_secs: u64,
}

impl Default for KubernetesConfig {
    fn default() -> Self {
        Self {
            kubeconfig: None, context: None, namespace: None,
            api_server_url: None,
            service_account_token_path: Some("/var/run/secrets/kubernetes.io/serviceaccount/token".into()),
            timeout_secs: 30,
        }
    }
}

impl KubernetesConfig {
    pub fn from_provider_config(config: &ProviderConfig) -> Result<Self> {
        Ok(Self {
            kubeconfig: config.get("kubeconfig").map(String::from),
            context: config.get("context").map(String::from),
            namespace: config.get("namespace").map(String::from),
            api_server_url: config.get("api_server_url").map(String::from),
            service_account_token_path: config.get("sa_token_path").map(String::from)
                .or(Some("/var/run/secrets/kubernetes.io/serviceaccount/token".into())),
            timeout_secs: config.get("timeout_secs").and_then(|v| v.parse().ok()).unwrap_or(30),
        })
    }
}

#[derive(Debug)]
pub struct KubernetesSecretsProvider {
    config: KubernetesConfig,
    default_namespace: String,
    client: reqwest::Client,
    api_server: String,
    bearer_token: Option<String>,
}

impl KubernetesSecretsProvider {
    pub async fn new(config: KubernetesConfig) -> Result<Self> {
        let default_namespace = config.namespace.clone().unwrap_or_else(|| {
            std::fs::read_to_string("/var/run/secrets/kubernetes.io/serviceaccount/namespace")
                .unwrap_or_else(|_| "default".to_string())
                .trim()
                .to_string()
        });

        let api_server = config.api_server_url.clone().unwrap_or_else(|| {
            let host = std::env::var("KUBERNETES_SERVICE_HOST").unwrap_or_else(|_| "kubernetes.default.svc".into());
            let port = std::env::var("KUBERNETES_SERVICE_PORT").unwrap_or_else(|_| "443".into());
            format!("https://{host}:{port}")
        });

        let bearer_token = config.service_account_token_path.as_ref()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .map(|t| t.trim().to_string());

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .danger_accept_invalid_certs(true) // In-cluster CA is handled by K8s
            .build()
            .map_err(|e| SecretsError::connection_failed("k8s", e.to_string()))?;

        info!(namespace = %default_namespace, api = %api_server, "K8s secrets provider initialized");
        Ok(Self { config, default_namespace, client, api_server, bearer_token })
    }

    pub async fn from_config(config: &ProviderConfig) -> Result<Self> {
        Self::new(KubernetesConfig::from_provider_config(config)?).await
    }

    fn parse_path(&self, path: &str) -> Result<(String, String, String)> {
        let parts: Vec<&str> = path.split('/').collect();
        match parts.len() {
            2 => Ok((self.default_namespace.clone(), parts[0].to_string(), parts[1].to_string())),
            3 => Ok((parts[0].to_string(), parts[1].to_string(), parts[2].to_string())),
            _ => Err(SecretsError::InvalidFormat {
                message: format!("invalid k8s path: {path}. Expected [namespace/]secret/key"),
            }),
        }
    }
}

#[async_trait]
impl SecretsProvider for KubernetesSecretsProvider {
    async fn get_secret(&self, path: &str) -> Result<SecretValue> {
        let (namespace, secret_name, key) = self.parse_path(path)?;
        debug!(namespace = %namespace, secret = %secret_name, key = %key, "Fetching K8s secret");

        let url = format!("{}/api/v1/namespaces/{}/secrets/{}", self.api_server, namespace, secret_name);
        let mut req = self.client.get(&url);
        if let Some(token) = &self.bearer_token {
            req = req.bearer_auth(token);
        }

        let resp = req.send().await
            .map_err(|e| SecretsError::connection_failed("k8s", e.to_string()))?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(SecretsError::not_found(path));
        }
        if resp.status() == reqwest::StatusCode::FORBIDDEN {
            return Err(SecretsError::access_denied(path, None::<String>));
        }
        if !resp.status().is_success() {
            return Err(SecretsError::ProviderError {
                provider: "k8s".into(),
                message: format!("HTTP {}", resp.status()),
            });
        }

        let body: serde_json::Value = resp.json().await
            .map_err(|e| SecretsError::ProviderError { provider: "k8s".into(), message: e.to_string() })?;

        let encoded = body["data"][&key].as_str().ok_or_else(|| {
            SecretsError::not_found(format!("{path} (key '{key}' not in secret)"))
        })?;

        let decoded = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded)
            .map_err(|e| SecretsError::InvalidFormat { message: format!("base64 decode: {e}") })?;
        let value = String::from_utf8(decoded)
            .map_err(|e| SecretsError::InvalidFormat { message: format!("UTF-8 decode: {e}") })?;

        Ok(SecretValue::new(value))
    }

    async fn list_secrets(&self, prefix: &str) -> Result<Vec<String>> {
        let (namespace, secret_prefix) = if prefix.contains('/') {
            let parts: Vec<&str> = prefix.splitn(2, '/').collect();
            (parts[0].to_string(), parts.get(1).copied().unwrap_or(""))
        } else {
            (self.default_namespace.clone(), prefix)
        };

        let url = format!("{}/api/v1/namespaces/{}/secrets", self.api_server, namespace);
        let mut req = self.client.get(&url);
        if let Some(token) = &self.bearer_token {
            req = req.bearer_auth(token);
        }
        let resp = req.send().await
            .map_err(|e| SecretsError::connection_failed("k8s", e.to_string()))?;
        let body: serde_json::Value = resp.json().await
            .map_err(|e| SecretsError::ProviderError { provider: "k8s".into(), message: e.to_string() })?;

        let items = body["items"].as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| item["metadata"]["name"].as_str())
                    .filter(|name| name.starts_with(secret_prefix))
                    .map(|name| format!("{namespace}/{name}"))
                    .collect()
            })
            .unwrap_or_default();
        Ok(items)
    }

    fn provider_type(&self) -> ProviderType { ProviderType::Kubernetes }

    async fn health_check(&self) -> Result<()> {
        let url = format!("{}/api/v1/namespaces/{}/secrets?limit=1", self.api_server, self.default_namespace);
        let mut req = self.client.get(&url);
        if let Some(token) = &self.bearer_token {
            req = req.bearer_auth(token);
        }
        req.send().await
            .map_err(|e| SecretsError::connection_failed("k8s", e.to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_parse_path() {
        let config = KubernetesConfig { namespace: Some("default".into()), ..Default::default() };
        let provider = KubernetesSecretsProvider::new(config).await.unwrap();
        let (ns, name, key) = provider.parse_path("prod/db-creds/password").unwrap();
        assert_eq!(ns, "prod");
        assert_eq!(name, "db-creds");
        assert_eq!(key, "password");

        let (ns, name, key) = provider.parse_path("my-secret/api-key").unwrap();
        assert_eq!(ns, "default");
        assert_eq!(name, "my-secret");
        assert_eq!(key, "api-key");

        assert!(provider.parse_path("invalid").is_err());
    }

    #[tokio::test]
    async fn test_provider_type() {
        let provider = KubernetesSecretsProvider::new(KubernetesConfig::default()).await.unwrap();
        assert_eq!(provider.provider_type(), ProviderType::Kubernetes);
    }
}
