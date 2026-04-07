//! Per-job PKI infrastructure.
//!
//! Provides intermediate CA management, ephemeral keypair generation for jobs,
//! CSR validation/signing, and hybrid encryption (X25519 + AES-256-GCM).

pub mod ca;
pub mod encryption;
pub mod ephemeral;

pub use ca::{CaConfig, CertificateAuthority, SignedCertificate};
pub use encryption::{EncryptedEnvelope, HybridDecryption, HybridEncryption};
pub use ephemeral::{CertificateSigningRequest, EphemeralKeypair};
