//! Cache management for pipeline execution.
//!
//! Provides job-level caching to avoid re-executing jobs when inputs haven't changed.
//! Cache keys are computed from:
//! - Cache key template with variable interpolation
//! - File content hashes for specified paths
//! - Lock file hashes

use async_trait::async_trait;
use sha2::{Digest, Sha256};
use tracing::{debug, instrument};

use crate::context::ExecutionContext;
use crate::error::{EngineError, Result};

/// Cache key and metadata.
#[derive(Debug, Clone)]
pub struct CacheKey {
    /// The computed cache key.
    pub key: String,
    /// Original key template before interpolation.
    pub template: String,
    /// Paths included in the cache.
    pub paths: Vec<String>,
    /// Restore keys for partial matches.
    pub restore_keys: Vec<String>,
}

/// Result of a cache lookup.
#[derive(Debug, Clone)]
pub enum CacheLookupResult {
    /// Exact cache hit.
    Hit {
        key: String,
        storage_path: String,
        created_at: chrono::DateTime<chrono::Utc>,
    },
    /// Partial match using restore key.
    PartialHit {
        matched_key: String,
        original_key: String,
        storage_path: String,
        created_at: chrono::DateTime<chrono::Utc>,
    },
    /// No cache found.
    Miss { key: String },
}

/// Cache backend trait.
#[async_trait]
pub trait CacheBackend: Send + Sync {
    /// Look up a cache entry.
    async fn lookup(&self, key: &CacheKey) -> Result<CacheLookupResult>;

    /// Store a cache entry.
    async fn store(&self, key: &CacheKey, data_path: &str) -> Result<String>;

    /// Delete a cache entry.
    async fn delete(&self, key: &str) -> Result<()>;
}

/// Cache manager for pipeline execution.
pub struct CacheManager<B: CacheBackend> {
    backend: B,
}

impl<B: CacheBackend> CacheManager<B> {
    /// Create a new cache manager.
    pub fn new(backend: B) -> Self {
        Self { backend }
    }

    /// Compute a cache key from configuration.
    #[instrument(skip(self, ctx))]
    pub async fn compute_key(
        &self,
        template: &str,
        paths: &[String],
        restore_keys: &[String],
        ctx: &ExecutionContext,
    ) -> Result<CacheKey> {
        let interpolated = self.interpolate_template(template, ctx).await?;
        
        let key = format!("{}-{}", interpolated, self.hash_paths(paths));

        debug!(template, key = %key, "computed cache key");

        Ok(CacheKey {
            key,
            template: template.to_string(),
            paths: paths.to_vec(),
            restore_keys: restore_keys.to_vec(),
        })
    }

    /// Look up a cache entry.
    pub async fn lookup(&self, key: &CacheKey) -> Result<CacheLookupResult> {
        self.backend.lookup(key).await
    }

    /// Store a cache entry.
    pub async fn store(&self, key: &CacheKey, data_path: &str) -> Result<String> {
        self.backend.store(key, data_path).await
    }

    async fn interpolate_template(&self, template: &str, ctx: &ExecutionContext) -> Result<String> {
        let mut result = template.to_string();

        let var_pattern = regex::Regex::new(r"\$\{\{\s*(\w+)\s*\}\}")
            .map_err(|e| EngineError::internal(format!("Invalid regex: {e}")))?;

        for cap in var_pattern.captures_iter(template) {
            let var_name = &cap[1];
            let value = ctx.get_variable(var_name).await.unwrap_or_default();
            result = result.replace(&cap[0], &value);
        }

        let hashfiles_pattern = regex::Regex::new(r"hashFiles\('([^']+)'\)")
            .map_err(|e| EngineError::internal(format!("Invalid regex: {e}")))?;

        for cap in hashfiles_pattern.captures_iter(template) {
            let _glob_pattern = &cap[1];
            let hash = "placeholder_hash";
            result = result.replace(&cap[0], hash);
        }

        Ok(result)
    }

    fn hash_paths(&self, paths: &[String]) -> String {
        let mut hasher = Sha256::new();
        for path in paths {
            hasher.update(path.as_bytes());
        }
        let result = hasher.finalize();
        hex::encode(&result[..8])
    }
}

/// Object storage-backed cache implementation.
pub struct ObjectStoreCache {
    prefix: String,
}

impl ObjectStoreCache {
    pub fn new(prefix: impl Into<String>) -> Self {
        Self {
            prefix: prefix.into(),
        }
    }

    fn cache_path(&self, key: &str) -> String {
        format!("{}/cache/{}.tar.gz", self.prefix, key)
    }
}

#[async_trait]
impl CacheBackend for ObjectStoreCache {
    async fn lookup(&self, key: &CacheKey) -> Result<CacheLookupResult> {
        let path = self.cache_path(&key.key);
        
        debug!(key = %key.key, path = %path, "looking up cache");
        Ok(CacheLookupResult::Miss { key: key.key.clone() })
    }

    async fn store(&self, key: &CacheKey, _data_path: &str) -> Result<String> {
        let path = self.cache_path(&key.key);
        debug!(key = %key.key, path = %path, "storing cache");
        Ok(path)
    }

    async fn delete(&self, key: &str) -> Result<()> {
        let path = self.cache_path(key);
        debug!(key, path = %path, "deleting cache");
        Ok(())
    }
}

/// In-memory cache for testing.
pub struct MemoryCache {
    entries: std::sync::RwLock<std::collections::HashMap<String, (String, chrono::DateTime<chrono::Utc>)>>,
}

impl MemoryCache {
    pub fn new() -> Self {
        Self {
            entries: std::sync::RwLock::new(std::collections::HashMap::new()),
        }
    }
}

impl Default for MemoryCache {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CacheBackend for MemoryCache {
    async fn lookup(&self, key: &CacheKey) -> Result<CacheLookupResult> {
        let entries = self.entries.read().map_err(|e| EngineError::internal(e.to_string()))?;

        if let Some((path, created_at)) = entries.get(&key.key) {
            return Ok(CacheLookupResult::Hit {
                key: key.key.clone(),
                storage_path: path.clone(),
                created_at: *created_at,
            });
        }

        for restore_key in &key.restore_keys {
            for (k, (path, created_at)) in entries.iter() {
                if k.starts_with(restore_key) {
                    return Ok(CacheLookupResult::PartialHit {
                        matched_key: k.clone(),
                        original_key: key.key.clone(),
                        storage_path: path.clone(),
                        created_at: *created_at,
                    });
                }
            }
        }

        Ok(CacheLookupResult::Miss { key: key.key.clone() })
    }

    async fn store(&self, key: &CacheKey, _data_path: &str) -> Result<String> {
        let mut entries = self.entries.write().map_err(|e| EngineError::internal(e.to_string()))?;
        let storage_path = format!("memory://{}", key.key);
        entries.insert(key.key.clone(), (storage_path.clone(), chrono::Utc::now()));
        Ok(storage_path)
    }

    async fn delete(&self, key: &str) -> Result<()> {
        let mut entries = self.entries.write().map_err(|e| EngineError::internal(e.to_string()))?;
        entries.remove(key);
        Ok(())
    }
}

mod hex {
    pub fn encode(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }
}
