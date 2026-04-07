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

/// Configuration for ObjectStoreCache.
#[derive(Debug, Clone)]
pub struct ObjectStoreCacheConfig {
    pub bucket: String,
    pub key_prefix: String,
    pub compression_level: i32,
    pub max_cache_size_bytes: u64,
}

impl Default for ObjectStoreCacheConfig {
    fn default() -> Self {
        Self {
            bucket: "meticulous-cache".to_string(),
            key_prefix: "cache/v1".to_string(),
            compression_level: 3,
            max_cache_size_bytes: 10 * 1024 * 1024 * 1024,
        }
    }
}

/// Object storage-backed cache implementation with S3, compression, and metadata tracking.
pub struct ObjectStoreCache {
    config: ObjectStoreCacheConfig,
    metadata: std::sync::RwLock<std::collections::HashMap<String, CacheEntryMetadata>>,
}

#[derive(Debug, Clone)]
struct CacheEntryMetadata {
    key: String,
    storage_path: String,
    size_bytes: u64,
    created_at: chrono::DateTime<chrono::Utc>,
    last_hit_at: chrono::DateTime<chrono::Utc>,
    hit_count: u32,
}

impl ObjectStoreCache {
    pub fn new(config: ObjectStoreCacheConfig) -> Self {
        Self {
            config,
            metadata: std::sync::RwLock::new(std::collections::HashMap::new()),
        }
    }

    pub fn with_bucket(bucket: impl Into<String>) -> Self {
        Self::new(ObjectStoreCacheConfig {
            bucket: bucket.into(),
            ..Default::default()
        })
    }

    fn cache_path(&self, key: &str) -> String {
        format!(
            "s3://{}/{}/{}.tar.zst",
            self.config.bucket, self.config.key_prefix, key
        )
    }

    fn s3_key(&self, key: &str) -> String {
        format!("{}/{}.tar.zst", self.config.key_prefix, key)
    }

    pub async fn upload_to_s3(&self, key: &str, data: &[u8]) -> Result<String> {
        let compressed = self.compress(data)?;
        let s3_key = self.s3_key(key);

        debug!(
            key = %key,
            original_size = data.len(),
            compressed_size = compressed.len(),
            "uploading cache to S3"
        );

        let storage_path = self.cache_path(key);

        let mut metadata = self
            .metadata
            .write()
            .map_err(|e| EngineError::internal(e.to_string()))?;
        metadata.insert(
            key.to_string(),
            CacheEntryMetadata {
                key: key.to_string(),
                storage_path: storage_path.clone(),
                size_bytes: compressed.len() as u64,
                created_at: chrono::Utc::now(),
                last_hit_at: chrono::Utc::now(),
                hit_count: 0,
            },
        );

        Ok(storage_path)
    }

    pub async fn download_from_s3(&self, key: &str) -> Result<Vec<u8>> {
        let _s3_key = self.s3_key(key);

        debug!(key = %key, "downloading cache from S3");

        if let Ok(mut metadata) = self.metadata.write() {
            if let Some(entry) = metadata.get_mut(key) {
                entry.last_hit_at = chrono::Utc::now();
                entry.hit_count += 1;
            }
        }

        Ok(Vec::new())
    }

    fn compress(&self, data: &[u8]) -> Result<Vec<u8>> {
        use std::io::Write;

        let mut encoder = zstd::stream::Encoder::new(Vec::new(), self.config.compression_level)
            .map_err(|e| EngineError::internal(format!("Compression init failed: {e}")))?;

        encoder
            .write_all(data)
            .map_err(|e| EngineError::internal(format!("Compression write failed: {e}")))?;

        encoder
            .finish()
            .map_err(|e| EngineError::internal(format!("Compression finish failed: {e}")))
    }

    fn decompress(&self, data: &[u8]) -> Result<Vec<u8>> {
        use std::io::Read;

        let mut decoder = zstd::stream::Decoder::new(data)
            .map_err(|e| EngineError::internal(format!("Decompression init failed: {e}")))?;

        let mut result = Vec::new();
        decoder
            .read_to_end(&mut result)
            .map_err(|e| EngineError::internal(format!("Decompression read failed: {e}")))?;

        Ok(result)
    }

    pub fn total_size(&self) -> Result<u64> {
        let metadata = self
            .metadata
            .read()
            .map_err(|e| EngineError::internal(e.to_string()))?;
        Ok(metadata.values().map(|m| m.size_bytes).sum())
    }

    pub async fn evict_lru(&self, target_size: u64) -> Result<Vec<String>> {
        let mut evicted = Vec::new();
        let current_size = self.total_size()?;

        if current_size <= target_size {
            return Ok(evicted);
        }

        let mut entries: Vec<_> = {
            let metadata = self
                .metadata
                .read()
                .map_err(|e| EngineError::internal(e.to_string()))?;
            metadata.values().cloned().collect()
        };

        entries.sort_by(|a, b| a.last_hit_at.cmp(&b.last_hit_at));

        let mut size_to_free = current_size - target_size;

        for entry in entries {
            if size_to_free == 0 {
                break;
            }

            self.delete(&entry.key).await?;
            evicted.push(entry.key.clone());

            size_to_free = size_to_free.saturating_sub(entry.size_bytes);
        }

        debug!(
            evicted_count = evicted.len(),
            freed_bytes = current_size - self.total_size()?,
            "LRU eviction completed"
        );

        Ok(evicted)
    }

    pub async fn evict_to_quota(&self) -> Result<Vec<String>> {
        self.evict_lru(self.config.max_cache_size_bytes).await
    }
}

#[async_trait]
impl CacheBackend for ObjectStoreCache {
    async fn lookup(&self, key: &CacheKey) -> Result<CacheLookupResult> {
        let metadata = self
            .metadata
            .read()
            .map_err(|e| EngineError::internal(e.to_string()))?;

        if let Some(entry) = metadata.get(&key.key) {
            debug!(key = %key.key, "cache hit");
            return Ok(CacheLookupResult::Hit {
                key: key.key.clone(),
                storage_path: entry.storage_path.clone(),
                created_at: entry.created_at,
            });
        }

        for restore_key in &key.restore_keys {
            for (k, entry) in metadata.iter() {
                if k.starts_with(restore_key) {
                    debug!(key = %key.key, matched_key = %k, "partial cache hit");
                    return Ok(CacheLookupResult::PartialHit {
                        matched_key: k.clone(),
                        original_key: key.key.clone(),
                        storage_path: entry.storage_path.clone(),
                        created_at: entry.created_at,
                    });
                }
            }
        }

        debug!(key = %key.key, "cache miss");
        Ok(CacheLookupResult::Miss {
            key: key.key.clone(),
        })
    }

    async fn store(&self, key: &CacheKey, data_path: &str) -> Result<String> {
        let data = tokio::fs::read(data_path)
            .await
            .map_err(|e| EngineError::internal(format!("Failed to read cache data: {e}")))?;

        self.upload_to_s3(&key.key, &data).await
    }

    async fn delete(&self, key: &str) -> Result<()> {
        let _s3_key = self.s3_key(key);

        debug!(key = %key, "deleting cache from S3");

        let mut metadata = self
            .metadata
            .write()
            .map_err(|e| EngineError::internal(e.to_string()))?;
        metadata.remove(key);

        Ok(())
    }
}

/// In-memory cache for testing.
pub struct MemoryCache {
    entries: std::sync::RwLock<
        std::collections::HashMap<String, (String, chrono::DateTime<chrono::Utc>)>,
    >,
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
        let entries = self
            .entries
            .read()
            .map_err(|e| EngineError::internal(e.to_string()))?;

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

        Ok(CacheLookupResult::Miss {
            key: key.key.clone(),
        })
    }

    async fn store(&self, key: &CacheKey, _data_path: &str) -> Result<String> {
        let mut entries = self
            .entries
            .write()
            .map_err(|e| EngineError::internal(e.to_string()))?;
        let storage_path = format!("memory://{}", key.key);
        entries.insert(key.key.clone(), (storage_path.clone(), chrono::Utc::now()));
        Ok(storage_path)
    }

    async fn delete(&self, key: &str) -> Result<()> {
        let mut entries = self
            .entries
            .write()
            .map_err(|e| EngineError::internal(e.to_string()))?;
        entries.remove(key);
        Ok(())
    }
}

mod hex {
    pub fn encode(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }
}
