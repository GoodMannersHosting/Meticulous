//! Built-in encrypted secrets provider using AES-256-GCM envelope encryption.

use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit, OsRng},
};
use async_trait::async_trait;
use hkdf::Hkdf;
use rand::RngCore;
use sha2::Sha256;
use tracing::{debug, info, warn};
use zeroize::Zeroizing;

use crate::error::{Result, SecretsError};
use crate::traits::{ProviderConfig, SecretsProvider, SecretsWriter};
use crate::types::{ProviderType, SecretMetadata, SecretValue};

use crate::stored_crypto::BUILTIN_SECRETS_HKDF_INFO;

const NONCE_SIZE: usize = 12;

/// Configuration for the built-in secrets provider.
#[derive(Debug, Clone)]
pub struct BuiltinConfig {
    /// Base64-encoded master encryption key (256-bit).
    pub master_key: Option<String>,
    /// Current key ID for rotation support.
    pub key_id: Option<String>,
}

impl Default for BuiltinConfig {
    fn default() -> Self {
        Self {
            master_key: None,
            key_id: None,
        }
    }
}

impl BuiltinConfig {
    pub fn from_provider_config(config: &ProviderConfig) -> Result<Self> {
        Ok(Self {
            master_key: config.get("master_key").map(String::from),
            key_id: config.get("key_id").map(String::from),
        })
    }
}

/// Derived encryption key from the master key.
struct DerivedKey {
    key: Zeroizing<[u8; 32]>,
    key_id: String,
}

/// Built-in encrypted secrets provider.
///
/// Uses AES-256-GCM with HKDF-derived keys from a master key.
/// Supports key rotation via key_id versioning.
#[derive(Debug)]
pub struct BuiltinSecretsProvider {
    config: BuiltinConfig,
    derived_key: Option<DerivedKey>,
    /// In-memory store for testing when no database is available.
    /// In production, this should be replaced with sqlx queries.
    store: tokio::sync::RwLock<std::collections::HashMap<String, StoredSecret>>,
}

impl std::fmt::Debug for DerivedKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DerivedKey")
            .field("key_id", &self.key_id)
            .finish()
    }
}

#[derive(Clone, Debug)]
struct StoredSecret {
    encrypted_value: Vec<u8>,
    nonce: [u8; NONCE_SIZE],
    key_id: String,
    version: u32,
    metadata: SecretMetadata,
}

impl BuiltinSecretsProvider {
    pub async fn new(config: BuiltinConfig) -> Result<Self> {
        let derived_key = if let Some(master_key_b64) = &config.master_key {
            let master_key_bytes =
                base64::Engine::decode(&base64::engine::general_purpose::STANDARD, master_key_b64)
                    .map_err(|e| SecretsError::Crypto(format!("invalid master key base64: {e}")))?;

            if master_key_bytes.len() < 16 {
                return Err(SecretsError::Crypto(
                    "master key too short (min 16 bytes)".into(),
                ));
            }

            let hk = Hkdf::<Sha256>::new(None, &master_key_bytes);
            let mut key = Zeroizing::new([0u8; 32]);
            hk.expand(BUILTIN_SECRETS_HKDF_INFO, key.as_mut())
                .map_err(|e| SecretsError::Crypto(format!("HKDF expand failed: {e}")))?;

            let key_id = config.key_id.clone().unwrap_or_else(|| "v1".to_string());
            info!(key_id = %key_id, "Built-in secrets provider initialized with encryption");
            Some(DerivedKey { key, key_id })
        } else {
            warn!("Built-in secrets provider initialized WITHOUT encryption key");
            None
        };

        Ok(Self {
            config,
            derived_key,
            store: tokio::sync::RwLock::new(std::collections::HashMap::new()),
        })
    }

    pub async fn from_config(config: &ProviderConfig) -> Result<Self> {
        Self::new(BuiltinConfig::from_provider_config(config)?).await
    }

    fn encrypt(&self, plaintext: &[u8]) -> Result<(Vec<u8>, [u8; NONCE_SIZE], String)> {
        let dk = self
            .derived_key
            .as_ref()
            .ok_or_else(|| SecretsError::Crypto("no encryption key configured".into()))?;

        let mut nonce_bytes = [0u8; NONCE_SIZE];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let cipher = Aes256Gcm::new_from_slice(dk.key.as_ref())
            .map_err(|e| SecretsError::Crypto(format!("AES init: {e}")))?;
        let ciphertext = cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| SecretsError::Crypto(format!("encryption failed: {e}")))?;

        Ok((ciphertext, nonce_bytes, dk.key_id.clone()))
    }

    fn decrypt(&self, ciphertext: &[u8], nonce: &[u8; NONCE_SIZE]) -> Result<Zeroizing<Vec<u8>>> {
        let dk = self
            .derived_key
            .as_ref()
            .ok_or_else(|| SecretsError::Crypto("no encryption key configured".into()))?;

        let n = Nonce::from_slice(nonce);
        let cipher = Aes256Gcm::new_from_slice(dk.key.as_ref())
            .map_err(|e| SecretsError::Crypto(format!("AES init: {e}")))?;
        let plaintext = cipher
            .decrypt(n, ciphertext)
            .map_err(|e| SecretsError::Crypto(format!("decryption failed: {e}")))?;

        Ok(Zeroizing::new(plaintext))
    }

    /// Rotate to a new master key. Re-encrypts all stored secrets.
    pub async fn rotate_master_key(
        &mut self,
        new_master_key_b64: &str,
        new_key_id: &str,
    ) -> Result<u64> {
        let new_bytes = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            new_master_key_b64,
        )
        .map_err(|e| SecretsError::Crypto(format!("invalid new key: {e}")))?;

        let hk = Hkdf::<Sha256>::new(None, &new_bytes);
        let mut new_key = Zeroizing::new([0u8; 32]);
        hk.expand(BUILTIN_SECRETS_HKDF_INFO, new_key.as_mut())
            .map_err(|e| SecretsError::Crypto(format!("HKDF: {e}")))?;

        let mut store = self.store.write().await;
        let mut rotated = 0u64;

        for (path, stored) in store.iter_mut() {
            let plaintext = self.decrypt(&stored.encrypted_value, &stored.nonce)?;
            let mut nonce_bytes = [0u8; NONCE_SIZE];
            OsRng.fill_bytes(&mut nonce_bytes);
            let nonce = Nonce::from_slice(&nonce_bytes);
            let cipher = Aes256Gcm::new_from_slice(new_key.as_ref())
                .map_err(|e| SecretsError::Crypto(format!("AES init: {e}")))?;
            let new_ct = cipher
                .encrypt(nonce, plaintext.as_ref())
                .map_err(|e| SecretsError::Crypto(format!("re-encryption: {e}")))?;
            stored.encrypted_value = new_ct;
            stored.nonce = nonce_bytes;
            stored.key_id = new_key_id.to_string();
            stored.version += 1;
            rotated += 1;
            debug!(path, "re-encrypted secret with new key");
        }

        self.derived_key = Some(DerivedKey {
            key: new_key,
            key_id: new_key_id.to_string(),
        });

        info!(rotated, new_key_id, "master key rotation complete");
        Ok(rotated)
    }
}

#[async_trait]
impl SecretsProvider for BuiltinSecretsProvider {
    async fn get_secret(&self, path: &str) -> Result<SecretValue> {
        let store = self.store.read().await;
        let stored = store
            .get(path)
            .ok_or_else(|| SecretsError::not_found(path))?;
        let plaintext = self.decrypt(&stored.encrypted_value, &stored.nonce)?;
        let value =
            String::from_utf8(plaintext.to_vec()).map_err(|e| SecretsError::InvalidFormat {
                message: format!("UTF-8: {e}"),
            })?;
        Ok(SecretValue::with_metadata(value, stored.metadata.clone()))
    }

    async fn list_secrets(&self, prefix: &str) -> Result<Vec<String>> {
        let store = self.store.read().await;
        Ok(store
            .keys()
            .filter(|k| k.starts_with(prefix))
            .cloned()
            .collect())
    }

    fn provider_type(&self) -> ProviderType {
        ProviderType::Builtin
    }

    async fn health_check(&self) -> Result<()> {
        if self.derived_key.is_some() {
            Ok(())
        } else {
            Err(SecretsError::provider_unavailable(
                "builtin",
                "no encryption key",
            ))
        }
    }
}

#[async_trait]
impl SecretsWriter for BuiltinSecretsProvider {
    async fn put_secret(&self, path: &str, value: &SecretValue) -> Result<SecretMetadata> {
        let (encrypted, nonce, key_id) = self.encrypt(value.expose_secret().as_bytes())?;
        let mut store = self.store.write().await;
        let version = store.get(path).map(|s| s.version + 1).unwrap_or(1);
        let metadata = SecretMetadata {
            version: Some(version.to_string()),
            created_at: Some(chrono::Utc::now()),
            ..Default::default()
        };
        store.insert(
            path.to_string(),
            StoredSecret {
                encrypted_value: encrypted,
                nonce,
                key_id,
                version,
                metadata: metadata.clone(),
            },
        );
        debug!(path, version, "stored encrypted secret");
        Ok(metadata)
    }

    async fn delete_secret(&self, path: &str) -> Result<()> {
        let mut store = self.store.write().await;
        store
            .remove(path)
            .ok_or_else(|| SecretsError::not_found(path))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_master_key() -> String {
        base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            b"0123456789abcdef0123456789abcdef",
        )
    }

    #[tokio::test]
    async fn test_encrypt_decrypt_roundtrip() {
        let provider = BuiltinSecretsProvider::new(BuiltinConfig {
            master_key: Some(test_master_key()),
            key_id: Some("v1".into()),
        })
        .await
        .unwrap();

        let secret = SecretValue::new("my-secret-value");
        provider.put_secret("app/api-key", &secret).await.unwrap();

        let retrieved = provider.get_secret("app/api-key").await.unwrap();
        assert_eq!(retrieved.expose_secret(), "my-secret-value");
    }

    #[tokio::test]
    async fn test_key_rotation() {
        let mut provider = BuiltinSecretsProvider::new(BuiltinConfig {
            master_key: Some(test_master_key()),
            key_id: Some("v1".into()),
        })
        .await
        .unwrap();

        let secret = SecretValue::new("rotate-me");
        provider.put_secret("test/secret", &secret).await.unwrap();

        let new_key = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            b"abcdef0123456789abcdef0123456789",
        );
        let rotated = provider.rotate_master_key(&new_key, "v2").await.unwrap();
        assert_eq!(rotated, 1);

        let retrieved = provider.get_secret("test/secret").await.unwrap();
        assert_eq!(retrieved.expose_secret(), "rotate-me");
    }

    #[tokio::test]
    async fn test_not_found() {
        let provider = BuiltinSecretsProvider::new(BuiltinConfig {
            master_key: Some(test_master_key()),
            ..Default::default()
        })
        .await
        .unwrap();
        assert!(provider.get_secret("nonexistent").await.is_err());
    }

    #[tokio::test]
    async fn test_delete() {
        let provider = BuiltinSecretsProvider::new(BuiltinConfig {
            master_key: Some(test_master_key()),
            ..Default::default()
        })
        .await
        .unwrap();
        provider
            .put_secret("to-delete", &SecretValue::new("val"))
            .await
            .unwrap();
        provider.delete_secret("to-delete").await.unwrap();
        assert!(provider.get_secret("to-delete").await.is_err());
    }

    #[tokio::test]
    async fn test_no_key_fails() {
        let provider = BuiltinSecretsProvider::new(BuiltinConfig::default())
            .await
            .unwrap();
        assert!(provider.health_check().await.is_err());
    }
}
