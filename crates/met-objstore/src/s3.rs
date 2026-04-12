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
    Client,
    config::{Credentials, Region},
    error::SdkError,
    operation::create_bucket::CreateBucketError,
    operation::get_object::GetObjectError,
    operation::RequestId,
    primitives::ByteStream,
    types::{BucketLocationConstraint, CreateBucketConfiguration},
};
use aws_smithy_types::error::metadata::ProvideErrorMetadata;
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
        let bucket = config.bucket.clone();
        if config.auto_create_bucket {
            ensure_bucket_exists(&client, &bucket, &config).await?;
        }
        Ok(Self {
            client,
            bucket,
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

    /// Verify the bucket exists and credentials allow `s3:ListBucket` / head access.
    pub async fn head_bucket(&self) -> Result<()> {
        self.client
            .head_bucket()
            .bucket(&self.bucket)
            .send()
            .await
            .map_err(|e| map_sdk_error_detailed("head_bucket", e))?;
        Ok(())
    }

    /// Health-style reachability check for S3-compatible backends.
    ///
    /// Tries [`Self::head_bucket`] first, then a minimal `ListObjectsV2` (one key).
    /// Some gateways (including certain SeaweedFS S3 configurations) return generic
    /// "service" errors for `HeadBucket` while list still works.
    pub async fn check_bucket_reachable(&self) -> Result<()> {
        if self.head_bucket().await.is_ok() {
            return Ok(());
        }

        self.client
            .list_objects_v2()
            .bucket(&self.bucket)
            .max_keys(1)
            .send()
            .await
            .map_err(|e| map_sdk_error_detailed("list_objects_v2", e))?;
        Ok(())
    }
}

/// Credential strategy for the AWS SDK: static signing keys vs full AWS default chain (IAM/SSO/profile).
enum ResolvedSigningCredentials {
    Static(Credentials),
    AwsDefaultChain,
}

fn static_pair_from_config(config: &ObjectStoreConfig) -> Option<(String, String)> {
    let ak = config.access_key_id.as_ref()?.trim();
    let sk = config.secret_access_key.as_ref()?.trim();
    if ak.is_empty() || sk.is_empty() {
        return None;
    }
    Some((ak.to_string(), sk.to_string()))
}

fn env_key_pair(access_var: &str, secret_var: &str) -> Option<(String, String)> {
    let ak = std::env::var(access_var).ok()?.trim().to_string();
    let sk = std::env::var(secret_var).ok()?.trim().to_string();
    if ak.is_empty() || sk.is_empty() {
        return None;
    }
    Some((ak, sk))
}

/// True when the endpoint URL host is a typical local S3-compatible dev target (SeaweedFS, MinIO).
fn endpoint_host_is_local_dev(endpoint: &str) -> bool {
    let Ok(url) = Url::parse(endpoint) else {
        return endpoint.contains("127.0.0.1")
            || endpoint.contains("localhost")
            || endpoint.contains("[::1]");
    };
    matches!(
        url.host_str(),
        Some("localhost" | "127.0.0.1" | "::1")
    )
}

fn credentials_from_access_secret(ak: String, sk: String) -> Credentials {
    let token = std::env::var("AWS_SESSION_TOKEN")
        .ok()
        .and_then(|t| {
            let t = t.trim().to_string();
            if t.is_empty() {
                None
            } else {
                Some(t)
            }
        });
    Credentials::new(ak, sk, token, None, "meticulous-static")
}

/// Resolve credentials without touching the AWS profile/SSO chain when a custom `endpoint` is set.
fn resolve_signing_credentials(config: &ObjectStoreConfig) -> Result<ResolvedSigningCredentials> {
    let custom_endpoint = !config.endpoint.is_empty();

    if let Some((ak, sk)) = static_pair_from_config(config) {
        return Ok(ResolvedSigningCredentials::Static(credentials_from_access_secret(
            ak, sk,
        )));
    }

    if custom_endpoint {
        if let Some((ak, sk)) = env_key_pair("AWS_ACCESS_KEY_ID", "AWS_SECRET_ACCESS_KEY") {
            return Ok(ResolvedSigningCredentials::Static(credentials_from_access_secret(
                ak, sk,
            )));
        }
        if let Some((ak, sk)) =
            env_key_pair("MET_STORAGE__ACCESS_KEY", "MET_STORAGE__SECRET_KEY")
        {
            return Ok(ResolvedSigningCredentials::Static(credentials_from_access_secret(
                ak, sk,
            )));
        }
        if endpoint_host_is_local_dev(&config.endpoint) {
            tracing::debug!(
                endpoint = %config.endpoint,
                "S3-compatible local endpoint: using admin/admin signing credentials (override with MET_STORAGE__ACCESS_KEY / MET_STORAGE__SECRET_KEY or AWS_ACCESS_KEY_ID / AWS_SECRET_ACCESS_KEY)"
            );
            return Ok(ResolvedSigningCredentials::Static(Credentials::new(
                "admin",
                "admin",
                None,
                None,
                "meticulous-local-s3-placeholder",
            )));
        }
        return Err(ObjectStoreError::config(
            "storage access_key and secret_key are required for this S3 endpoint \
(set storage.access_key/secret_key, MET_STORAGE__ACCESS_KEY / MET_STORAGE__SECRET_KEY, or AWS_ACCESS_KEY_ID / AWS_SECRET_ACCESS_KEY)",
        ));
    }

    Ok(ResolvedSigningCredentials::AwsDefaultChain)
}

async fn create_s3_client(config: &ObjectStoreConfig) -> Result<Client> {
    let mut loader = aws_config::defaults(BehaviorVersion::latest())
        .region(Region::new(config.region.clone()));

    if !config.endpoint.is_empty() {
        loader = loader.endpoint_url(&config.endpoint);
    }

    let sdk_config = match resolve_signing_credentials(config)? {
        ResolvedSigningCredentials::Static(creds) => {
            loader.credentials_provider(creds).load().await
        }
        ResolvedSigningCredentials::AwsDefaultChain => loader.load().await,
    };

    let mut s3_config_builder =
        aws_sdk_s3::config::Builder::from(&sdk_config).force_path_style(config.path_style);

    if !config.endpoint.is_empty() {
        s3_config_builder = s3_config_builder.endpoint_url(&config.endpoint);
    }

    Ok(Client::from_conf(s3_config_builder.build()))
}

/// Ensure the bucket exists, using the same credentials as [`create_s3_client`].
///
/// Tries [`HeadBucket`] first; if that fails (missing bucket or S3-compatible quirks), attempts
/// [`CreateBucket`] and treats "already exists" responses as success.
async fn ensure_bucket_exists(
    client: &Client,
    bucket: &str,
    config: &ObjectStoreConfig,
) -> Result<()> {
    if client
        .head_bucket()
        .bucket(bucket)
        .send()
        .await
        .is_ok()
    {
        return Ok(());
    }

    let mut req = client.create_bucket().bucket(bucket);

    // Real AWS requires `LocationConstraint` outside `us-east-1`. Custom endpoints (MinIO, SeaweedFS)
    // typically reject or ignore extra XML; omit the constraint for those.
    if config.endpoint.is_empty() {
        let region = config.region.as_str();
        if region != "us-east-1" {
            let loc = BucketLocationConstraint::from(region);
            let cbc = CreateBucketConfiguration::builder()
                .location_constraint(loc)
                .build();
            req = req.create_bucket_configuration(cbc);
        }
    }

    match req.send().await {
        Ok(_) => {
            tracing::info!(%bucket, "created object storage bucket");
            Ok(())
        }
        Err(e) => {
            let err = e.into_service_error();
            if is_create_bucket_duplicate(&err) {
                Ok(())
            } else {
                Err(map_sdk_error_inner("create_bucket", err))
            }
        }
    }
}

fn is_create_bucket_duplicate(err: &CreateBucketError) -> bool {
    if err.is_bucket_already_exists() || err.is_bucket_already_owned_by_you() {
        return true;
    }
    matches!(
        err.meta().code(),
        Some("BucketAlreadyExists" | "BucketAlreadyOwnedByYou")
    )
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
                chrono::DateTime::from_timestamp(t.secs(), t.subsec_nanos()).unwrap_or_default()
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
                chrono::DateTime::from_timestamp(t.secs(), t.subsec_nanos()).unwrap_or_default()
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
        let result = self
            .list_objects_with_options(prefix, ListOptions::default())
            .await?;
        Ok(result.objects)
    }

    async fn list_objects_with_options(
        &self,
        prefix: &str,
        options: ListOptions,
    ) -> Result<ListResult> {
        let mut request = self
            .client
            .list_objects_v2()
            .bucket(&self.bucket)
            .prefix(prefix);

        if let Some(max_keys) = options.max_keys {
            request = request.max_keys(max_keys);
        }
        if let Some(token) = options.continuation_token {
            request = request.continuation_token(token);
        }
        if let Some(delimiter) = options.delimiter {
            request = request.delimiter(delimiter);
        }

        let output = request
            .send()
            .await
            .map_err(|e| map_sdk_error("list_objects", e))?;

        let objects: Vec<_> = output
            .contents()
            .iter()
            .map(|obj| ObjectMeta {
                key: obj.key().unwrap_or_default().to_string(),
                size: obj.size().unwrap_or(0) as u64,
                etag: obj.e_tag().map(|s| s.trim_matches('"').to_string()),
                last_modified: obj.last_modified().map(|t| {
                    chrono::DateTime::from_timestamp(t.secs(), t.subsec_nanos()).unwrap_or_default()
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

        Ok(MultipartUpload::new(
            key.clone(),
            upload_id,
            self.bucket.clone(),
        ))
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

/// Map AWS SDK errors with HTTP metadata (many S3-compatible servers return useful codes only here).
fn map_sdk_error_detailed<E>(operation: &str, err: SdkError<E>) -> ObjectStoreError
where
    E: std::fmt::Display + ProvideErrorMetadata,
{
    let detail = match &err {
        SdkError::ServiceError(se) => {
            let meta = se.err().meta();
            let code = meta.code().unwrap_or("Unknown");
            let msg = meta.message().unwrap_or("");
            let mut out = if msg.is_empty() {
                code.to_string()
            } else {
                format!("{code}: {msg}")
            };
            if let Some(rid) = meta.request_id() {
                if !rid.is_empty() {
                    out.push_str(&format!(" (request_id={rid})"));
                }
            }
            out
        }
        _ => err.to_string(),
    };
    ObjectStoreError::s3(format!("{operation}: {detail}"))
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
        assert!(obj_config.auto_create_bucket);
    }
}
