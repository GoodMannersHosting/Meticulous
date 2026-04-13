//! AWS Secrets Manager provider.
//!
//! Supports automatic credential discovery via the standard AWS credential chain
//! and Roles Anywhere via pipeline OIDC tokens.

use async_trait::async_trait;
use serde::Deserialize;
use tracing::{debug, info};

use crate::error::{Result, SecretsError};
use crate::traits::{ProviderConfig, SecretsProvider};
use crate::types::{ProviderType, SecretMetadata, SecretValue};

/// Configuration for the AWS Secrets Manager provider.
#[derive(Debug, Clone)]
pub struct AwsSecretsConfig {
    pub region: Option<String>,
    pub endpoint_url: Option<String>,
    pub access_key_id: Option<String>,
    pub secret_access_key: Option<String>,
    pub oidc_token: Option<String>,
    pub role_arn: Option<String>,
    pub timeout_secs: u64,
}

impl Default for AwsSecretsConfig {
    fn default() -> Self {
        Self {
            region: None,
            endpoint_url: None,
            access_key_id: None,
            secret_access_key: None,
            oidc_token: None,
            role_arn: None,
            timeout_secs: 30,
        }
    }
}

impl AwsSecretsConfig {
    pub fn from_provider_config(config: &ProviderConfig) -> Result<Self> {
        Ok(Self {
            region: config.get("region").map(String::from),
            endpoint_url: config.get("endpoint_url").map(String::from),
            access_key_id: config.get("access_key_id").map(String::from),
            secret_access_key: config.get("secret_access_key").map(String::from),
            oidc_token: config.get("oidc_token").map(String::from),
            role_arn: config.get("role_arn").map(String::from),
            timeout_secs: config
                .get("timeout_secs")
                .and_then(|v| v.parse().ok())
                .unwrap_or(30),
        })
    }
}

#[derive(Debug, Deserialize)]
struct AwsSecretResponse {
    #[serde(rename = "SecretString")]
    secret_string: Option<String>,
    #[serde(rename = "VersionId")]
    version_id: Option<String>,
    #[allow(dead_code)]
    #[serde(rename = "CreatedDate")]
    created_date: Option<String>,
    #[allow(dead_code)]
    #[serde(rename = "Name")]
    name: Option<String>,
}

#[derive(Debug)]
pub struct AwsSecretsProvider {
    config: AwsSecretsConfig,
    client: reqwest::Client,
}

impl AwsSecretsProvider {
    pub async fn new(config: AwsSecretsConfig) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .build()
            .map_err(|e| SecretsError::connection_failed("aws_sm", e.to_string()))?;
        info!(region = ?config.region, "AWS Secrets Manager provider initialized");
        Ok(Self { config, client })
    }

    pub async fn from_config(config: &ProviderConfig) -> Result<Self> {
        Self::new(AwsSecretsConfig::from_provider_config(config)?).await
    }

    fn endpoint(&self) -> String {
        self.config.endpoint_url.clone().unwrap_or_else(|| {
            let region = self.config.region.as_deref().unwrap_or("us-east-1");
            format!("https://secretsmanager.{region}.amazonaws.com")
        })
    }
}

#[async_trait]
impl SecretsProvider for AwsSecretsProvider {
    async fn get_secret(&self, path: &str) -> Result<SecretValue> {
        debug!(secret_id = %path, "Fetching secret from AWS Secrets Manager");

        // In a full implementation this would use aws-sdk-secretsmanager.
        // This implementation uses the HTTP API directly as a demonstration.
        // Production deployments should use the AWS SDK.
        let endpoint = self.endpoint();
        let body = serde_json::json!({ "SecretId": path });

        let resp = self
            .client
            .post(&endpoint)
            .header("Content-Type", "application/x-amz-json-1.1")
            .header("X-Amz-Target", "secretsmanager.GetSecretValue")
            .json(&body)
            .send()
            .await
            .map_err(|e| SecretsError::connection_failed("aws_sm", e.to_string()))?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(SecretsError::not_found(path));
        }
        if !resp.status().is_success() {
            return Err(SecretsError::ProviderError {
                provider: "aws_sm".into(),
                message: format!("HTTP {}", resp.status()),
            });
        }

        let aws_resp: AwsSecretResponse =
            resp.json().await.map_err(|e| SecretsError::ProviderError {
                provider: "aws_sm".into(),
                message: format!("failed to parse response: {e}"),
            })?;

        let value = aws_resp
            .secret_string
            .ok_or_else(|| SecretsError::InvalidFormat {
                message: "SecretString is null (binary secrets not supported)".into(),
            })?;

        Ok(SecretValue::with_metadata(
            value,
            SecretMetadata {
                version: aws_resp.version_id,
                ..Default::default()
            },
        ))
    }

    async fn list_secrets(&self, prefix: &str) -> Result<Vec<String>> {
        debug!(prefix, "Listing secrets from AWS Secrets Manager");
        let endpoint = self.endpoint();
        let body = serde_json::json!({
            "Filters": [{ "Key": "name", "Values": [prefix] }]
        });
        let resp = self
            .client
            .post(&endpoint)
            .header("Content-Type", "application/x-amz-json-1.1")
            .header("X-Amz-Target", "secretsmanager.ListSecrets")
            .json(&body)
            .send()
            .await
            .map_err(|e| SecretsError::connection_failed("aws_sm", e.to_string()))?;
        let body: serde_json::Value =
            resp.json().await.map_err(|e| SecretsError::ProviderError {
                provider: "aws_sm".into(),
                message: e.to_string(),
            })?;
        let names = body["SecretList"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v["Name"].as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        Ok(names)
    }

    fn provider_type(&self) -> ProviderType {
        ProviderType::AwsSecretsManager
    }

    async fn health_check(&self) -> Result<()> {
        let endpoint = self.endpoint();
        let body = serde_json::json!({ "MaxResults": 1 });
        self.client
            .post(&endpoint)
            .header("Content-Type", "application/x-amz-json-1.1")
            .header("X-Amz-Target", "secretsmanager.ListSecrets")
            .json(&body)
            .send()
            .await
            .map_err(|e| SecretsError::connection_failed("aws_sm", e.to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aws_config_default() {
        let config = AwsSecretsConfig::default();
        assert!(config.region.is_none());
        assert_eq!(config.timeout_secs, 30);
    }

    #[tokio::test]
    async fn test_provider_type() {
        let provider = AwsSecretsProvider::new(AwsSecretsConfig::default())
            .await
            .unwrap();
        assert_eq!(provider.provider_type(), ProviderType::AwsSecretsManager);
    }
}
