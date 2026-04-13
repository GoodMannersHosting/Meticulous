//! HashiCorp Vault / OpenBao secrets provider.
//!
//! Supports KV v1 and KV v2 secret engines with AppRole and JWT authentication.

use async_trait::async_trait;
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

use crate::error::{Result, SecretsError};
use crate::traits::{ProviderConfig, SecretsProvider};
use crate::types::{ProviderType, SecretMetadata, SecretValue};

/// Authentication method for Vault.
#[derive(Debug, Clone)]
pub enum VaultAuth {
    /// Direct token authentication.
    Token(String),
    /// AppRole authentication.
    AppRole { role_id: String, secret_id: String },
    /// JWT authentication (preferred for zero-credential setups).
    Jwt { role: String, jwt: String },
}

/// Configuration for the Vault provider.
#[derive(Debug, Clone)]
pub struct VaultConfig {
    pub address: String,
    pub auth: Option<VaultAuth>,
    pub namespace: Option<String>,
    pub mount_path: String,
    pub kv_version: u8,
    pub timeout_secs: u64,
}

impl Default for VaultConfig {
    fn default() -> Self {
        Self {
            address: "http://127.0.0.1:8200".into(),
            auth: None,
            namespace: None,
            mount_path: "secret".into(),
            kv_version: 2,
            timeout_secs: 30,
        }
    }
}

impl VaultConfig {
    pub fn from_provider_config(config: &ProviderConfig) -> Result<Self> {
        let address = config.require("address")?.to_string();
        let namespace = config.get("namespace").map(String::from);
        let mount_path = config.get("mount_path").unwrap_or("secret").to_string();
        let kv_version = config
            .get("kv_version")
            .and_then(|v| v.parse().ok())
            .unwrap_or(2);

        let auth = if let Some(token) = config.get("token") {
            Some(VaultAuth::Token(token.to_string()))
        } else if let (Some(role_id), Some(secret_id)) =
            (config.get("role_id"), config.get("secret_id"))
        {
            Some(VaultAuth::AppRole {
                role_id: role_id.to_string(),
                secret_id: secret_id.to_string(),
            })
        } else if let (Some(role), Some(jwt)) = (config.get("jwt_role"), config.get("jwt")) {
            Some(VaultAuth::Jwt {
                role: role.to_string(),
                jwt: jwt.to_string(),
            })
        } else {
            None
        };

        Ok(Self {
            address,
            auth,
            namespace,
            mount_path,
            kv_version,
            timeout_secs: 30,
        })
    }
}

// Wire format types for Vault KV/auth JSON (deserialized when KV client is extended).
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct VaultResponse<T> {
    data: T,
    lease_duration: Option<u64>,
    renewable: Option<bool>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct KvV2Data {
    data: std::collections::HashMap<String, serde_json::Value>,
    metadata: Option<KvV2Metadata>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct KvV2Metadata {
    version: Option<u64>,
    created_time: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct VaultAuthResponse {
    client_token: String,
    lease_duration: u64,
    renewable: bool,
}

#[derive(Debug)]
pub struct VaultProvider {
    config: VaultConfig,
    client: reqwest::Client,
    token: Arc<RwLock<Option<String>>>,
}

impl VaultProvider {
    pub async fn new(config: VaultConfig) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .build()
            .map_err(|e| SecretsError::connection_failed("vault", e.to_string()))?;

        let token = match &config.auth {
            Some(VaultAuth::Token(t)) => Some(t.clone()),
            _ => None,
        };

        let provider = Self {
            config,
            client,
            token: Arc::new(RwLock::new(token)),
        };

        if matches!(
            &provider.config.auth,
            Some(VaultAuth::AppRole { .. }) | Some(VaultAuth::Jwt { .. })
        ) {
            provider.authenticate().await?;
        }

        info!(address = %provider.config.address, "Vault provider initialized");
        Ok(provider)
    }

    pub async fn from_config(config: &ProviderConfig) -> Result<Self> {
        Self::new(VaultConfig::from_provider_config(config)?).await
    }

    async fn authenticate(&self) -> Result<()> {
        let token = match &self.config.auth {
            Some(VaultAuth::AppRole { role_id, secret_id }) => {
                let url = format!("{}/v1/auth/approle/login", self.config.address);
                let body = serde_json::json!({ "role_id": role_id, "secret_id": secret_id });
                let resp = self
                    .client
                    .post(&url)
                    .json(&body)
                    .send()
                    .await
                    .map_err(|e| SecretsError::auth_failed("vault", e.to_string()))?;
                let auth_resp: serde_json::Value = resp
                    .json()
                    .await
                    .map_err(|e| SecretsError::auth_failed("vault", e.to_string()))?;
                auth_resp["auth"]["client_token"]
                    .as_str()
                    .ok_or_else(|| {
                        SecretsError::auth_failed("vault", "no client_token in response")
                    })?
                    .to_string()
            }
            Some(VaultAuth::Jwt { role, jwt }) => {
                let url = format!("{}/v1/auth/jwt/login", self.config.address);
                let body = serde_json::json!({ "role": role, "jwt": jwt });
                let resp = self
                    .client
                    .post(&url)
                    .json(&body)
                    .send()
                    .await
                    .map_err(|e| SecretsError::auth_failed("vault", e.to_string()))?;
                let auth_resp: serde_json::Value = resp
                    .json()
                    .await
                    .map_err(|e| SecretsError::auth_failed("vault", e.to_string()))?;
                auth_resp["auth"]["client_token"]
                    .as_str()
                    .ok_or_else(|| {
                        SecretsError::auth_failed("vault", "no client_token in response")
                    })?
                    .to_string()
            }
            _ => return Ok(()),
        };
        *self.token.write().await = Some(token);
        debug!("Vault authentication successful");
        Ok(())
    }

    async fn get_token(&self) -> Result<String> {
        self.token
            .read()
            .await
            .clone()
            .ok_or_else(|| SecretsError::auth_failed("vault", "no token available"))
    }

    fn build_url(&self, path: &str) -> String {
        if self.config.kv_version == 2 {
            format!(
                "{}/v1/{}/data/{}",
                self.config.address, self.config.mount_path, path
            )
        } else {
            format!(
                "{}/v1/{}/{}",
                self.config.address, self.config.mount_path, path
            )
        }
    }

    async fn request(&self, url: &str) -> Result<reqwest::Response> {
        let token = self.get_token().await?;
        let mut req = self.client.get(url).header("X-Vault-Token", &token);
        if let Some(ns) = &self.config.namespace {
            req = req.header("X-Vault-Namespace", ns);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| SecretsError::connection_failed("vault", e.to_string()))?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(SecretsError::not_found(url));
        }
        if resp.status() == reqwest::StatusCode::FORBIDDEN {
            return Err(SecretsError::access_denied(url, None::<String>));
        }
        if !resp.status().is_success() {
            return Err(SecretsError::ProviderError {
                provider: "vault".into(),
                message: format!("HTTP {}", resp.status()),
            });
        }
        Ok(resp)
    }

    /// Generate a Vault policy HCL document for a given set of secret paths.
    pub fn generate_policy(paths: &[&str], mount_path: &str) -> String {
        let mut policy = String::new();
        for path in paths {
            policy.push_str(&format!(
                "path \"{mount_path}/data/{path}\" {{\n  capabilities = [\"read\"]\n}}\n\n"
            ));
            policy.push_str(&format!(
                "path \"{mount_path}/metadata/{path}\" {{\n  capabilities = [\"read\", \"list\"]\n}}\n\n"
            ));
        }
        policy
    }
}

#[async_trait]
impl SecretsProvider for VaultProvider {
    async fn get_secret(&self, path: &str) -> Result<SecretValue> {
        let url = self.build_url(path);
        debug!(path, url = %url, "Fetching secret from Vault");

        let resp = self.request(&url).await?;
        let body: serde_json::Value =
            resp.json().await.map_err(|e| SecretsError::ProviderError {
                provider: "vault".into(),
                message: format!("failed to parse response: {e}"),
            })?;

        if self.config.kv_version == 2 {
            let data =
                body["data"]["data"]
                    .as_object()
                    .ok_or_else(|| SecretsError::InvalidFormat {
                        message: "unexpected KV v2 response format".into(),
                    })?;
            let value_str = serde_json::to_string(data).unwrap_or_default();
            let version = body["data"]["metadata"]["version"]
                .as_u64()
                .map(|v| v.to_string());
            Ok(SecretValue::with_metadata(
                value_str,
                SecretMetadata {
                    version,
                    ..Default::default()
                },
            ))
        } else {
            let data = body["data"]
                .as_object()
                .ok_or_else(|| SecretsError::InvalidFormat {
                    message: "unexpected KV v1 response format".into(),
                })?;
            let value_str = serde_json::to_string(data).unwrap_or_default();
            Ok(SecretValue::new(value_str))
        }
    }

    async fn get_secret_version(&self, path: &str, version: &str) -> Result<SecretValue> {
        if self.config.kv_version != 2 {
            return self.get_secret(path).await;
        }
        let url = format!("{}?version={}", self.build_url(path), version);
        let resp = self.request(&url).await?;
        let body: serde_json::Value =
            resp.json().await.map_err(|e| SecretsError::ProviderError {
                provider: "vault".into(),
                message: e.to_string(),
            })?;
        let data = body["data"]["data"]
            .as_object()
            .ok_or_else(|| SecretsError::InvalidFormat {
                message: "unexpected response".into(),
            })?;
        Ok(SecretValue::with_metadata(
            serde_json::to_string(data).unwrap_or_default(),
            SecretMetadata {
                version: Some(version.to_string()),
                ..Default::default()
            },
        ))
    }

    async fn list_secrets(&self, prefix: &str) -> Result<Vec<String>> {
        let url = if self.config.kv_version == 2 {
            format!(
                "{}/v1/{}/metadata/{}?list=true",
                self.config.address, self.config.mount_path, prefix
            )
        } else {
            format!(
                "{}/v1/{}/{}?list=true",
                self.config.address, self.config.mount_path, prefix
            )
        };
        let token = self.get_token().await?;
        let resp = self
            .client
            .get(&url)
            .header("X-Vault-Token", &token)
            .send()
            .await
            .map_err(|e| SecretsError::connection_failed("vault", e.to_string()))?;
        let body: serde_json::Value =
            resp.json().await.map_err(|e| SecretsError::ProviderError {
                provider: "vault".into(),
                message: e.to_string(),
            })?;
        let keys = body["data"]["keys"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        Ok(keys)
    }

    fn provider_type(&self) -> ProviderType {
        ProviderType::Vault
    }

    async fn health_check(&self) -> Result<()> {
        let url = format!("{}/v1/sys/health", self.config.address);
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| SecretsError::connection_failed("vault", e.to_string()))?;
        if resp.status().is_success() || resp.status().as_u16() == 429 {
            Ok(())
        } else {
            Err(SecretsError::provider_unavailable(
                "vault",
                format!("health check returned {}", resp.status()),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vault_config_default() {
        let config = VaultConfig::default();
        assert_eq!(config.address, "http://127.0.0.1:8200");
        assert_eq!(config.mount_path, "secret");
        assert_eq!(config.kv_version, 2);
    }

    #[test]
    fn test_generate_policy() {
        let policy = VaultProvider::generate_policy(&["myapp/config", "myapp/db"], "secret");
        assert!(policy.contains("secret/data/myapp/config"));
        assert!(policy.contains("secret/metadata/myapp/db"));
        assert!(policy.contains("capabilities"));
    }
}
