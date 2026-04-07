//! Object storage configuration.

use serde::{Deserialize, Serialize};

/// Configuration for object storage.
///
/// This can be used alongside or instead of `met_core::config::StorageConfig`
/// for more detailed object store configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ObjectStoreConfig {
    /// S3-compatible endpoint URL.
    pub endpoint: String,
    /// Default bucket name.
    pub bucket: String,
    /// Access key ID (optional, can use IAM/IRSA).
    pub access_key_id: Option<String>,
    /// Secret access key (optional, can use IAM/IRSA).
    pub secret_access_key: Option<String>,
    /// AWS region.
    pub region: String,
    /// Use path-style URLs (required for MinIO, SeaweedFS, etc.).
    pub path_style: bool,
    /// Connection timeout in seconds.
    pub connect_timeout_secs: u64,
    /// Read timeout in seconds.
    pub read_timeout_secs: u64,
    /// Maximum number of retry attempts.
    pub max_retries: u32,
    /// Default presigned URL expiration in seconds.
    pub presigned_url_expiry_secs: u64,
    /// Multipart upload part size in bytes (minimum 5MB).
    pub multipart_part_size: usize,
    /// Threshold for using multipart upload in bytes.
    pub multipart_threshold: usize,
}

impl Default for ObjectStoreConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://localhost:8333".to_string(),
            bucket: "meticulous".to_string(),
            access_key_id: None,
            secret_access_key: None,
            region: "us-east-1".to_string(),
            path_style: true,
            connect_timeout_secs: 5,
            read_timeout_secs: 30,
            max_retries: 3,
            presigned_url_expiry_secs: 3600,        // 1 hour
            multipart_part_size: 5 * 1024 * 1024,   // 5 MB (S3 minimum)
            multipart_threshold: 100 * 1024 * 1024, // 100 MB
        }
    }
}

impl ObjectStoreConfig {
    /// Create a config for local development with SeaweedFS.
    pub fn local_dev() -> Self {
        Self {
            endpoint: "http://localhost:8333".to_string(),
            bucket: "meticulous-dev".to_string(),
            access_key_id: Some("admin".to_string()),
            secret_access_key: Some("admin".to_string()),
            region: "us-east-1".to_string(),
            path_style: true,
            ..Default::default()
        }
    }

    /// Create a config for AWS S3.
    pub fn aws_s3(bucket: impl Into<String>, region: impl Into<String>) -> Self {
        Self {
            endpoint: String::new(), // Use default AWS endpoint
            bucket: bucket.into(),
            region: region.into(),
            path_style: false,
            access_key_id: None, // Use IAM role
            secret_access_key: None,
            ..Default::default()
        }
    }

    /// Check if static credentials are configured.
    pub fn has_static_credentials(&self) -> bool {
        self.access_key_id.is_some() && self.secret_access_key.is_some()
    }
}

impl From<met_core::config::StorageConfig> for ObjectStoreConfig {
    fn from(config: met_core::config::StorageConfig) -> Self {
        Self {
            endpoint: config.endpoint,
            bucket: config.bucket,
            access_key_id: config.access_key,
            secret_access_key: config.secret_key,
            region: config.region.unwrap_or_else(|| "us-east-1".to_string()),
            path_style: config.path_style,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = ObjectStoreConfig::default();
        assert_eq!(config.bucket, "meticulous");
        assert!(config.path_style);
    }

    #[test]
    fn test_local_dev_config() {
        let config = ObjectStoreConfig::local_dev();
        assert!(config.has_static_credentials());
        assert_eq!(config.bucket, "meticulous-dev");
    }

    #[test]
    fn test_aws_s3_config() {
        let config = ObjectStoreConfig::aws_s3("my-bucket", "us-west-2");
        assert!(!config.has_static_credentials());
        assert!(!config.path_style);
        assert_eq!(config.region, "us-west-2");
    }
}
