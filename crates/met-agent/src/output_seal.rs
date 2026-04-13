//! Wrap secret outputs for the controller using X25519 + AES-256-GCM (see design/workflow-invocation-outputs.md).

use aes_gcm::aead::{Aead, AeadCore, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use rand::rngs::OsRng;
use x25519_dalek::{EphemeralSecret, PublicKey, StaticSecret};

const HKDF_INFO: &[u8] = b"meticulous.met-output.v1";

/// One sealed secret: 32 (ephemeral pub) + 12 (nonce) + ciphertext (includes 16-byte GCM tag).
pub fn seal_secret_value(
    controller_public: &[u8; 32],
    plaintext: &[u8],
) -> Result<Vec<u8>, &'static str> {
    let server_pk = PublicKey::from(*controller_public);
    let ephemeral = EphemeralSecret::random_from_rng(OsRng);
    let eph_pub = PublicKey::from(&ephemeral);
    let shared = ephemeral.diffie_hellman(&server_pk);

    let mut okm = [0u8; 32];
    hkdf::Hkdf::<sha2::Sha256>::new(None, shared.as_bytes())
        .expand(HKDF_INFO, &mut okm)
        .map_err(|_| "hkdf expand")?;

    let cipher = Aes256Gcm::new_from_slice(&okm).map_err(|_| "aes key")?;
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let nonce_bytes = nonce.as_slice();
    let mut ct = cipher.encrypt(&nonce, plaintext).map_err(|_| "encrypt")?;

    let mut out = Vec::with_capacity(32 + 12 + ct.len());
    out.extend_from_slice(eph_pub.as_bytes());
    out.extend_from_slice(nonce_bytes);
    out.append(&mut ct);
    Ok(out)
}

/// Undo [`seal_secret_value`] given the job's static X25519 secret (32 bytes).
#[allow(dead_code)] // Used by tests / controller-side tooling when wired
pub fn open_secret_envelope(
    job_static_secret: &[u8; 32],
    envelope: &[u8],
) -> Result<Vec<u8>, &'static str> {
    if envelope.len() < 32 + 12 + 16 {
        return Err("short envelope");
    }
    let eph_bytes: [u8; 32] = envelope[..32].try_into().map_err(|_| "eph")?;
    let nonce_bytes: [u8; 12] = envelope[32..44].try_into().map_err(|_| "nonce")?;
    let ct = &envelope[44..];

    let eph_pk = PublicKey::from(eph_bytes);
    let ours = StaticSecret::from(*job_static_secret);
    let shared = ours.diffie_hellman(&eph_pk);

    let mut okm = [0u8; 32];
    hkdf::Hkdf::<sha2::Sha256>::new(None, shared.as_bytes())
        .expand(HKDF_INFO, &mut okm)
        .map_err(|_| "hkdf expand")?;

    let cipher = Aes256Gcm::new_from_slice(&okm).map_err(|_| "aes key")?;
    let nonce = Nonce::from_slice(&nonce_bytes);
    cipher.decrypt(nonce, ct).map_err(|_| "decrypt")
}
