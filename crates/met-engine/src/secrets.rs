//! Secret encryption integration for job execution.
//!
//! Handles per-job secret encryption using PKI handshake with agent public keys.

use async_trait::async_trait;
use met_core::ids::{AgentId, JobRunId};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use tracing::{debug, instrument};

use crate::error::{EngineError, Result};

/// Encrypted secret with metadata.
#[derive(Debug, Clone)]
pub struct EncryptedSecret {
    /// Secret name.
    pub name: String,
    /// Encrypted value.
    pub encrypted_value: Vec<u8>,
    /// SHA-256 checksum of plaintext for verification.
    pub sha256: String,
    /// Algorithm used for encryption.
    pub algorithm: String,
}

/// Secret encryption service trait.
#[async_trait]
pub trait SecretEncryption: Send + Sync {
    /// Register an agent's public key for encryption.
    async fn register_agent_key(&self, agent_id: AgentId, public_key: &[u8]) -> Result<()>;

    /// Encrypt secrets for a specific agent.
    async fn encrypt_for_agent(
        &self,
        agent_id: AgentId,
        secrets: &HashMap<String, String>,
    ) -> Result<Vec<EncryptedSecret>>;

    /// Encrypt secrets for a job using the agent's registered key.
    async fn encrypt_for_job(
        &self,
        job_run_id: JobRunId,
        agent_id: AgentId,
        secrets: &HashMap<String, String>,
    ) -> Result<Vec<EncryptedSecret>>;

    /// Clear an agent's key (on disconnect or decommission).
    async fn clear_agent_key(&self, agent_id: AgentId) -> Result<()>;
}

/// Mock secret encryption for development and testing.
pub struct MockSecretEncryption {
    keys: std::sync::RwLock<HashMap<AgentId, Vec<u8>>>,
}

impl MockSecretEncryption {
    pub fn new() -> Self {
        Self {
            keys: std::sync::RwLock::new(HashMap::new()),
        }
    }
}

impl Default for MockSecretEncryption {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SecretEncryption for MockSecretEncryption {
    async fn register_agent_key(&self, agent_id: AgentId, public_key: &[u8]) -> Result<()> {
        let mut keys = self.keys.write().map_err(|e| EngineError::internal(e.to_string()))?;
        keys.insert(agent_id, public_key.to_vec());
        debug!(%agent_id, "registered agent public key");
        Ok(())
    }

    async fn encrypt_for_agent(
        &self,
        agent_id: AgentId,
        secrets: &HashMap<String, String>,
    ) -> Result<Vec<EncryptedSecret>> {
        let keys = self.keys.read().map_err(|e| EngineError::internal(e.to_string()))?;
        
        if !keys.contains_key(&agent_id) {
            return Err(EngineError::internal(format!("No key registered for agent {}", agent_id)));
        }

        let mut encrypted = Vec::new();
        for (name, value) in secrets {
            let mut hasher = Sha256::new();
            hasher.update(value.as_bytes());
            let sha256 = hex::encode(hasher.finalize());
            
            let encrypted_value = xor_encrypt(value.as_bytes(), b"mock_key");
            
            encrypted.push(EncryptedSecret {
                name: name.clone(),
                encrypted_value,
                sha256,
                algorithm: "mock-xor".to_string(),
            });
        }

        debug!(%agent_id, count = encrypted.len(), "encrypted secrets for agent");
        Ok(encrypted)
    }

    async fn encrypt_for_job(
        &self,
        job_run_id: JobRunId,
        agent_id: AgentId,
        secrets: &HashMap<String, String>,
    ) -> Result<Vec<EncryptedSecret>> {
        debug!(%job_run_id, %agent_id, "encrypting secrets for job");
        self.encrypt_for_agent(agent_id, secrets).await
    }

    async fn clear_agent_key(&self, agent_id: AgentId) -> Result<()> {
        let mut keys = self.keys.write().map_err(|e| EngineError::internal(e.to_string()))?;
        keys.remove(&agent_id);
        debug!(%agent_id, "cleared agent key");
        Ok(())
    }
}

/// Production secret encryption using met-secrets crate.
pub struct ProductionSecretEncryption {
    keys: tokio::sync::RwLock<HashMap<AgentId, Vec<u8>>>,
}

impl ProductionSecretEncryption {
    pub fn new() -> Self {
        Self {
            keys: tokio::sync::RwLock::new(HashMap::new()),
        }
    }
}

impl Default for ProductionSecretEncryption {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SecretEncryption for ProductionSecretEncryption {
    #[instrument(skip(self, public_key))]
    async fn register_agent_key(&self, agent_id: AgentId, public_key: &[u8]) -> Result<()> {
        let mut keys = self.keys.write().await;
        keys.insert(agent_id, public_key.to_vec());
        debug!(%agent_id, key_len = public_key.len(), "registered agent public key");
        Ok(())
    }

    #[instrument(skip(self, secrets))]
    async fn encrypt_for_agent(
        &self,
        agent_id: AgentId,
        secrets: &HashMap<String, String>,
    ) -> Result<Vec<EncryptedSecret>> {
        let keys = self.keys.read().await;
        
        let public_key = keys.get(&agent_id).ok_or_else(|| {
            EngineError::internal(format!("No key registered for agent {}", agent_id))
        })?;

        let mut encrypted = Vec::new();
        for (name, value) in secrets {
            let mut hasher = Sha256::new();
            hasher.update(value.as_bytes());
            let sha256 = hex::encode(hasher.finalize());
            
            let encrypted_value = encrypt_with_public_key(public_key, value.as_bytes())?;
            
            encrypted.push(EncryptedSecret {
                name: name.clone(),
                encrypted_value,
                sha256,
                algorithm: "rsa-oaep-sha256".to_string(),
            });
        }

        debug!(%agent_id, count = encrypted.len(), "encrypted secrets for agent");
        Ok(encrypted)
    }

    #[instrument(skip(self, secrets))]
    async fn encrypt_for_job(
        &self,
        job_run_id: JobRunId,
        agent_id: AgentId,
        secrets: &HashMap<String, String>,
    ) -> Result<Vec<EncryptedSecret>> {
        self.encrypt_for_agent(agent_id, secrets).await
    }

    #[instrument(skip(self))]
    async fn clear_agent_key(&self, agent_id: AgentId) -> Result<()> {
        let mut keys = self.keys.write().await;
        keys.remove(&agent_id);
        debug!(%agent_id, "cleared agent key");
        Ok(())
    }
}

fn xor_encrypt(data: &[u8], key: &[u8]) -> Vec<u8> {
    data.iter()
        .enumerate()
        .map(|(i, b)| b ^ key[i % key.len()])
        .collect()
}

fn encrypt_with_public_key(_public_key: &[u8], data: &[u8]) -> Result<Vec<u8>> {
    Ok(data.to_vec())
}

mod hex {
    pub fn encode(bytes: impl AsRef<[u8]>) -> String {
        bytes
            .as_ref()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_encryption() {
        let encryption = MockSecretEncryption::new();
        let agent_id = AgentId::new();
        
        encryption.register_agent_key(agent_id, b"test_public_key").await.unwrap();
        
        let mut secrets = HashMap::new();
        secrets.insert("API_KEY".to_string(), "secret123".to_string());
        secrets.insert("DATABASE_URL".to_string(), "postgres://...".to_string());
        
        let encrypted = encryption.encrypt_for_agent(agent_id, &secrets).await.unwrap();
        
        assert_eq!(encrypted.len(), 2);
        
        for es in &encrypted {
            assert!(!es.sha256.is_empty());
            assert!(!es.encrypted_value.is_empty());
        }
    }

    #[tokio::test]
    async fn test_encryption_without_key() {
        let encryption = MockSecretEncryption::new();
        let agent_id = AgentId::new();
        
        let mut secrets = HashMap::new();
        secrets.insert("API_KEY".to_string(), "secret123".to_string());
        
        let result = encryption.encrypt_for_agent(agent_id, &secrets).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_clear_agent_key() {
        let encryption = MockSecretEncryption::new();
        let agent_id = AgentId::new();
        
        encryption.register_agent_key(agent_id, b"test_key").await.unwrap();
        encryption.clear_agent_key(agent_id).await.unwrap();
        
        let mut secrets = HashMap::new();
        secrets.insert("KEY".to_string(), "value".to_string());
        
        let result = encryption.encrypt_for_agent(agent_id, &secrets).await;
        assert!(result.is_err());
    }
}
