//! Core trait definitions for object storage.

use crate::{
    error::Result,
    multipart::MultipartUpload,
    paths::ObjectKey,
};
use async_trait::async_trait;
use bytes::Bytes;
use chrono::{DateTime, Utc};
use std::time::Duration;
use url::Url;

/// Result of a put operation.
#[derive(Debug, Clone)]
pub struct PutResult {
    /// ETag of the uploaded object.
    pub etag: Option<String>,
    /// Version ID if versioning is enabled.
    pub version_id: Option<String>,
}

/// Result of a get operation with the object body.
#[derive(Debug)]
pub struct GetResult {
    /// Object content as bytes.
    pub body: Bytes,
    /// Object metadata.
    pub meta: ObjectMeta,
}

/// Metadata about an object.
#[derive(Debug, Clone)]
pub struct ObjectMeta {
    /// Object key.
    pub key: String,
    /// Content length in bytes.
    pub size: u64,
    /// ETag (usually MD5 hash).
    pub etag: Option<String>,
    /// Last modified timestamp.
    pub last_modified: Option<DateTime<Utc>>,
    /// Content type (MIME type).
    pub content_type: Option<String>,
    /// Version ID if versioning is enabled.
    pub version_id: Option<String>,
    /// User-defined metadata.
    pub metadata: std::collections::HashMap<String, String>,
}

impl ObjectMeta {
    /// Create a new `ObjectMeta` with required fields.
    pub fn new(key: impl Into<String>, size: u64) -> Self {
        Self {
            key: key.into(),
            size,
            etag: None,
            last_modified: None,
            content_type: None,
            version_id: None,
            metadata: std::collections::HashMap::new(),
        }
    }
}

/// Options for list operations.
#[derive(Debug, Clone, Default)]
pub struct ListOptions {
    /// Maximum number of keys to return.
    pub max_keys: Option<i32>,
    /// Continuation token for pagination.
    pub continuation_token: Option<String>,
    /// Delimiter for grouping keys (usually "/").
    pub delimiter: Option<String>,
}

/// Result of a list operation.
#[derive(Debug, Clone)]
pub struct ListResult {
    /// Objects matching the prefix.
    pub objects: Vec<ObjectMeta>,
    /// Common prefixes (directories) when using delimiter.
    pub common_prefixes: Vec<String>,
    /// Token for fetching the next page.
    pub next_continuation_token: Option<String>,
    /// Whether there are more results.
    pub is_truncated: bool,
}

/// Trait defining the object storage interface.
///
/// All operations are async and return `Result` types for proper error handling.
#[async_trait]
pub trait ObjectStore: Send + Sync {
    /// Upload an object to the store.
    async fn put_object(&self, key: &ObjectKey, body: Bytes) -> Result<PutResult>;

    /// Upload an object with content type specified.
    async fn put_object_with_content_type(
        &self,
        key: &ObjectKey,
        body: Bytes,
        content_type: &str,
    ) -> Result<PutResult>;

    /// Download an object from the store.
    async fn get_object(&self, key: &ObjectKey) -> Result<GetResult>;

    /// Get object metadata without downloading the body.
    async fn head_object(&self, key: &ObjectKey) -> Result<ObjectMeta>;

    /// Delete an object from the store.
    async fn delete_object(&self, key: &ObjectKey) -> Result<()>;

    /// Delete multiple objects.
    async fn delete_objects(&self, keys: &[ObjectKey]) -> Result<Vec<String>>;

    /// List objects with the given prefix.
    async fn list_objects(&self, prefix: &str) -> Result<Vec<ObjectMeta>>;

    /// List objects with pagination options.
    async fn list_objects_with_options(
        &self,
        prefix: &str,
        options: ListOptions,
    ) -> Result<ListResult>;

    /// Check if an object exists.
    async fn exists(&self, key: &ObjectKey) -> Result<bool>;

    /// Copy an object within the store.
    async fn copy_object(&self, source: &ObjectKey, destination: &ObjectKey) -> Result<PutResult>;

    /// Generate a presigned URL for downloading an object.
    async fn presigned_get(&self, key: &ObjectKey, expires_in: Duration) -> Result<Url>;

    /// Generate a presigned URL for uploading an object.
    async fn presigned_put(&self, key: &ObjectKey, expires_in: Duration) -> Result<Url>;

    /// Initiate a multipart upload.
    async fn initiate_multipart(&self, key: &ObjectKey) -> Result<MultipartUpload>;

    /// Get the bucket name.
    fn bucket(&self) -> &str;
}

/// Extension trait for convenient streaming operations.
#[async_trait]
pub trait ObjectStoreExt: ObjectStore {
    /// Upload a string as an object.
    async fn put_string(&self, key: &ObjectKey, content: &str) -> Result<PutResult> {
        self.put_object_with_content_type(key, Bytes::from(content.to_owned()), "text/plain")
            .await
    }

    /// Upload JSON as an object.
    async fn put_json<T: serde::Serialize + Send + Sync>(
        &self,
        key: &ObjectKey,
        value: &T,
    ) -> Result<PutResult> {
        let json = serde_json::to_vec(value)
            .map_err(|e| crate::error::ObjectStoreError::internal(e.to_string()))?;
        self.put_object_with_content_type(key, Bytes::from(json), "application/json")
            .await
    }

    /// Download an object as a string.
    async fn get_string(&self, key: &ObjectKey) -> Result<String> {
        let result = self.get_object(key).await?;
        String::from_utf8(result.body.to_vec())
            .map_err(|e| crate::error::ObjectStoreError::internal(e.to_string()))
    }

    /// Download and parse a JSON object.
    async fn get_json<T: serde::de::DeserializeOwned>(&self, key: &ObjectKey) -> Result<T> {
        let result = self.get_object(key).await?;
        serde_json::from_slice(&result.body)
            .map_err(|e| crate::error::ObjectStoreError::internal(e.to_string()))
    }
}

impl<T: ObjectStore + ?Sized> ObjectStoreExt for T {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_object_meta_new() {
        let meta = ObjectMeta::new("test/key", 1024);
        assert_eq!(meta.key, "test/key");
        assert_eq!(meta.size, 1024);
        assert!(meta.etag.is_none());
    }

    #[test]
    fn test_list_options_default() {
        let opts = ListOptions::default();
        assert!(opts.max_keys.is_none());
        assert!(opts.continuation_token.is_none());
    }
}
