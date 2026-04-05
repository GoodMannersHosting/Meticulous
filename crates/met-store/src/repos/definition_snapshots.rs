//! Content-addressed pipeline / workflow definition bodies (deduplicated by SHA-256).

use sha2::{Digest, Sha256};
use sqlx::PgPool;

use crate::error::{Result, StoreError};

/// Store and resolve definition JSON by content hash.
pub struct DefinitionSnapshotRepo;

impl DefinitionSnapshotRepo {
    /// SHA-256 of `serde_json` canonical UTF-8 bytes (key order as serialized).
    #[must_use]
    pub fn digest_json(value: &serde_json::Value) -> Result<[u8; 32]> {
        let bytes =
            serde_json::to_vec(value).map_err(|e| StoreError::validation(e.to_string()))?;
        Ok(Sha256::digest(bytes).into())
    }

    /// Insert snapshot if missing; always returns the digest for `body`.
    pub async fn ensure_json(pool: &PgPool, body: &serde_json::Value) -> Result<[u8; 32]> {
        let digest = Self::digest_json(body)?;
        sqlx::query(
            r#"
            INSERT INTO definition_snapshots (content_sha256, body)
            VALUES ($1, $2)
            ON CONFLICT (content_sha256) DO NOTHING
            "#,
        )
        .bind(&digest[..])
        .bind(body)
        .execute(pool)
        .await?;
        Ok(digest)
    }

    /// Load a stored JSON body by content hash (UTF-8 pipeline / workflow snapshot).
    pub async fn get_json(pool: &PgPool, content_sha256: &[u8; 32]) -> Result<Option<serde_json::Value>> {
        sqlx::query_scalar::<_, serde_json::Value>(
            r#"
            SELECT body FROM definition_snapshots WHERE content_sha256 = $1
            "#,
        )
        .bind(&content_sha256[..])
        .fetch_optional(pool)
        .await
        .map_err(Into::into)
    }
}
