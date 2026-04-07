//! Decrypt `met-output` secret envelopes (agent uses the matching seal in `met-agent`).

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use sha2::Sha256;
use x25519_dalek::{PublicKey, StaticSecret};

const HKDF_INFO: &[u8] = b"meticulous.met-output.v1";

pub fn open_secret_envelope(job_static_secret: &[u8; 32], envelope: &[u8]) -> Result<Vec<u8>, String> {
    if envelope.len() < 32 + 12 + 16 {
        return Err("short envelope".into());
    }
    let eph_bytes: [u8; 32] = envelope[..32].try_into().map_err(|_| "eph")?;
    let nonce_bytes: [u8; 12] = envelope[32..44].try_into().map_err(|_| "nonce")?;
    let ct = &envelope[44..];

    let eph_pk = PublicKey::from(eph_bytes);
    let ours = StaticSecret::from(*job_static_secret);
    let shared = ours.diffie_hellman(&eph_pk);

    let mut okm = [0u8; 32];
    hkdf::Hkdf::<Sha256>::new(None, shared.as_bytes())
        .expand(HKDF_INFO, &mut okm)
        .map_err(|_| "hkdf expand".to_string())?;

    let cipher = Aes256Gcm::new_from_slice(&okm).map_err(|e| e.to_string())?;
    let nonce = Nonce::from_slice(&nonce_bytes);
    cipher
        .decrypt(nonce, ct)
        .map_err(|_| "decrypt".to_string())
}
