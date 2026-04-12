//! Platform policy for which external (provider-reference) stored secret kinds are allowed.

use std::collections::HashMap;

use met_store::repos::{PlatformSettingsRepo, StoredSecretKind};
use sqlx::PgPool;

/// DB key under `platform_settings`.
pub const STORED_SECRET_EXTERNAL_KINDS_KEY: &str = "stored_secret_external_kinds";

/// External provider kinds controlled by platform settings (must match `StoredSecretKind::as_str()`).
pub const EXTERNAL_STORED_SECRET_KINDS: &[&str] =
    &["aws_sm", "vault", "gcp_sm", "azure_kv", "kubernetes"];

#[must_use]
pub fn default_external_kind_policy() -> HashMap<String, bool> {
    EXTERNAL_STORED_SECRET_KINDS
        .iter()
        .map(|k| ((*k).to_string(), true))
        .collect()
}

/// Merge stored JSON with defaults (all enabled when unset).
pub async fn load_merged_external_kind_policy(
    pool: &PgPool,
) -> Result<HashMap<String, bool>, met_store::StoreError> {
    let mut map = default_external_kind_policy();
    let repo = PlatformSettingsRepo::new(pool);
    if let Some(row) = repo.get(STORED_SECRET_EXTERNAL_KINDS_KEY).await? {
        if let Some(obj) = row.value.as_object() {
            for (k, v) in obj {
                if let Some(b) = v.as_bool() {
                    map.insert(k.clone(), b);
                }
            }
        }
    }
    Ok(map)
}

#[must_use]
pub fn is_external_kind_allowed(policy: &HashMap<String, bool>, kind: StoredSecretKind) -> bool {
    if !kind.stores_remote_ref_in_metadata() {
        return true;
    }
    policy.get(kind.as_str()).copied().unwrap_or(true)
}
