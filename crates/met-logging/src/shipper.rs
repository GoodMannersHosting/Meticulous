//! Log shipping and archival to object storage.
//!
//! Handles persisting logs to durable storage with compression.

use crate::aggregator::LogLine;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use met_core::ids::{JobId, RunId, StepId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use thiserror::Error;
use tracing::{debug, instrument};

/// Errors from log shipping operations.
#[derive(Debug, Error)]
pub enum ShipperError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Archive not found: {0}")]
    NotFound(String),
}

pub type Result<T> = std::result::Result<T, ShipperError>;

/// Configuration for the log shipper.
#[derive(Debug, Clone)]
pub struct ShipperConfig {
    /// Base path for local storage (if using file backend).
    pub storage_path: PathBuf,
    /// Whether to compress archived logs.
    pub compress: bool,
    /// Retention period in days.
    pub retention_days: u32,
}

impl Default for ShipperConfig {
    fn default() -> Self {
        Self {
            storage_path: PathBuf::from("/var/lib/meticulous/logs"),
            compress: true,
            retention_days: 90,
        }
    }
}

/// Metadata for an archived log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogArchive {
    /// Run ID for this archive.
    pub run_id: RunId,
    /// Job ID (optional for run-level archives).
    pub job_id: Option<JobId>,
    /// Step ID (optional for job/run-level archives).
    pub step_id: Option<StepId>,
    /// When the archive was created.
    pub archived_at: DateTime<Utc>,
    /// Total line count.
    pub line_count: u64,
    /// Compressed size in bytes.
    pub size_bytes: u64,
    /// Storage path/key.
    pub storage_key: String,
    /// Whether the archive is compressed.
    pub compressed: bool,
}

/// Trait for log storage backends.
#[async_trait]
pub trait LogStorageBackend: Send + Sync {
    /// Store log data and return the storage key.
    async fn store(&self, key: &str, data: &[u8]) -> Result<()>;

    /// Retrieve log data by key.
    async fn retrieve(&self, key: &str) -> Result<Vec<u8>>;

    /// Delete log data by key.
    async fn delete(&self, key: &str) -> Result<()>;

    /// Check if a key exists.
    async fn exists(&self, key: &str) -> Result<bool>;
}

/// Local filesystem storage backend.
pub struct FileStorageBackend {
    base_path: PathBuf,
}

impl FileStorageBackend {
    pub fn new(base_path: PathBuf) -> Self {
        Self { base_path }
    }

    fn key_to_path(&self, key: &str) -> PathBuf {
        self.base_path.join(key)
    }
}

#[async_trait]
impl LogStorageBackend for FileStorageBackend {
    async fn store(&self, key: &str, data: &[u8]) -> Result<()> {
        let path = self.key_to_path(key);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&path, data).await?;
        Ok(())
    }

    async fn retrieve(&self, key: &str) -> Result<Vec<u8>> {
        let path = self.key_to_path(key);
        if !path.exists() {
            return Err(ShipperError::NotFound(key.to_string()));
        }
        Ok(tokio::fs::read(&path).await?)
    }

    async fn delete(&self, key: &str) -> Result<()> {
        let path = self.key_to_path(key);
        if path.exists() {
            tokio::fs::remove_file(&path).await?;
        }
        Ok(())
    }

    async fn exists(&self, key: &str) -> Result<bool> {
        let path = self.key_to_path(key);
        Ok(path.exists())
    }
}

/// In-memory storage backend (for testing).
#[derive(Default)]
pub struct MemoryStorageBackend {
    data: tokio::sync::RwLock<HashMap<String, Vec<u8>>>,
}

#[async_trait]
impl LogStorageBackend for MemoryStorageBackend {
    async fn store(&self, key: &str, data: &[u8]) -> Result<()> {
        let mut store = self.data.write().await;
        store.insert(key.to_string(), data.to_vec());
        Ok(())
    }

    async fn retrieve(&self, key: &str) -> Result<Vec<u8>> {
        let store = self.data.read().await;
        store
            .get(key)
            .cloned()
            .ok_or_else(|| ShipperError::NotFound(key.to_string()))
    }

    async fn delete(&self, key: &str) -> Result<()> {
        let mut store = self.data.write().await;
        store.remove(key);
        Ok(())
    }

    async fn exists(&self, key: &str) -> Result<bool> {
        let store = self.data.read().await;
        Ok(store.contains_key(key))
    }
}

/// Log shipper for archiving logs to storage.
pub struct LogShipper<B: LogStorageBackend> {
    backend: B,
    config: ShipperConfig,
}

impl<B: LogStorageBackend> LogShipper<B> {
    /// Create a new shipper with the given backend.
    pub fn new(backend: B, config: ShipperConfig) -> Self {
        Self { backend, config }
    }

    /// Generate storage key for a run's logs.
    fn run_key(&self, run_id: RunId) -> String {
        let extension = if self.config.compress {
            "jsonl.gz"
        } else {
            "jsonl"
        };
        format!("runs/{}/{}.{}", run_id, "logs", extension)
    }

    /// Generate storage key for a job's logs.
    fn job_key(&self, run_id: RunId, job_id: JobId) -> String {
        let extension = if self.config.compress {
            "jsonl.gz"
        } else {
            "jsonl"
        };
        format!("runs/{}/jobs/{}/logs.{}", run_id, job_id, extension)
    }

    /// Serialize logs to JSONL format.
    fn serialize_logs(&self, lines: &[LogLine]) -> Result<Vec<u8>> {
        let mut output = Vec::new();
        for line in lines {
            serde_json::to_writer(&mut output, line)?;
            output.write_all(b"\n")?;
        }

        if self.config.compress {
            // Simple compression placeholder - in production would use gzip/zstd
            Ok(output)
        } else {
            Ok(output)
        }
    }

    /// Deserialize logs from JSONL format.
    fn deserialize_logs(&self, data: &[u8]) -> Result<Vec<LogLine>> {
        let data = if self.config.compress {
            // Decompress placeholder
            data.to_vec()
        } else {
            data.to_vec()
        };

        let mut lines = Vec::new();
        for line in data.split(|&b| b == b'\n') {
            if line.is_empty() {
                continue;
            }
            let log_line: LogLine = serde_json::from_slice(line)?;
            lines.push(log_line);
        }
        Ok(lines)
    }

    /// Archive logs for a run.
    #[instrument(skip(self, lines), fields(run_id = %run_id, line_count = lines.len()))]
    pub async fn archive_run(&self, run_id: RunId, lines: &[LogLine]) -> Result<LogArchive> {
        let key = self.run_key(run_id);
        let data = self.serialize_logs(lines)?;
        let size_bytes = data.len() as u64;

        debug!("Archiving {} lines for run {}", lines.len(), run_id);
        self.backend.store(&key, &data).await?;

        Ok(LogArchive {
            run_id,
            job_id: None,
            step_id: None,
            archived_at: Utc::now(),
            line_count: lines.len() as u64,
            size_bytes,
            storage_key: key,
            compressed: self.config.compress,
        })
    }

    /// Archive logs for a specific job.
    #[instrument(skip(self, lines), fields(run_id = %run_id, job_id = %job_id))]
    pub async fn archive_job(
        &self,
        run_id: RunId,
        job_id: JobId,
        lines: &[LogLine],
    ) -> Result<LogArchive> {
        let key = self.job_key(run_id, job_id);
        let data = self.serialize_logs(lines)?;
        let size_bytes = data.len() as u64;

        self.backend.store(&key, &data).await?;

        Ok(LogArchive {
            run_id,
            job_id: Some(job_id),
            step_id: None,
            archived_at: Utc::now(),
            line_count: lines.len() as u64,
            size_bytes,
            storage_key: key,
            compressed: self.config.compress,
        })
    }

    /// Retrieve archived logs for a run.
    pub async fn retrieve_run(&self, run_id: RunId) -> Result<Vec<LogLine>> {
        let key = self.run_key(run_id);
        let data = self.backend.retrieve(&key).await?;
        self.deserialize_logs(&data)
    }

    /// Retrieve archived logs for a job.
    pub async fn retrieve_job(&self, run_id: RunId, job_id: JobId) -> Result<Vec<LogLine>> {
        let key = self.job_key(run_id, job_id);
        let data = self.backend.retrieve(&key).await?;
        self.deserialize_logs(&data)
    }

    /// Delete archived logs for a run.
    pub async fn delete_run(&self, run_id: RunId) -> Result<()> {
        let key = self.run_key(run_id);
        self.backend.delete(&key).await
    }

    /// Check if logs exist for a run.
    pub async fn exists(&self, run_id: RunId) -> Result<bool> {
        let key = self.run_key(run_id);
        self.backend.exists(&key).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capture::LogSource;

    fn make_log_line(content: &str, run_id: RunId) -> LogLine {
        LogLine {
            line_number: 1,
            timestamp: Utc::now(),
            content: content.to_string(),
            source: "stdout".to_string(),
            run_id,
            job_id: JobId::new(),
            step_id: StepId::new(),
            job_name: None,
            step_name: None,
        }
    }

    #[tokio::test]
    async fn test_archive_and_retrieve() {
        let backend = MemoryStorageBackend::default();
        let shipper = LogShipper::new(
            backend,
            ShipperConfig {
                compress: false,
                ..Default::default()
            },
        );

        let run_id = RunId::new();
        let lines = vec![
            make_log_line("Line 1", run_id),
            make_log_line("Line 2", run_id),
            make_log_line("Line 3", run_id),
        ];

        let archive = shipper.archive_run(run_id, &lines).await.unwrap();
        assert_eq!(archive.line_count, 3);
        assert!(archive.size_bytes > 0);

        let retrieved = shipper.retrieve_run(run_id).await.unwrap();
        assert_eq!(retrieved.len(), 3);
        assert_eq!(retrieved[0].content, "Line 1");
    }

    #[tokio::test]
    async fn test_exists() {
        let backend = MemoryStorageBackend::default();
        let shipper = LogShipper::new(backend, ShipperConfig::default());

        let run_id = RunId::new();
        assert!(!shipper.exists(run_id).await.unwrap());

        let lines = vec![make_log_line("Test", run_id)];
        shipper.archive_run(run_id, &lines).await.unwrap();

        assert!(shipper.exists(run_id).await.unwrap());
    }

    #[tokio::test]
    async fn test_delete() {
        let backend = MemoryStorageBackend::default();
        let shipper = LogShipper::new(backend, ShipperConfig::default());

        let run_id = RunId::new();
        let lines = vec![make_log_line("Test", run_id)];
        shipper.archive_run(run_id, &lines).await.unwrap();

        assert!(shipper.exists(run_id).await.unwrap());
        shipper.delete_run(run_id).await.unwrap();
        assert!(!shipper.exists(run_id).await.unwrap());
    }
}
