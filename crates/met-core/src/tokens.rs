//! High-entropy token hashing (join tokens, API tokens).

use sha2::{Digest, Sha256};

/// SHA-256 hex digest of a token for storage and lookup.
///
/// Used for join tokens and API tokens. Plaintext tokens are random/high-entropy;
/// a fast hash is sufficient (no need for Argon2/bcrypt).
#[must_use]
pub fn hash_join_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_join_token_stable() {
        let h = hash_join_token("met_join_test");
        assert_eq!(h.len(), 64);
        assert_eq!(h, hash_join_token("met_join_test"));
    }
}
