//! Artifact management for pipeline execution.
//!
//! Handles uploading, downloading, and passing artifacts between jobs.

use async_trait::async_trait;
use met_core::ids::{ArtifactId, JobId, JobRunId, RunId};
use serde::{Deserialize, Serialize};
use tracing::{debug, instrument};

use crate::error::{EngineError, Result};

/// Artifact metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactMetadata {
    pub id: ArtifactId,
    pub run_id: RunId,
    pub job_run_id: JobRunId,
    pub job_id: JobId,
    pub name: String,
    pub path_pattern: String,
    pub content_type: Option<String>,
    pub size_bytes: u64,
    pub sha256: Option<String>,
    pub storage_path: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Artifact upload request.
#[derive(Debug, Clone)]
pub struct UploadArtifactRequest {
    pub run_id: RunId,
    pub job_run_id: JobRunId,
    pub job_id: JobId,
    pub name: String,
    pub path_pattern: String,
    pub content_type: Option<String>,
    pub retention_days: Option<u32>,
}

/// Artifact download request.
#[derive(Debug, Clone)]
pub struct DownloadArtifactRequest {
    pub artifact_id: ArtifactId,
    pub destination_path: String,
}

/// Artifact backend trait.
#[async_trait]
pub trait ArtifactBackend: Send + Sync {
    /// Upload an artifact.
    async fn upload(
        &self,
        request: &UploadArtifactRequest,
        data: &[u8],
    ) -> Result<ArtifactMetadata>;

    /// Download an artifact.
    async fn download(&self, artifact_id: ArtifactId) -> Result<Vec<u8>>;

    /// Get artifact metadata.
    async fn get_metadata(&self, artifact_id: ArtifactId) -> Result<ArtifactMetadata>;

    /// List artifacts for a job run.
    async fn list_by_job_run(&self, job_run_id: JobRunId) -> Result<Vec<ArtifactMetadata>>;

    /// List artifacts for a run.
    async fn list_by_run(&self, run_id: RunId) -> Result<Vec<ArtifactMetadata>>;

    /// Delete an artifact.
    async fn delete(&self, artifact_id: ArtifactId) -> Result<()>;

    /// Generate a presigned download URL.
    async fn presigned_download_url(
        &self,
        artifact_id: ArtifactId,
        expires_in: std::time::Duration,
    ) -> Result<String>;
}

/// Artifact manager for pipeline execution.
pub struct ArtifactManager<B: ArtifactBackend> {
    backend: B,
}

impl<B: ArtifactBackend> ArtifactManager<B> {
    pub fn new(backend: B) -> Self {
        Self { backend }
    }

    /// Upload an artifact from job execution.
    #[instrument(skip(self, data))]
    pub async fn upload(
        &self,
        request: &UploadArtifactRequest,
        data: &[u8],
    ) -> Result<ArtifactMetadata> {
        debug!(
            name = %request.name,
            pattern = %request.path_pattern,
            size = data.len(),
            "uploading artifact"
        );

        self.backend.upload(request, data).await
    }

    /// Download an artifact for use in another job.
    #[instrument(skip(self))]
    pub async fn download(&self, artifact_id: ArtifactId) -> Result<Vec<u8>> {
        debug!(%artifact_id, "downloading artifact");
        self.backend.download(artifact_id).await
    }

    /// Get artifact metadata.
    pub async fn get_metadata(&self, artifact_id: ArtifactId) -> Result<ArtifactMetadata> {
        self.backend.get_metadata(artifact_id).await
    }

    /// List all artifacts from a job run.
    pub async fn list_job_artifacts(&self, job_run_id: JobRunId) -> Result<Vec<ArtifactMetadata>> {
        self.backend.list_by_job_run(job_run_id).await
    }

    /// List all artifacts from a run.
    pub async fn list_run_artifacts(&self, run_id: RunId) -> Result<Vec<ArtifactMetadata>> {
        self.backend.list_by_run(run_id).await
    }

    /// Get a presigned URL for downloading an artifact.
    pub async fn get_download_url(
        &self,
        artifact_id: ArtifactId,
        expires_in: std::time::Duration,
    ) -> Result<String> {
        self.backend
            .presigned_download_url(artifact_id, expires_in)
            .await
    }
}

/// In-memory artifact store for testing.
pub struct MemoryArtifactStore {
    artifacts:
        std::sync::RwLock<std::collections::HashMap<ArtifactId, (ArtifactMetadata, Vec<u8>)>>,
}

impl MemoryArtifactStore {
    pub fn new() -> Self {
        Self {
            artifacts: std::sync::RwLock::new(std::collections::HashMap::new()),
        }
    }
}

impl Default for MemoryArtifactStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ArtifactBackend for MemoryArtifactStore {
    async fn upload(
        &self,
        request: &UploadArtifactRequest,
        data: &[u8],
    ) -> Result<ArtifactMetadata> {
        let id = ArtifactId::new();
        let now = chrono::Utc::now();

        let expires_at = request
            .retention_days
            .map(|days| now + chrono::Duration::days(i64::from(days)));

        let metadata = ArtifactMetadata {
            id,
            run_id: request.run_id,
            job_run_id: request.job_run_id,
            job_id: request.job_id,
            name: request.name.clone(),
            path_pattern: request.path_pattern.clone(),
            content_type: request.content_type.clone(),
            size_bytes: data.len() as u64,
            sha256: Some(compute_sha256(data)),
            storage_path: format!("memory://{}", id),
            created_at: now,
            expires_at,
        };

        self.artifacts
            .write()
            .map_err(|e| EngineError::internal(e.to_string()))?
            .insert(id, (metadata.clone(), data.to_vec()));

        Ok(metadata)
    }

    async fn download(&self, artifact_id: ArtifactId) -> Result<Vec<u8>> {
        let artifacts = self
            .artifacts
            .read()
            .map_err(|e| EngineError::internal(e.to_string()))?;

        artifacts
            .get(&artifact_id)
            .map(|(_, data)| data.clone())
            .ok_or_else(|| EngineError::Artifact(format!("Artifact not found: {artifact_id}")))
    }

    async fn get_metadata(&self, artifact_id: ArtifactId) -> Result<ArtifactMetadata> {
        let artifacts = self
            .artifacts
            .read()
            .map_err(|e| EngineError::internal(e.to_string()))?;

        artifacts
            .get(&artifact_id)
            .map(|(meta, _)| meta.clone())
            .ok_or_else(|| EngineError::Artifact(format!("Artifact not found: {artifact_id}")))
    }

    async fn list_by_job_run(&self, job_run_id: JobRunId) -> Result<Vec<ArtifactMetadata>> {
        let artifacts = self
            .artifacts
            .read()
            .map_err(|e| EngineError::internal(e.to_string()))?;

        Ok(artifacts
            .values()
            .filter(|(meta, _)| meta.job_run_id == job_run_id)
            .map(|(meta, _)| meta.clone())
            .collect())
    }

    async fn list_by_run(&self, run_id: RunId) -> Result<Vec<ArtifactMetadata>> {
        let artifacts = self
            .artifacts
            .read()
            .map_err(|e| EngineError::internal(e.to_string()))?;

        Ok(artifacts
            .values()
            .filter(|(meta, _)| meta.run_id == run_id)
            .map(|(meta, _)| meta.clone())
            .collect())
    }

    async fn delete(&self, artifact_id: ArtifactId) -> Result<()> {
        self.artifacts
            .write()
            .map_err(|e| EngineError::internal(e.to_string()))?
            .remove(&artifact_id);
        Ok(())
    }

    async fn presigned_download_url(
        &self,
        artifact_id: ArtifactId,
        _expires_in: std::time::Duration,
    ) -> Result<String> {
        Ok(format!("memory://{}", artifact_id))
    }
}

fn compute_sha256(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    result.iter().map(|b| format!("{:02x}", b)).collect()
}
