//! Presigned URL generation utilities.

use crate::paths::ObjectKey;
use std::time::Duration;
use url::Url;

/// Options for generating presigned URLs.
#[derive(Debug, Clone)]
pub struct PresignedOptions {
    /// URL expiration duration.
    pub expires_in: Duration,
    /// Content type for PUT requests.
    pub content_type: Option<String>,
    /// Content disposition header value.
    pub content_disposition: Option<String>,
    /// Response content type override for GET requests.
    pub response_content_type: Option<String>,
}

impl Default for PresignedOptions {
    fn default() -> Self {
        Self {
            expires_in: Duration::from_secs(3600), // 1 hour
            content_type: None,
            content_disposition: None,
            response_content_type: None,
        }
    }
}

impl PresignedOptions {
    /// Create options with a specific expiration.
    pub fn with_expires_in(expires_in: Duration) -> Self {
        Self { expires_in, ..Default::default() }
    }

    /// Set the content type for uploads.
    pub fn content_type(mut self, content_type: impl Into<String>) -> Self {
        self.content_type = Some(content_type.into());
        self
    }

    /// Set the content disposition (e.g., "attachment; filename=file.txt").
    pub fn content_disposition(mut self, disposition: impl Into<String>) -> Self {
        self.content_disposition = Some(disposition.into());
        self
    }

    /// Set response content type override for downloads.
    pub fn response_content_type(mut self, content_type: impl Into<String>) -> Self {
        self.response_content_type = Some(content_type.into());
        self
    }
}

/// Builder for creating presigned URLs with a fluent API.
pub struct PresignedUrlBuilder {
    key: ObjectKey,
    options: PresignedOptions,
}

impl PresignedUrlBuilder {
    /// Create a new builder for the given key.
    pub fn new(key: ObjectKey) -> Self {
        Self { key, options: PresignedOptions::default() }
    }

    /// Set the expiration duration.
    pub fn expires_in(mut self, duration: Duration) -> Self {
        self.options.expires_in = duration;
        self
    }

    /// Set the content type.
    pub fn content_type(mut self, content_type: impl Into<String>) -> Self {
        self.options.content_type = Some(content_type.into());
        self
    }

    /// Set the content disposition for downloads.
    pub fn attachment(mut self, filename: impl AsRef<str>) -> Self {
        self.options.content_disposition =
            Some(format!("attachment; filename=\"{}\"", filename.as_ref()));
        self
    }

    /// Set response content type override.
    pub fn response_content_type(mut self, content_type: impl Into<String>) -> Self {
        self.options.response_content_type = Some(content_type.into());
        self
    }

    /// Get the key.
    pub fn key(&self) -> &ObjectKey {
        &self.key
    }

    /// Get the options.
    pub fn options(&self) -> &PresignedOptions {
        &self.options
    }

    /// Consume the builder and return the key and options.
    pub fn build(self) -> (ObjectKey, PresignedOptions) {
        (self.key, self.options)
    }
}

/// Generate a content disposition header for attachment downloads.
pub fn attachment_disposition(filename: &str) -> String {
    let safe_filename = filename.replace('"', "\\\"");
    format!("attachment; filename=\"{safe_filename}\"")
}

/// Generate a content disposition header for inline display.
pub fn inline_disposition(filename: &str) -> String {
    let safe_filename = filename.replace('"', "\\\"");
    format!("inline; filename=\"{safe_filename}\"")
}

/// Parse a presigned URL to extract its components.
#[derive(Debug, Clone)]
pub struct ParsedPresignedUrl {
    /// The base URL without query parameters.
    pub base_url: Url,
    /// The object key extracted from the path.
    pub key: String,
    /// The bucket name.
    pub bucket: Option<String>,
    /// Whether this is a path-style URL.
    pub path_style: bool,
}

impl ParsedPresignedUrl {
    /// Parse a presigned URL.
    pub fn parse(url: &Url) -> Option<Self> {
        let path = url.path();
        let host = url.host_str()?;

        // Detect path-style vs virtual-hosted style
        let (bucket, key, path_style) = if host.contains('.') && !host.starts_with("s3.") {
            // Virtual-hosted style: bucket.s3.region.amazonaws.com/key
            let bucket = host.split('.').next()?.to_string();
            let key = path.trim_start_matches('/').to_string();
            (Some(bucket), key, false)
        } else {
            // Path-style: endpoint/bucket/key
            let parts: Vec<_> = path.trim_start_matches('/').splitn(2, '/').collect();
            if parts.len() == 2 {
                (Some(parts[0].to_string()), parts[1].to_string(), true)
            } else {
                (None, path.trim_start_matches('/').to_string(), true)
            }
        };

        let mut base_url = url.clone();
        base_url.set_query(None);

        Some(Self { base_url, key, bucket, path_style })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_presigned_options_default() {
        let opts = PresignedOptions::default();
        assert_eq!(opts.expires_in, Duration::from_secs(3600));
        assert!(opts.content_type.is_none());
    }

    #[test]
    fn test_presigned_options_builder() {
        let opts = PresignedOptions::with_expires_in(Duration::from_secs(7200))
            .content_type("application/json")
            .content_disposition("attachment; filename=test.json");

        assert_eq!(opts.expires_in, Duration::from_secs(7200));
        assert_eq!(opts.content_type, Some("application/json".to_string()));
    }

    #[test]
    fn test_presigned_url_builder() {
        let (key, opts) = PresignedUrlBuilder::new(ObjectKey::new("test/file.txt"))
            .expires_in(Duration::from_secs(300))
            .attachment("file.txt")
            .build();

        assert_eq!(key.as_str(), "test/file.txt");
        assert_eq!(opts.expires_in, Duration::from_secs(300));
        assert!(opts.content_disposition.unwrap().contains("attachment"));
    }

    #[test]
    fn test_attachment_disposition() {
        let disp = attachment_disposition("test file.txt");
        assert_eq!(disp, "attachment; filename=\"test file.txt\"");
    }

    #[test]
    fn test_parse_presigned_url_path_style() {
        let url = Url::parse("http://localhost:8333/bucket/path/to/key?sig=abc").unwrap();
        let parsed = ParsedPresignedUrl::parse(&url).unwrap();

        assert_eq!(parsed.bucket, Some("bucket".to_string()));
        assert_eq!(parsed.key, "path/to/key");
        assert!(parsed.path_style);
    }
}
