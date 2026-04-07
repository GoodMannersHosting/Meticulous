//! AES-256-GCM envelope helpers for platform-stored secrets (HKDF from master key).
//!
//! Same derivation as [`crate::providers::BuiltinSecretsProvider`]. Master key is supplied via
//! configuration or environment at runtime — never embedded in source.

use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit, OsRng},
};
use hkdf::Hkdf;
use rand::RngCore;
use sha2::Sha256;
use zeroize::Zeroizing;

use crate::error::{Result, SecretsError};

const NONCE_SIZE: usize = 12;

/// HKDF info string shared with built-in provider (must stay stable for ciphertext compatibility).
pub const BUILTIN_SECRETS_HKDF_INFO: &[u8] = b"meticulous-builtin-secrets-v1";

/// Derive a DEK from a base64-encoded master key and perform AES-256-GCM encrypt/decrypt.
#[derive(Debug)]
pub struct BuiltinStoredCrypto {
    key: Zeroizing<[u8; 32]>,
    key_id: String,
}

impl BuiltinStoredCrypto {
    /// Build crypto helper from base64 master key material (16+ bytes after decode).
    pub fn from_master_key_b64(master_key_b64: &str, key_id: Option<&str>) -> Result<Self> {
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

        Ok(Self {
            key,
            key_id: key_id.unwrap_or("v1").to_string(),
        })
    }

    /// Current logical key id label (stored alongside ciphertext rows).
    #[must_use]
    pub fn key_id(&self) -> &str {
        &self.key_id
    }

    /// Encrypt plaintext; returns (ciphertext, nonce bytes, key_id).
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<(Vec<u8>, [u8; NONCE_SIZE], String)> {
        let mut nonce_bytes = [0u8; NONCE_SIZE];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let cipher = Aes256Gcm::new_from_slice(self.key.as_ref())
            .map_err(|e| SecretsError::Crypto(format!("AES init: {e}")))?;
        let ciphertext = cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| SecretsError::Crypto(format!("encryption failed: {e}")))?;

        Ok((ciphertext, nonce_bytes, self.key_id.clone()))
    }

    /// Decrypt ciphertext (ignores row `key_id` for now; single active master per process).
    pub fn decrypt(
        &self,
        ciphertext: &[u8],
        nonce: &[u8; NONCE_SIZE],
    ) -> Result<Zeroizing<Vec<u8>>> {
        let n = Nonce::from_slice(nonce.as_slice());
        let cipher = Aes256Gcm::new_from_slice(self.key.as_ref())
            .map_err(|e| SecretsError::Crypto(format!("AES init: {e}")))?;
        let plaintext = cipher
            .decrypt(n, ciphertext)
            .map_err(|e| SecretsError::Crypto(format!("decryption failed: {e}")))?;

        Ok(Zeroizing::new(plaintext))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_key_b64() -> String {
        base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            b"0123456789abcdef0123456789abcdef",
        )
    }

    #[test]
    fn roundtrip() {
        let c = BuiltinStoredCrypto::from_master_key_b64(&sample_key_b64(), Some("k1")).unwrap();
        let (ct, nonce, kid) = c.encrypt(b"hello").unwrap();
        assert_eq!(kid, "k1");
        let pt = c.decrypt(&ct, &nonce).unwrap();
        assert_eq!(pt.as_slice(), b"hello");
    }
}
