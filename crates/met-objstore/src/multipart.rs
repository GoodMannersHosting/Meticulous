//! Multipart upload support for large objects.

use crate::{
    error::{ObjectStoreError, Result},
    paths::ObjectKey,
    traits::PutResult,
};
use bytes::Bytes;

/// Represents an in-progress multipart upload.
#[derive(Debug, Clone)]
pub struct MultipartUpload {
    /// The object key being uploaded.
    pub key: ObjectKey,
    /// The upload ID assigned by the storage service.
    pub upload_id: String,
    /// Bucket name.
    pub bucket: String,
    /// Parts that have been uploaded.
    pub parts: Vec<CompletedPart>,
}

impl MultipartUpload {
    /// Create a new multipart upload tracker.
    pub fn new(key: ObjectKey, upload_id: String, bucket: String) -> Self {
        Self {
            key,
            upload_id,
            bucket,
            parts: Vec::new(),
        }
    }

    /// Record a completed part.
    pub fn add_part(&mut self, part: CompletedPart) {
        self.parts.push(part);
    }

    /// Get the number of parts uploaded.
    pub fn part_count(&self) -> usize {
        self.parts.len()
    }

    /// Sort parts by part number for completion.
    pub fn sorted_parts(&self) -> Vec<CompletedPart> {
        let mut parts = self.parts.clone();
        parts.sort_by_key(|p| p.part_number);
        parts
    }
}

/// A completed part of a multipart upload.
#[derive(Debug, Clone)]
pub struct CompletedPart {
    /// Part number (1-indexed).
    pub part_number: i32,
    /// ETag returned by the upload part operation.
    pub etag: String,
}

impl CompletedPart {
    pub fn new(part_number: i32, etag: String) -> Self {
        Self { part_number, etag }
    }
}

/// Options for multipart upload.
#[derive(Debug, Clone)]
pub struct MultipartOptions {
    /// Size of each part in bytes.
    pub part_size: usize,
    /// Maximum concurrent part uploads.
    pub max_concurrent_uploads: usize,
}

impl Default for MultipartOptions {
    fn default() -> Self {
        Self {
            part_size: 5 * 1024 * 1024, // 5 MB minimum for S3
            max_concurrent_uploads: 4,
        }
    }
}

/// Helper for managing multipart uploads with automatic chunking.
pub struct MultipartUploader<S> {
    store: S,
    options: MultipartOptions,
}

impl<S> MultipartUploader<S> {
    /// Create a new multipart uploader.
    pub fn new(store: S, options: MultipartOptions) -> Self {
        Self { store, options }
    }

    /// Get the configured part size.
    pub fn part_size(&self) -> usize {
        self.options.part_size
    }
}

impl<S: crate::traits::ObjectStore> MultipartUploader<S> {
    /// Upload data using multipart upload.
    ///
    /// This automatically chunks the data and uploads parts in sequence.
    pub async fn upload(&self, key: &ObjectKey, data: Bytes) -> Result<PutResult> {
        if data.len() < self.options.part_size {
            return self.store.put_object(key, data).await;
        }

        let mut upload = self.store.initiate_multipart(key).await?;
        let chunks: Vec<_> = data.chunks(self.options.part_size).collect();

        for (i, chunk) in chunks.iter().enumerate() {
            let part_number = (i + 1) as i32;
            let part_data = Bytes::copy_from_slice(chunk);

            let etag = upload_part(&self.store, &upload, part_number, part_data).await?;
            upload.add_part(CompletedPart::new(part_number, etag));
        }

        complete_multipart(&self.store, upload).await
    }
}

/// Upload a single part of a multipart upload.
async fn upload_part<S: crate::traits::ObjectStore + ?Sized>(
    _store: &S,
    upload: &MultipartUpload,
    part_number: i32,
    _data: Bytes,
) -> Result<String> {
    // In a real implementation, this would call the S3 UploadPart API.
    // For now, we return a mock ETag.
    tracing::debug!(
        upload_id = %upload.upload_id,
        part_number,
        "Uploading part"
    );

    Ok(format!("etag-{}-{}", upload.upload_id, part_number))
}

/// Complete a multipart upload.
async fn complete_multipart<S: crate::traits::ObjectStore + ?Sized>(
    _store: &S,
    upload: MultipartUpload,
) -> Result<PutResult> {
    if upload.parts.is_empty() {
        return Err(ObjectStoreError::multipart("No parts uploaded"));
    }

    // In a real implementation, this would call the S3 CompleteMultipartUpload API.
    tracing::debug!(
        upload_id = %upload.upload_id,
        part_count = upload.part_count(),
        "Completing multipart upload"
    );

    Ok(PutResult {
        etag: Some(format!("multipart-etag-{}", upload.upload_id)),
        version_id: None,
    })
}

/// Abort a multipart upload.
pub async fn abort_multipart<S: crate::traits::ObjectStore + ?Sized>(
    _store: &S,
    upload: &MultipartUpload,
) -> Result<()> {
    // In a real implementation, this would call the S3 AbortMultipartUpload API.
    tracing::debug!(
        upload_id = %upload.upload_id,
        "Aborting multipart upload"
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_multipart_upload_new() {
        let upload = MultipartUpload::new(
            ObjectKey::new("test/key"),
            "upload-123".to_string(),
            "bucket".to_string(),
        );
        assert_eq!(upload.upload_id, "upload-123");
        assert_eq!(upload.part_count(), 0);
    }

    #[test]
    fn test_multipart_upload_add_parts() {
        let mut upload = MultipartUpload::new(
            ObjectKey::new("test/key"),
            "upload-123".to_string(),
            "bucket".to_string(),
        );

        upload.add_part(CompletedPart::new(2, "etag-2".to_string()));
        upload.add_part(CompletedPart::new(1, "etag-1".to_string()));
        upload.add_part(CompletedPart::new(3, "etag-3".to_string()));

        assert_eq!(upload.part_count(), 3);

        let sorted = upload.sorted_parts();
        assert_eq!(sorted[0].part_number, 1);
        assert_eq!(sorted[1].part_number, 2);
        assert_eq!(sorted[2].part_number, 3);
    }

    #[test]
    fn test_multipart_options_default() {
        let opts = MultipartOptions::default();
        assert_eq!(opts.part_size, 5 * 1024 * 1024);
        assert_eq!(opts.max_concurrent_uploads, 4);
    }
}
