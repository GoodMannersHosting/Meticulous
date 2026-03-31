//! Ephemeral keypair generation for per-job PKI.

use rcgen::KeyPair;
use sha2::{Digest, Sha256};
use tracing::debug;
use zeroize::Zeroizing;

use crate::error::SecretsError;

/// An ephemeral keypair for a single job.
///
/// The private key is zeroized on drop. The keypair is used for:
/// 1. CSR generation (sent to controller)
/// 2. Decrypting secrets encrypted with the public key
pub struct EphemeralKeypair {
    key_pair: KeyPair,
    private_key_der: Zeroizing<Vec<u8>>,
}

impl EphemeralKeypair {
    /// Generate a new ephemeral keypair.
    pub fn generate() -> Result<Self, SecretsError> {
        let key_pair = KeyPair::generate()
            .map_err(|e| SecretsError::Crypto(format!("ephemeral key generation failed: {e}")))?;
        let private_key_der = Zeroizing::new(key_pair.serialize_der().to_vec());

        debug!("Generated ephemeral keypair for job PKI");
        Ok(Self {
            key_pair,
            private_key_der,
        })
    }

    /// Get the public key in DER format (for CSR/encryption).
    pub fn public_key_der(&self) -> Vec<u8> {
        self.key_pair.public_key_der().to_vec()
    }

    /// Get the SHA-256 fingerprint of the public key.
    pub fn public_key_fingerprint(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.key_pair.public_key_der());
        hex::encode(hasher.finalize())
    }

    /// Get the private key DER bytes (for decryption).
    pub fn private_key_der(&self) -> &[u8] {
        &self.private_key_der
    }

    /// Get the full serialized key pair in DER format (PKCS#8).
    ///
    /// This is needed for the CA to reconstruct the `rcgen::KeyPair` when
    /// signing the certificate. In a real PKI flow this would be a proper
    /// CSR; here we pass the full key pair because `rcgen` requires it for
    /// `CertificateParams::signed_by`.
    pub fn key_pair_der(&self) -> &[u8] {
        &self.private_key_der
    }
}

impl std::fmt::Debug for EphemeralKeypair {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EphemeralKeypair")
            .field("fingerprint", &self.public_key_fingerprint())
            .finish()
    }
}

/// A Certificate Signing Request to send to the controller.
#[derive(Debug, Clone)]
pub struct CertificateSigningRequest {
    /// The public key in DER format.
    pub public_key_der: Vec<u8>,
    /// SHA-256 fingerprint of the public key.
    pub fingerprint: String,
    /// The agent ID making the request.
    pub agent_id: String,
    /// The job ID this CSR is for.
    pub job_id: String,
}

impl CertificateSigningRequest {
    /// Create a CSR from an ephemeral keypair.
    pub fn new(keypair: &EphemeralKeypair, agent_id: String, job_id: String) -> Self {
        Self {
            public_key_der: keypair.public_key_der(),
            fingerprint: keypair.public_key_fingerprint(),
            agent_id,
            job_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ephemeral_keypair_generation() {
        let kp = EphemeralKeypair::generate().unwrap();
        assert!(!kp.public_key_der().is_empty());
        assert!(!kp.public_key_fingerprint().is_empty());
        assert!(!kp.private_key_der().is_empty());
    }

    #[test]
    fn test_csr_creation() {
        let kp = EphemeralKeypair::generate().unwrap();
        let csr = CertificateSigningRequest::new(&kp, "agent-1".into(), "job-1".into());
        assert_eq!(csr.agent_id, "agent-1");
        assert_eq!(csr.job_id, "job-1");
        assert_eq!(csr.fingerprint, kp.public_key_fingerprint());
    }

    #[test]
    fn test_different_keypairs_have_different_fingerprints() {
        let kp1 = EphemeralKeypair::generate().unwrap();
        let kp2 = EphemeralKeypair::generate().unwrap();
        assert_ne!(kp1.public_key_fingerprint(), kp2.public_key_fingerprint());
    }
}
