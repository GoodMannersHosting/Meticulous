//! Hybrid encryption using X25519 key exchange + AES-256-GCM.
//!
//! The encryption flow:
//! 1. Sender generates ephemeral X25519 keypair
//! 2. Sender computes shared secret via ECDH with recipient's X25519 public key
//! 3. Shared secret is expanded via HKDF-SHA256 into a 256-bit AES key
//! 4. Plaintext is encrypted with AES-256-GCM
//! 5. Envelope contains: sender's ephemeral public key + nonce + ciphertext + tag

use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use rand::RngCore;
use sha2::Sha256;
use x25519_dalek::{EphemeralSecret, PublicKey, StaticSecret};
use zeroize::Zeroizing;

use crate::error::SecretsError;

type HmacSha256 = Hmac<Sha256>;

fn new_hmac(key: &[u8]) -> Result<HmacSha256, SecretsError> {
    <HmacSha256 as KeyInit>::new_from_slice(key)
        .map_err(|e| SecretsError::Crypto(format!("HMAC init failed: {e}")))
}

const HKDF_INFO: &[u8] = b"meticulous-pki-hybrid-encryption-v1";
const NONCE_SIZE: usize = 12;

/// An encrypted envelope containing a single secret.
#[derive(Debug, Clone)]
pub struct EncryptedEnvelope {
    /// Sender's ephemeral X25519 public key (32 bytes).
    pub ephemeral_public_key: [u8; 32],
    /// AES-256-GCM nonce (12 bytes).
    pub nonce: [u8; NONCE_SIZE],
    /// Encrypted ciphertext with appended GCM tag.
    pub ciphertext: Vec<u8>,
    /// HMAC-SHA256 of the plaintext for integrity verification after decryption.
    pub plaintext_hmac: [u8; 32],
}

impl EncryptedEnvelope {
    /// Serialize to bytes: ephemeral_pk (32) || nonce (12) || hmac (32) || ciphertext_len (4) || ciphertext
    pub fn to_bytes(&self) -> Vec<u8> {
        let ct_len = (self.ciphertext.len() as u32).to_be_bytes();
        let mut buf = Vec::with_capacity(32 + NONCE_SIZE + 32 + 4 + self.ciphertext.len());
        buf.extend_from_slice(&self.ephemeral_public_key);
        buf.extend_from_slice(&self.nonce);
        buf.extend_from_slice(&self.plaintext_hmac);
        buf.extend_from_slice(&ct_len);
        buf.extend_from_slice(&self.ciphertext);
        buf
    }

    /// Deserialize from bytes.
    pub fn from_bytes(data: &[u8]) -> Result<Self, SecretsError> {
        if data.len() < 32 + NONCE_SIZE + 32 + 4 {
            return Err(SecretsError::Crypto("encrypted envelope too short".into()));
        }

        let mut ephemeral_public_key = [0u8; 32];
        ephemeral_public_key.copy_from_slice(&data[..32]);

        let mut nonce = [0u8; NONCE_SIZE];
        nonce.copy_from_slice(&data[32..32 + NONCE_SIZE]);

        let mut plaintext_hmac = [0u8; 32];
        plaintext_hmac.copy_from_slice(&data[32 + NONCE_SIZE..32 + NONCE_SIZE + 32]);

        let ct_len_start = 32 + NONCE_SIZE + 32;
        let mut ct_len_bytes = [0u8; 4];
        ct_len_bytes.copy_from_slice(&data[ct_len_start..ct_len_start + 4]);
        let ct_len = u32::from_be_bytes(ct_len_bytes) as usize;

        let ct_start = ct_len_start + 4;
        if data.len() < ct_start + ct_len {
            return Err(SecretsError::Crypto("encrypted envelope truncated".into()));
        }

        let ciphertext = data[ct_start..ct_start + ct_len].to_vec();

        Ok(Self {
            ephemeral_public_key,
            nonce,
            ciphertext,
            plaintext_hmac,
        })
    }
}

/// Hybrid encryption for the server/broker side (encrypts secrets for agents).
pub struct HybridEncryption;

impl HybridEncryption {
    /// Encrypt a secret for a recipient identified by their X25519 public key.
    pub fn encrypt(
        recipient_public_key: &[u8; 32],
        plaintext: &[u8],
        hmac_key: &[u8],
    ) -> Result<EncryptedEnvelope, SecretsError> {
        let recipient_pk = PublicKey::from(*recipient_public_key);

        let sender_secret = EphemeralSecret::random_from_rng(OsRng);
        let sender_public = PublicKey::from(&sender_secret);

        let shared_secret = sender_secret.diffie_hellman(&recipient_pk);

        let aes_key = derive_aes_key(shared_secret.as_bytes())?;

        let mut nonce_bytes = [0u8; NONCE_SIZE];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let cipher = Aes256Gcm::new_from_slice(&aes_key)
            .map_err(|e| SecretsError::Crypto(format!("AES key init failed: {e}")))?;
        let ciphertext = cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| SecretsError::Crypto(format!("AES-GCM encryption failed: {e}")))?;

        let mut mac = new_hmac(hmac_key)?;
        mac.update(plaintext);
        let hmac_result = mac.finalize().into_bytes();
        let mut plaintext_hmac = [0u8; 32];
        plaintext_hmac.copy_from_slice(&hmac_result);

        Ok(EncryptedEnvelope {
            ephemeral_public_key: sender_public.to_bytes(),
            nonce: nonce_bytes,
            ciphertext,
            plaintext_hmac,
        })
    }
}

/// Hybrid decryption for the agent side.
pub struct HybridDecryption;

impl HybridDecryption {
    /// Decrypt an envelope using the recipient's X25519 private key.
    pub fn decrypt(
        recipient_private_key: &[u8; 32],
        envelope: &EncryptedEnvelope,
        hmac_key: &[u8],
    ) -> Result<Zeroizing<Vec<u8>>, SecretsError> {
        let recipient_secret = StaticSecret::from(*recipient_private_key);
        let sender_pk = PublicKey::from(envelope.ephemeral_public_key);

        let shared_secret = recipient_secret.diffie_hellman(&sender_pk);
        let aes_key = derive_aes_key(shared_secret.as_bytes())?;

        let nonce = Nonce::from_slice(&envelope.nonce);
        let cipher = Aes256Gcm::new_from_slice(&aes_key)
            .map_err(|e| SecretsError::Crypto(format!("AES key init failed: {e}")))?;

        let plaintext = cipher
            .decrypt(nonce, envelope.ciphertext.as_ref())
            .map_err(|e| SecretsError::Crypto(format!("AES-GCM decryption failed: {e}")))?;

        let mut mac = new_hmac(hmac_key)?;
        mac.update(&plaintext);
        mac.verify_slice(&envelope.plaintext_hmac)
            .map_err(|_| SecretsError::Crypto("HMAC verification failed: secret integrity compromised".into()))?;

        Ok(Zeroizing::new(plaintext))
    }
}

fn derive_aes_key(shared_secret: &[u8]) -> Result<[u8; 32], SecretsError> {
    let hk = Hkdf::<Sha256>::new(None, shared_secret);
    let mut key = [0u8; 32];
    hk.expand(HKDF_INFO, &mut key)
        .map_err(|e| SecretsError::Crypto(format!("HKDF expand failed: {e}")))?;
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let recipient_secret = StaticSecret::random_from_rng(OsRng);
        let recipient_public = PublicKey::from(&recipient_secret);
        let hmac_key = b"test-hmac-key-for-verification!!";

        let plaintext = b"super-secret-api-key-12345";

        let envelope = HybridEncryption::encrypt(
            &recipient_public.to_bytes(),
            plaintext,
            hmac_key,
        ).unwrap();

        let decrypted = HybridDecryption::decrypt(
            &recipient_secret.to_bytes(),
            &envelope,
            hmac_key,
        ).unwrap();

        assert_eq!(&*decrypted, plaintext);
    }

    #[test]
    fn test_wrong_key_fails() {
        let recipient_secret = StaticSecret::random_from_rng(OsRng);
        let recipient_public = PublicKey::from(&recipient_secret);
        let wrong_secret = StaticSecret::random_from_rng(OsRng);
        let hmac_key = b"test-hmac-key-for-verification!!";

        let envelope = HybridEncryption::encrypt(
            &recipient_public.to_bytes(),
            b"secret",
            hmac_key,
        ).unwrap();

        let result = HybridDecryption::decrypt(
            &wrong_secret.to_bytes(),
            &envelope,
            hmac_key,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_envelope_serialization_roundtrip() {
        let recipient_secret = StaticSecret::random_from_rng(OsRng);
        let recipient_public = PublicKey::from(&recipient_secret);
        let hmac_key = b"test-hmac-key-for-verification!!";

        let envelope = HybridEncryption::encrypt(
            &recipient_public.to_bytes(),
            b"another-secret",
            hmac_key,
        ).unwrap();

        let bytes = envelope.to_bytes();
        let restored = EncryptedEnvelope::from_bytes(&bytes).unwrap();

        let decrypted = HybridDecryption::decrypt(
            &recipient_secret.to_bytes(),
            &restored,
            hmac_key,
        ).unwrap();

        assert_eq!(&*decrypted, b"another-secret");
    }

    #[test]
    fn test_tampered_hmac_fails() {
        let recipient_secret = StaticSecret::random_from_rng(OsRng);
        let recipient_public = PublicKey::from(&recipient_secret);
        let hmac_key = b"test-hmac-key-for-verification!!";

        let mut envelope = HybridEncryption::encrypt(
            &recipient_public.to_bytes(),
            b"secret",
            hmac_key,
        ).unwrap();

        envelope.plaintext_hmac[0] ^= 0xff;

        let result = HybridDecryption::decrypt(
            &recipient_secret.to_bytes(),
            &envelope,
            hmac_key,
        );
        assert!(result.is_err());
    }
}
