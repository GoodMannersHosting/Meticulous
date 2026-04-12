//! OIDC workload identity signing keys (ADR-017): P-256 ES256 key generation and encrypted storage.

use base64::Engine;
use chrono::{DateTime, Duration, Utc};
use p256::ecdsa::{SigningKey, VerifyingKey};
use pkcs8::EncodePrivateKey;
use rand_core::{OsRng, RngCore};
use serde_json::{Value, json};
use zeroize::Zeroizing;

use crate::error::SecretsError;
use crate::stored_crypto::BuiltinStoredCrypto;

const STORAGE_V1: u8 = 1;

/// Encrypted PKCS#8 layout: `[version:1][nonce:12][ciphertext]`.
pub fn encrypt_pkcs8_private_key(
    crypto: &BuiltinStoredCrypto,
    pkcs8_der: &[u8],
) -> Result<Vec<u8>, SecretsError> {
    let (ct, nonce, _kid) = crypto.encrypt(pkcs8_der)?;
    let mut out = Vec::with_capacity(1 + 12 + ct.len());
    out.push(STORAGE_V1);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ct);
    Ok(out)
}

/// Decrypt PKCS#8 DER bytes for ES256 signing.
pub fn decrypt_pkcs8_private_key(
    crypto: &BuiltinStoredCrypto,
    blob: &[u8],
) -> Result<Zeroizing<Vec<u8>>, SecretsError> {
    if blob.len() < 1 + 12 + 1 {
        return Err(SecretsError::Crypto(
            "oidc private_key_enc too short".into(),
        ));
    }
    if blob[0] != STORAGE_V1 {
        return Err(SecretsError::Crypto(
            "unsupported oidc private_key_enc version".into(),
        ));
    }
    let mut nonce = [0u8; 12];
    nonce.copy_from_slice(&blob[1..13]);
    let ct = &blob[13..];
    crypto.decrypt(ct, &nonce)
}

/// New workload identity signing material for `oidc_signing_keys`.
pub struct GeneratedOidcSigningKey {
    /// Key id (base64url, no padding).
    pub kid: String,
    pub private_key_enc: Vec<u8>,
    pub public_key_jwk: Value,
    pub expires_at: DateTime<Utc>,
}

/// Generate a P-256 signing key, encrypt the PKCS#8 private key, and build the public JWK (`ES256`).
pub fn generate_oidc_signing_key(
    crypto: &BuiltinStoredCrypto,
    lifetime: Duration,
) -> Result<GeneratedOidcSigningKey, SecretsError> {
    let signing_key = SigningKey::random(&mut OsRng);
    let pkcs8_der = signing_key
        .to_pkcs8_der()
        .map_err(|e| SecretsError::Crypto(format!("pkcs8 encode: {e}")))?;
    let pkcs8_bytes = pkcs8_der.as_bytes();

    let mut kid_raw = [0u8; 16];
    OsRng.fill_bytes(&mut kid_raw);
    let kid = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(kid_raw);

    let private_key_enc = encrypt_pkcs8_private_key(crypto, pkcs8_bytes)?;

    let verifying_key = VerifyingKey::from(&signing_key);
    let ep = verifying_key.to_encoded_point(false);
    let x = ep
        .x()
        .ok_or_else(|| SecretsError::Crypto("missing x coordinate".into()))?;
    let y = ep
        .y()
        .ok_or_else(|| SecretsError::Crypto("missing y coordinate".into()))?;

    let x_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(x.as_slice());
    let y_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(y.as_slice());

    let public_key_jwk = json!({
        "kty": "EC",
        "crv": "P-256",
        "kid": kid,
        "use": "sig",
        "alg": "ES256",
        "x": x_b64,
        "y": y_b64,
    });

    let expires_at = Utc::now() + lifetime;

    Ok(GeneratedOidcSigningKey {
        kid,
        private_key_enc,
        public_key_jwk,
        expires_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_master_b64() -> String {
        base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            b"0123456789abcdef0123456789abcdef",
        )
    }

    #[test]
    fn generate_and_decrypt_roundtrip() {
        let crypto = BuiltinStoredCrypto::from_master_key_b64(&sample_master_b64(), None).unwrap();
        let generated = generate_oidc_signing_key(&crypto, Duration::days(90)).unwrap();
        assert!(generated.kid.len() >= 16);
        assert!(generated.public_key_jwk.get("x").is_some());

        let pkcs8 = decrypt_pkcs8_private_key(&crypto, &generated.private_key_enc).unwrap();
        assert!(!pkcs8.is_empty());
    }
}
