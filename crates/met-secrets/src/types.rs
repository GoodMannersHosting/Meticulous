//! Core types for secret handling.
//!
//! This module provides secure types for working with secrets, including
//! automatic memory zeroization on drop and protection against accidental
//! logging or serialization of sensitive values.

use chrono::{DateTime, Utc};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use std::fmt;
use zeroize::{Zeroize, ZeroizeOnDrop};

/// A secret value with secure memory handling.
///
/// This type wraps sensitive data and provides:
/// - Automatic zeroization of memory on drop
/// - Protection against accidental Debug/Display output
/// - Explicit exposure through `expose_secret()`
#[derive(Clone, ZeroizeOnDrop)]
pub struct SecretValue {
    /// The actual secret data, stored securely.
    #[zeroize(skip)]
    inner: SecretString,
    /// Optional metadata about the secret.
    #[zeroize(skip)]
    metadata: SecretMetadata,
}

impl SecretValue {
    /// Create a new secret value from a string.
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            inner: SecretString::from(value.into()),
            metadata: SecretMetadata::default(),
        }
    }

    /// Create a secret value with metadata.
    pub fn with_metadata(value: impl Into<String>, metadata: SecretMetadata) -> Self {
        Self {
            inner: SecretString::from(value.into()),
            metadata,
        }
    }

    /// Expose the secret value.
    ///
    /// Use this method sparingly and only when you actually need the value.
    /// The returned reference should not be stored or logged.
    pub fn expose_secret(&self) -> &str {
        self.inner.expose_secret()
    }

    /// Get the metadata associated with this secret.
    pub fn metadata(&self) -> &SecretMetadata {
        &self.metadata
    }

    /// Check if this secret has expired based on its metadata.
    pub fn is_expired(&self) -> bool {
        self.metadata
            .expires_at
            .is_some_and(|exp| exp < Utc::now())
    }

    /// Get the length of the secret value.
    pub fn len(&self) -> usize {
        self.inner.expose_secret().len()
    }

    /// Check if the secret is empty.
    pub fn is_empty(&self) -> bool {
        self.inner.expose_secret().is_empty()
    }
}

impl fmt::Debug for SecretValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SecretValue")
            .field("inner", &"[REDACTED]")
            .field("metadata", &self.metadata)
            .finish()
    }
}

impl PartialEq for SecretValue {
    fn eq(&self, other: &Self) -> bool {
        self.inner.expose_secret() == other.inner.expose_secret()
    }
}

impl Eq for SecretValue {}

/// Metadata about a secret.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SecretMetadata {
    /// The version of the secret (provider-specific).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// When the secret was created.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
    /// When the secret was last rotated.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotated_at: Option<DateTime<Utc>>,
    /// When the secret expires.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
    /// Content type hint (e.g., "application/json", "text/plain").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
}

/// A reference to a secret without loading the actual value.
///
/// Use this type when you need to pass around secret identifiers
/// without actually retrieving the secret data.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct SecretPath {
    /// The provider type.
    pub provider: ProviderType,
    /// The path within the provider.
    pub path: String,
    /// Optional key within the secret (for JSON/map secrets).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// Optional version to fetch.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

impl SecretPath {
    /// Create a new secret path.
    pub fn new(provider: ProviderType, path: impl Into<String>) -> Self {
        Self {
            provider,
            path: path.into(),
            key: None,
            version: None,
        }
    }

    /// Set the key within the secret.
    pub fn with_key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Set the version to fetch.
    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }
}

impl fmt::Display for SecretPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.provider, self.path)?;
        if let Some(key) = &self.key {
            write!(f, "#{key}")?;
        }
        if let Some(version) = &self.version {
            write!(f, "@{version}")?;
        }
        Ok(())
    }
}

/// The type of secrets provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProviderType {
    /// HashiCorp Vault or OpenBao.
    Vault,
    /// AWS Secrets Manager.
    AwsSecretsManager,
    /// Kubernetes secrets.
    Kubernetes,
    /// Azure Key Vault.
    AzureKeyVault,
    /// GCP Secret Manager.
    GcpSecretManager,
    /// Built-in encrypted storage (stored in Postgres).
    #[default]
    Builtin,
}

impl ProviderType {
    /// Get the string identifier for this provider type.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Vault => "vault",
            Self::AwsSecretsManager => "aws_sm",
            Self::Kubernetes => "k8s",
            Self::AzureKeyVault => "azure_kv",
            Self::GcpSecretManager => "gcp_sm",
            Self::Builtin => "builtin",
        }
    }
}

impl fmt::Display for ProviderType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for ProviderType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "vault" | "hashicorp_vault" | "openbao" => Ok(Self::Vault),
            "aws_sm" | "aws_secrets_manager" | "aws" => Ok(Self::AwsSecretsManager),
            "k8s" | "kubernetes" => Ok(Self::Kubernetes),
            "azure_kv" | "azure_key_vault" | "azure" => Ok(Self::AzureKeyVault),
            "gcp_sm" | "gcp_secret_manager" | "gcp" => Ok(Self::GcpSecretManager),
            "builtin" | "internal" => Ok(Self::Builtin),
            _ => Err(format!("unknown provider type: {s}")),
        }
    }
}

/// Binary secret data with secure memory handling.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct SecretBytes {
    inner: Vec<u8>,
}

impl SecretBytes {
    /// Create from raw bytes.
    pub fn new(data: Vec<u8>) -> Self {
        Self { inner: data }
    }

    /// Expose the raw bytes.
    pub fn expose_secret(&self) -> &[u8] {
        &self.inner
    }

    /// Get the length.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

impl fmt::Debug for SecretBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SecretBytes")
            .field("len", &self.inner.len())
            .finish()
    }
}

impl From<Vec<u8>> for SecretBytes {
    fn from(data: Vec<u8>) -> Self {
        Self::new(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secret_value_redacted_debug() {
        let secret = SecretValue::new("super-secret-password");
        let debug_output = format!("{secret:?}");
        assert!(debug_output.contains("[REDACTED]"));
        assert!(!debug_output.contains("super-secret-password"));
    }

    #[test]
    fn test_secret_value_expose() {
        let secret = SecretValue::new("my-api-key");
        assert_eq!(secret.expose_secret(), "my-api-key");
    }

    #[test]
    fn test_secret_value_equality() {
        let s1 = SecretValue::new("same");
        let s2 = SecretValue::new("same");
        let s3 = SecretValue::new("different");
        assert_eq!(s1, s2);
        assert_ne!(s1, s3);
    }

    #[test]
    fn test_provider_type_from_str() {
        assert_eq!("vault".parse::<ProviderType>().unwrap(), ProviderType::Vault);
        assert_eq!("aws_sm".parse::<ProviderType>().unwrap(), ProviderType::AwsSecretsManager);
        assert_eq!("k8s".parse::<ProviderType>().unwrap(), ProviderType::Kubernetes);
        assert!("unknown".parse::<ProviderType>().is_err());
    }

    #[test]
    fn test_secret_path_display() {
        let path = SecretPath::new(ProviderType::Vault, "secret/myapp/api-key")
            .with_key("password")
            .with_version("3");
        assert_eq!(path.to_string(), "vault:secret/myapp/api-key#password@3");
    }

    #[test]
    fn test_secret_expiration() {
        let expired = SecretValue::with_metadata(
            "value",
            SecretMetadata {
                expires_at: Some(Utc::now() - chrono::Duration::hours(1)),
                ..Default::default()
            },
        );
        assert!(expired.is_expired());

        let valid = SecretValue::with_metadata(
            "value",
            SecretMetadata {
                expires_at: Some(Utc::now() + chrono::Duration::hours(1)),
                ..Default::default()
            },
        );
        assert!(!valid.is_expired());
    }
}
