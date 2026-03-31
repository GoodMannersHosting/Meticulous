//! Build artifact models.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{ArtifactId, JobRunId, RunId};

/// A build artifact produced by a job.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct Artifact {
    /// Unique identifier.
    pub id: ArtifactId,
    /// Parent run.
    pub run_id: RunId,
    /// Job run that produced this artifact.
    pub job_run_id: JobRunId,
    /// Artifact name.
    pub name: String,
    /// MIME content type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    /// Size in bytes.
    pub size_bytes: i64,
    /// Path in object storage.
    pub storage_path: String,
    /// SHA-256 hash of contents.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    /// When the artifact was created.
    pub created_at: DateTime<Utc>,
    /// When the artifact expires (for auto-cleanup).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
}

impl Artifact {
    /// Create a new artifact record.
    #[must_use]
    pub fn new(
        run_id: RunId,
        job_run_id: JobRunId,
        name: impl Into<String>,
        storage_path: impl Into<String>,
        size_bytes: i64,
    ) -> Self {
        Self {
            id: ArtifactId::new(),
            run_id,
            job_run_id,
            name: name.into(),
            content_type: None,
            size_bytes,
            storage_path: storage_path.into(),
            sha256: None,
            created_at: Utc::now(),
            expires_at: None,
        }
    }
}

/// Input for uploading an artifact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadArtifact {
    /// Artifact name.
    pub name: String,
    /// MIME content type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    /// Retention period in days (None for default).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retention_days: Option<i32>,
}
