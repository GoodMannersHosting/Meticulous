//! Repository for OIDC signing keys and token audit (ADR-017, Phase 2.2).

use met_secrets::BuiltinStoredCrypto;
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{Result, StoreError};

/// OIDC signing key row.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct OidcSigningKeyRow {
    pub id: Uuid,
    pub kid: String,
    pub private_key_enc: Vec<u8>,
    pub public_key_jwk: serde_json::Value,
    pub algorithm: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub revoked_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Public key metadata for JWKS responses (no private key).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct OidcPublicKeyRow {
    pub kid: String,
    pub public_key_jwk: serde_json::Value,
    pub algorithm: String,
}

/// Token audit row.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct OidcTokenAuditRow {
    pub id: Uuid,
    pub job_run_id: Uuid,
    pub agent_id: Uuid,
    pub audience: String,
    pub kid: String,
    pub jti: Uuid,
    pub issued_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

pub struct OidcSigningKeyRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> OidcSigningKeyRepo<'a> {
    #[must_use]
    pub const fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Get the active signing key (newest non-revoked, non-expired).
    pub async fn active_key(&self) -> Result<Option<OidcSigningKeyRow>> {
        sqlx::query_as::<_, OidcSigningKeyRow>(
            r#"
            SELECT id, kid, private_key_enc, public_key_jwk, algorithm, created_at, expires_at, revoked_at
            FROM oidc_signing_keys
            WHERE revoked_at IS NULL AND expires_at > NOW()
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .fetch_optional(self.pool)
        .await
        .map_err(Into::into)
    }

    /// All non-revoked, non-expired public keys for the JWKS endpoint.
    pub async fn jwks_public_keys(&self) -> Result<Vec<OidcPublicKeyRow>> {
        sqlx::query_as::<_, OidcPublicKeyRow>(
            r#"
            SELECT kid, public_key_jwk, algorithm
            FROM oidc_signing_keys
            WHERE revoked_at IS NULL AND expires_at > NOW()
            ORDER BY created_at DESC
            "#,
        )
        .fetch_all(self.pool)
        .await
        .map_err(Into::into)
    }

    /// Insert a new signing key.
    pub async fn insert(
        &self,
        kid: &str,
        private_key_enc: &[u8],
        public_key_jwk: &serde_json::Value,
        expires_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<OidcSigningKeyRow> {
        sqlx::query_as::<_, OidcSigningKeyRow>(
            r#"
            INSERT INTO oidc_signing_keys (kid, private_key_enc, public_key_jwk, expires_at)
            VALUES ($1, $2, $3, $4)
            RETURNING id, kid, private_key_enc, public_key_jwk, algorithm, created_at, expires_at, revoked_at
            "#,
        )
        .bind(kid)
        .bind(private_key_enc)
        .bind(public_key_jwk)
        .bind(expires_at)
        .fetch_one(self.pool)
        .await
        .map_err(Into::into)
    }

    /// Revoke a key by kid (immediate removal from JWKS).
    pub async fn revoke(&self, kid: &str) -> Result<()> {
        let r = sqlx::query(
            "UPDATE oidc_signing_keys SET revoked_at = NOW() WHERE kid = $1 AND revoked_at IS NULL",
        )
        .bind(kid)
        .execute(self.pool)
        .await?;
        if r.rows_affected() == 0 {
            return Err(StoreError::not_found("oidc_signing_key", kid));
        }
        Ok(())
    }

    /// Record a token issuance for audit.
    pub async fn audit_token(
        &self,
        job_run_id: Uuid,
        agent_id: Uuid,
        audience: &str,
        kid: &str,
        jti: Uuid,
        expires_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO oidc_token_audit (job_run_id, agent_id, audience, kid, jti, expires_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(job_run_id)
        .bind(agent_id)
        .bind(audience)
        .bind(kid)
        .bind(jti)
        .bind(expires_at)
        .execute(self.pool)
        .await?;
        Ok(())
    }
}

/// Bootstrap a workload-identity signing key when none exists (ADR-017). Idempotent; uses a
/// PostgreSQL advisory lock so concurrent `met-api` / `met-controller` startups do not race.
pub async fn ensure_initial_oidc_signing_key(
    pool: &PgPool,
    crypto: &BuiltinStoredCrypto,
) -> Result<()> {
    const LOCK_K1: i32 = 884_291;
    const LOCK_K2: i32 = 291_884;

    let got: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1, $2)")
        .bind(LOCK_K1)
        .bind(LOCK_K2)
        .fetch_one(pool)
        .await?;

    if !got {
        return Ok(());
    }

    let inner = async {
        let repo = OidcSigningKeyRepo::new(pool);
        if repo.active_key().await?.is_some() {
            return Ok(());
        }

        let generated = met_secrets::generate_oidc_signing_key(crypto, chrono::Duration::days(90))
            .map_err(|e| StoreError::Validation(format!("OIDC signing key generation: {e}")))?;

        repo.insert(
            &generated.kid,
            &generated.private_key_enc,
            &generated.public_key_jwk,
            generated.expires_at,
        )
        .await?;

        tracing::info!(kid = %generated.kid, "bootstrapped OIDC workload signing key");
        Ok(())
    }
    .await;

    let _ = sqlx::query("SELECT pg_advisory_unlock($1, $2)")
        .bind(LOCK_K1)
        .bind(LOCK_K2)
        .execute(pool)
        .await;

    inner
}
