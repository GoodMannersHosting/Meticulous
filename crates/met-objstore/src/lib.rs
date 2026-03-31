//! S3-compatible object storage abstraction for Meticulous CI/CD.
//!
//! This crate provides a unified interface for storing and retrieving
//! artifacts, logs, and other binary data from S3-compatible storage.
//!
//! # Features
//!
//! - **Trait-based abstraction**: `ObjectStore` trait allows easy mocking and alternative implementations
//! - **S3 compatibility**: Works with AWS S3, MinIO, SeaweedFS, and other S3-compatible stores
//! - **Multipart uploads**: Support for large file uploads with automatic chunking
//! - **Presigned URLs**: Generate time-limited URLs for direct client uploads/downloads
//! - **Path conventions**: Standardized key patterns for artifacts, logs, and SBOMs
//!
//! # Usage
//!
//! ```ignore
//! use met_objstore::{ObjectStoreConfig, S3ObjectStore, ObjectStore, ObjectKey};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = ObjectStoreConfig::default();
//!     let store = S3ObjectStore::new(config).await?;
//!
//!     // Upload an artifact
//!     let key = ObjectKey::new("runs/123/artifacts/test-results.xml");
//!     store.put_object(&key, bytes::Bytes::from("test data")).await?;
//!
//!     // Generate presigned URL for download
//!     let url = store.presigned_get(&key, std::time::Duration::from_secs(3600)).await?;
//!     println!("Download URL: {}", url);
//!
//!     Ok(())
//! }
//! ```

pub mod config;
pub mod error;
pub mod multipart;
pub mod paths;
pub mod presigned;
pub mod s3;
pub mod traits;

pub use config::ObjectStoreConfig;
pub use error::{ObjectStoreError, Result};
pub use multipart::{CompletedPart, MultipartOptions, MultipartUpload, MultipartUploader};
pub use paths::{keys, ObjectKey, ObjectKeyBuilder, SbomFormat};
pub use presigned::{
    attachment_disposition, inline_disposition, ParsedPresignedUrl, PresignedOptions,
    PresignedUrlBuilder,
};
pub use s3::S3ObjectStore;
pub use traits::{GetResult, ListOptions, ListResult, ObjectMeta, ObjectStore, ObjectStoreExt, PutResult};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_public_api() {
        let key = ObjectKey::new("test/path");
        assert_eq!(key.as_str(), "test/path");

        let config = ObjectStoreConfig::default();
        assert_eq!(config.bucket, "meticulous");
    }

    #[test]
    fn test_key_builder() {
        let key = ObjectKeyBuilder::new()
            .organization("acme")
            .project("api")
            .artifact("run-123", "coverage.xml");

        assert!(key.as_str().contains("acme"));
        assert!(key.as_str().contains("api"));
        assert!(key.as_str().contains("run-123"));
    }
}
