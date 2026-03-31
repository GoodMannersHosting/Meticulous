//! S3-compatible object storage implementation.

use crate::{
    config::ObjectStoreConfig,
    error::{ObjectStoreError, Result},
    multipart::{CompletedPart, MultipartUpload},
    paths::ObjectKey,
    traits::{GetResult, ListOptions, ListResult, ObjectMeta, ObjectStore, PutResult},
};
use async_trait::async_trait;
use aws_config::BehaviorVersion;
use aws_sdk_s3::{
    config::{Credentials, Region},
    operation::get_object::GetObjectError,
    primitives::ByteStream,
    Client,
};
use bytes::Bytes;
use std::time::Duration;
use url::Url;

/// S3-compatible object store implementation.
pub struct S3ObjectStore {
    client: Client,
    bucket: String,
    config: ObjectStoreConfig,
}

impl S3ObjectStore {
    /// Create a new S3 object store from configuration.
    pub async fn new(config: ObjectStoreConfig) -> Result<Self> {
        let client = create_s3_client(&config).await?;
        Ok(Self {
            client,
            bucket: config.bucket.clone(),
            config,
        })
    }

    /// Create a new S3 object store with a custom client.
    pub fn with_client(client: Client, config: ObjectStoreConfig) -> Self {
        Self {
            client,
            bucket: config.bucket.clone(),
            config,
        }
    }

    /// Get a reference to the underlying S3 client.
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// Get the configuration.
    pub fn config(&self) -> &ObjectStoreConfig {
        &self.config
    }
}

async fn create_s3_client(config: &ObjectStoreConfig) -> Result<Client> {
    let mut sdk_config_builder = aws_config::defaults(BehaviorVersion::latest());

    if !config.endpoint.is_empty() {
        sdk_config_builder = sdk_config_builder.endpoint_url(&config.endpoint);
    }

    sdk_config_builder = sdk_config_builder.region(Region::new(config.region.clone()));

    if let (Some(access_key), Some(secret_key)) =
        (&config.access_key_id, &config.secret_access_key)
    {
        let credentials =
            Credentials::new(access_key, secret_key, None, None, "meticulous-static");
        sdk_config_builder = sdk_config_builder.credentials_provider(credentials);
    }

    let sdk_config = sdk_config_builder.load().await;

    let mut s3_config_builder = aws_sdk_s3::config::Builder::from(&sdk_config)
        .force_path_style(config.path_style);

    if !config.endpoint.is_empty() {
        s3_config_builder = s3_config_builder.endpoint_url(&config.endpoint);
    }

    Ok(Client::from_conf(s3_config_builder.build()))
}

#[async_trait]
impl ObjectStore for S3ObjectStore {
    async fn put_object(&self, key: &ObjectKey, body: Bytes) -> Result<PutResult> {
        self.put_object_with_content_type(key, body, "application/octet-stream")
            .await
    }

    async fn put_object_with_content_type(
        &self,
        key: &ObjectKey,
        body: Bytes,
        content_type: &str,
    ) -> Result<PutResult> {
        let result = self
            .client
            .put_object()
            .bucket(&self.bucket)
            .key(key.as_str())
            .body(ByteStream::from(body))
            .content_type(content_type)
            .send()
            .await
            .map_err(|e| map_sdk_error("put_object", e))?;

        Ok(PutResult {
            etag: result.e_tag().map(|s| s.trim_matches('"').to_string()),
            version_id: result.version_id().map(String::from),
        })
    }

    async fn get_object(&self, key: &ObjectKey) -> Result<GetResult> {
        let output = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key.as_str())
            .send()
            .await
            .map_err(|e| {
                let service_err = e.into_service_error();
                if matches!(service_err, GetObjectError::NoSuchKey(_)) {
                    ObjectStoreError::not_found(key.as_str())
                } else {
                    map_sdk_error_inner("get_object", service_err)
                }
            })?;

        let meta = ObjectMeta {
            key: key.as_str().to_string(),
            size: output.content_length().unwrap_or(0) as u64,
            etag: output.e_tag().map(|s| s.trim_matches('"').to_string()),
            last_modified: output.last_modified().map(|t| {
                chrono::DateTime::from_timestamp(t.secs(), t.subsec_nanos())
                    .unwrap_or_default()
            }),
            content_type: output.content_type().map(String::from),
            version_id: output.version_id().map(String::from),
            metadata: output
                .metadata()
                .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                .unwrap_or_default(),
        };

        let body = output
            .body
            .collect()
            .await
            .map_err(|e| ObjectStoreError::internal(e.to_string()))?
            .into_bytes();

        Ok(GetResult { body, meta })
    }

    async fn head_object(&self, key: &ObjectKey) -> Result<ObjectMeta> {
        let output = self
            .client
            .head_object()
            .bucket(&self.bucket)
            .key(key.as_str())
            .send()
            .await
            .map_err(|e| {
                let service_err = e.into_service_error();
                if service_err.is_not_found() {
                    ObjectStoreError::not_found(key.as_str())
                } else {
                    map_sdk_error_inner("head_object", service_err)
                }
            })?;

        Ok(ObjectMeta {
            key: key.as_str().to_string(),
            size: output.content_length().unwrap_or(0) as u64,
            etag: output.e_tag().map(|s| s.trim_matches('"').to_string()),
            last_modified: output.last_modified().map(|t| {
                chrono::DateTime::from_timestamp(t.secs(), t.subsec_nanos())
                    .unwrap_or_default()
            }),
            content_type: output.content_type().map(String::from),
            version_id: output.version_id().map(String::from),
            metadata: output
                .metadata()
                .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                .unwrap_or_default(),
        })
    }

    async fn delete_object(&self, key: &ObjectKey) -> Result<()> {
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(key.as_str())
            .send()
            .await
            .map_err(|e| map_sdk_error("delete_object", e))?;

        Ok(())
    }

    async fn delete_objects(&self, keys: &[ObjectKey]) -> Result<Vec<String>> {
        if keys.is_empty() {
            return Ok(Vec::new());
        }

        let objects: Vec<_> = keys
            .iter()
            .map(|k| {
                aws_sdk_s3::types::ObjectIdentifier::builder()
                    .key(k.as_str())
                    .build()
                    .expect("key is required")
            })
            .collect();

        let delete = aws_sdk_s3::types::Delete::builder()
            .set_objects(Some(objects))
            .build()
            .map_err(|e| ObjectStoreError::internal(e.to_string()))?;

        let output = self
            .client
            .delete_objects()
            .bucket(&self.bucket)
            .delete(delete)
            .send()
            .await
            .map_err(|e| map_sdk_error("delete_objects", e))?;

        let deleted: Vec<_> = output
            .deleted()
            .iter()
            .filter_map(|d| d.key().map(String::from))
            .collect();

        Ok(deleted)
    }

    async fn list_objects(&self, prefix: &str) -> Result<Vec<ObjectMeta>> {
        let result = self.list_objects_with_options(prefix, ListOptions::default()).await?;
        Ok(result.objects)
    }

    async fn list_objects_with_options(
        &self,
        prefix: &str,
        options: ListOptions,
    ) -> Result<ListResult> {
        let mut request = self.client.list_objects_v2().bucket(&self.bucket).prefix(prefix);

        if let Some(max_keys) = options.max_keys {
            request = request.max_keys(max_keys);
        }
        if let Some(token) = options.continuation_token {
            request = request.continuation_token(token);
        }
        if let Some(delimiter) = options.delimiter {
            request = request.delimiter(delimiter);
        }

        let output = request.send().await.map_err(|e| map_sdk_error("list_objects", e))?;

        let objects: Vec<_> = output
            .contents()
            .iter()
            .map(|obj| ObjectMeta {
                key: obj.key().unwrap_or_default().to_string(),
                size: obj.size().unwrap_or(0) as u64,
                etag: obj.e_tag().map(|s| s.trim_matches('"').to_string()),
                last_modified: obj.last_modified().map(|t| {
                    chrono::DateTime::from_timestamp(t.secs(), t.subsec_nanos())
                        .unwrap_or_default()
                }),
                content_type: None,
                version_id: None,
                metadata: std::collections::HashMap::new(),
            })
            .collect();

        let common_prefixes: Vec<_> = output
            .common_prefixes()
            .iter()
            .filter_map(|p| p.prefix().map(String::from))
            .collect();

        Ok(ListResult {
            objects,
            common_prefixes,
            next_continuation_token: output.next_continuation_token().map(String::from),
            is_truncated: output.is_truncated().unwrap_or(false),
        })
    }

    async fn exists(&self, key: &ObjectKey) -> Result<bool> {
        match self.head_object(key).await {
            Ok(_) => Ok(true),
            Err(ObjectStoreError::NotFound { .. }) => Ok(false),
            Err(e) => Err(e),
        }
    }

    async fn copy_object(&self, source: &ObjectKey, destination: &ObjectKey) -> Result<PutResult> {
        let copy_source = format!("{}/{}", self.bucket, source.as_str());

        let result = self
            .client
            .copy_object()
            .bucket(&self.bucket)
            .copy_source(copy_source)
            .key(destination.as_str())
            .send()
            .await
            .map_err(|e| map_sdk_error("copy_object", e))?;

        Ok(PutResult {
            etag: result
                .copy_object_result()
                .and_then(|r| r.e_tag())
                .map(|s| s.trim_matches('"').to_string()),
            version_id: result.version_id().map(String::from),
        })
    }

    async fn presigned_get(&self, key: &ObjectKey, expires_in: Duration) -> Result<Url> {
        let presigning_config = aws_sdk_s3::presigning::PresigningConfig::expires_in(expires_in)
            .map_err(|e| ObjectStoreError::presigned(e.to_string()))?;

        let presigned = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key.as_str())
            .presigned(presigning_config)
            .await
            .map_err(|e| ObjectStoreError::presigned(e.to_string()))?;

        Url::parse(presigned.uri()).map_err(|e| ObjectStoreError::presigned(e.to_string()))
    }

    async fn presigned_put(&self, key: &ObjectKey, expires_in: Duration) -> Result<Url> {
        let presigning_config = aws_sdk_s3::presigning::PresigningConfig::expires_in(expires_in)
            .map_err(|e| ObjectStoreError::presigned(e.to_string()))?;

        let presigned = self
            .client
            .put_object()
            .bucket(&self.bucket)
            .key(key.as_str())
            .presigned(presigning_config)
            .await
            .map_err(|e| ObjectStoreError::presigned(e.to_string()))?;

        Url::parse(presigned.uri()).map_err(|e| ObjectStoreError::presigned(e.to_string()))
    }

    async fn initiate_multipart(&self, key: &ObjectKey) -> Result<MultipartUpload> {
        let output = self
            .client
            .create_multipart_upload()
            .bucket(&self.bucket)
            .key(key.as_str())
            .send()
            .await
            .map_err(|e| map_sdk_error("create_multipart_upload", e))?;

        let upload_id = output
            .upload_id()
            .ok_or_else(|| ObjectStoreError::multipart("No upload ID returned"))?
            .to_string();

        Ok(MultipartUpload::new(key.clone(), upload_id, self.bucket.clone()))
    }

    fn bucket(&self) -> &str {
        &self.bucket
    }
}

impl S3ObjectStore {
    /// Upload a part of a multipart upload.
    pub async fn upload_part(
        &self,
        upload: &MultipartUpload,
        part_number: i32,
        body: Bytes,
    ) -> Result<CompletedPart> {
        let output = self
            .client
            .upload_part()
            .bucket(&self.bucket)
            .key(upload.key.as_str())
            .upload_id(&upload.upload_id)
            .part_number(part_number)
            .body(ByteStream::from(body))
            .send()
            .await
            .map_err(|e| map_sdk_error("upload_part", e))?;

        let etag = output
            .e_tag()
            .ok_or_else(|| ObjectStoreError::multipart("No ETag returned for part"))?
            .trim_matches('"')
            .to_string();

        Ok(CompletedPart::new(part_number, etag))
    }

    /// Complete a multipart upload.
    pub async fn complete_multipart(&self, upload: MultipartUpload) -> Result<PutResult> {
        let parts: Vec<_> = upload
            .sorted_parts()
            .into_iter()
            .map(|p| {
                aws_sdk_s3::types::CompletedPart::builder()
                    .part_number(p.part_number)
                    .e_tag(p.etag)
                    .build()
            })
            .collect();

        let completed_upload = aws_sdk_s3::types::CompletedMultipartUpload::builder()
            .set_parts(Some(parts))
            .build();

        let output = self
            .client
            .complete_multipart_upload()
            .bucket(&self.bucket)
            .key(upload.key.as_str())
            .upload_id(&upload.upload_id)
            .multipart_upload(completed_upload)
            .send()
            .await
            .map_err(|e| map_sdk_error("complete_multipart_upload", e))?;

        Ok(PutResult {
            etag: output.e_tag().map(|s| s.trim_matches('"').to_string()),
            version_id: output.version_id().map(String::from),
        })
    }

    /// Abort a multipart upload.
    pub async fn abort_multipart(&self, upload: &MultipartUpload) -> Result<()> {
        self.client
            .abort_multipart_upload()
            .bucket(&self.bucket)
            .key(upload.key.as_str())
            .upload_id(&upload.upload_id)
            .send()
            .await
            .map_err(|e| map_sdk_error("abort_multipart_upload", e))?;

        Ok(())
    }
}

fn map_sdk_error<E: std::fmt::Display>(operation: &str, err: E) -> ObjectStoreError {
    ObjectStoreError::s3(format!("{operation}: {err}"))
}

fn map_sdk_error_inner<E: std::fmt::Display>(operation: &str, err: E) -> ObjectStoreError {
    ObjectStoreError::s3(format!("{operation}: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_conversion() {
        let core_config = met_core::config::StorageConfig::default();
        let obj_config: ObjectStoreConfig = core_config.into();

        assert!(obj_config.path_style);
        assert_eq!(obj_config.bucket, "meticulous");
    }
}
