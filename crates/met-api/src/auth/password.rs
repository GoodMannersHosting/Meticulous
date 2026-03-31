//! Password hashing and verification using Argon2id.

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use thiserror::Error;

/// Errors that can occur during password operations.
#[derive(Debug, Error)]
pub enum PasswordError {
    /// Failed to hash password.
    #[error("failed to hash password: {0}")]
    HashError(String),

    /// Failed to verify password.
    #[error("failed to verify password: {0}")]
    VerifyError(String),

    /// Password is invalid (wrong password).
    #[error("invalid password")]
    Invalid,
}

/// Hash a password using Argon2id.
pub fn hash_password(password: &str) -> Result<String, PasswordError> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();

    let hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| PasswordError::HashError(e.to_string()))?;

    Ok(hash.to_string())
}

/// Verify a password against a hash.
pub fn verify_password(password: &str, hash: &str) -> Result<(), PasswordError> {
    let parsed_hash =
        PasswordHash::new(hash).map_err(|e| PasswordError::VerifyError(e.to_string()))?;

    Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .map_err(|_| PasswordError::Invalid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_and_verify() {
        let password = "secure-password-123";
        let hash = hash_password(password).unwrap();

        assert!(hash.starts_with("$argon2"));
        assert!(verify_password(password, &hash).is_ok());
    }

    #[test]
    fn test_wrong_password() {
        let password = "correct-password";
        let hash = hash_password(password).unwrap();

        let result = verify_password("wrong-password", &hash);
        assert!(matches!(result, Err(PasswordError::Invalid)));
    }

    #[test]
    fn test_unique_salts() {
        let password = "same-password";
        let hash1 = hash_password(password).unwrap();
        let hash2 = hash_password(password).unwrap();

        assert_ne!(hash1, hash2);
        assert!(verify_password(password, &hash1).is_ok());
        assert!(verify_password(password, &hash2).is_ok());
    }
}
