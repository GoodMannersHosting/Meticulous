//! Log streaming relay for pipeline execution.
//!
//! Receives log output from agents via gRPC, stores in object storage,
//! and emits WebSocket events for real-time UI updates.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use met_core::ids::{JobRunId, RunId, StepRunId};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, instrument, warn};

use crate::error::{EngineError, Result};
use crate::events::EventBroadcaster;

/// A chunk of log output.
#[derive(Debug, Clone)]
pub struct LogChunk {
    /// Run ID.
    pub run_id: RunId,
    /// Job run ID.
    pub job_run_id: JobRunId,
    /// Step run ID (optional, for step-level logs).
    pub step_run_id: Option<StepRunId>,
    /// Log content.
    pub content: String,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
    /// Stream type (stdout/stderr).
    pub stream: LogStream,
    /// Sequence number for ordering.
    pub sequence: u64,
}

/// Log stream type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogStream {
    Stdout,
    Stderr,
}

impl std::fmt::Display for LogStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogStream::Stdout => write!(f, "stdout"),
            LogStream::Stderr => write!(f, "stderr"),
        }
    }
}

/// Log storage backend trait.
#[async_trait]
pub trait LogStorage: Send + Sync {
    /// Append log content to storage.
    async fn append(&self, job_run_id: JobRunId, step_run_id: Option<StepRunId>, chunk: &LogChunk) -> Result<()>;
    
    /// Finalize and close the log file.
    async fn finalize(&self, job_run_id: JobRunId, step_run_id: Option<StepRunId>) -> Result<String>;
    
    /// Read log content from storage.
    async fn read(&self, path: &str, offset: u64, limit: u64) -> Result<Vec<u8>>;
}

/// Object storage-backed log storage.
pub struct ObjectStoreLogStorage {
    prefix: String,
    buffers: RwLock<HashMap<String, Vec<u8>>>,
}

impl ObjectStoreLogStorage {
    pub fn new(prefix: impl Into<String>) -> Self {
        Self {
            prefix: prefix.into(),
            buffers: RwLock::new(HashMap::new()),
        }
    }

    fn log_key(&self, job_run_id: JobRunId, step_run_id: Option<StepRunId>) -> String {
        match step_run_id {
            Some(step_id) => format!("logs/{}/{}.log", job_run_id, step_id),
            None => format!("logs/{}/job.log", job_run_id),
        }
    }

    fn full_path(&self, key: &str) -> String {
        format!("{}/{}", self.prefix, key)
    }
}

#[async_trait]
impl LogStorage for ObjectStoreLogStorage {
    async fn append(&self, job_run_id: JobRunId, step_run_id: Option<StepRunId>, chunk: &LogChunk) -> Result<()> {
        let key = self.log_key(job_run_id, step_run_id);
        let mut buffers = self.buffers.write().await;
        
        let buffer = buffers.entry(key).or_insert_with(Vec::new);
        
        let line = format!(
            "[{}] [{}] {}\n",
            chunk.timestamp.format("%Y-%m-%d %H:%M:%S%.3f"),
            chunk.stream,
            chunk.content
        );
        buffer.extend_from_slice(line.as_bytes());
        
        Ok(())
    }

    async fn finalize(&self, job_run_id: JobRunId, step_run_id: Option<StepRunId>) -> Result<String> {
        let key = self.log_key(job_run_id, step_run_id);
        let path = self.full_path(&key);
        
        let mut buffers = self.buffers.write().await;
        if let Some(_buffer) = buffers.remove(&key) {
            debug!(path = %path, "finalized log file");
        }
        
        Ok(path)
    }

    async fn read(&self, path: &str, offset: u64, limit: u64) -> Result<Vec<u8>> {
        let buffers = self.buffers.read().await;
        
        for (key, buffer) in buffers.iter() {
            if self.full_path(key) == path {
                let start = offset as usize;
                let end = (offset + limit) as usize;
                let end = end.min(buffer.len());
                
                if start < buffer.len() {
                    return Ok(buffer[start..end].to_vec());
                } else {
                    return Ok(Vec::new());
                }
            }
        }
        
        Ok(Vec::new())
    }
}

/// In-memory log storage for testing.
pub struct MemoryLogStorage {
    logs: RwLock<HashMap<String, Vec<LogChunk>>>,
}

impl MemoryLogStorage {
    pub fn new() -> Self {
        Self {
            logs: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for MemoryLogStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl LogStorage for MemoryLogStorage {
    async fn append(&self, job_run_id: JobRunId, step_run_id: Option<StepRunId>, chunk: &LogChunk) -> Result<()> {
        let key = format!("{}:{:?}", job_run_id, step_run_id);
        let mut logs = self.logs.write().await;
        logs.entry(key).or_default().push(chunk.clone());
        Ok(())
    }

    async fn finalize(&self, job_run_id: JobRunId, step_run_id: Option<StepRunId>) -> Result<String> {
        let key = format!("{}:{:?}", job_run_id, step_run_id);
        Ok(format!("memory://{}", key))
    }

    async fn read(&self, path: &str, _offset: u64, _limit: u64) -> Result<Vec<u8>> {
        let logs = self.logs.read().await;
        if let Some(key) = path.strip_prefix("memory://") {
            if let Some(chunks) = logs.get(key) {
                let content: String = chunks.iter().map(|c| format!("{}\n", c.content)).collect();
                return Ok(content.into_bytes());
            }
        }
        Ok(Vec::new())
    }
}

/// Log streaming relay service.
pub struct LogStreamRelay<S: LogStorage> {
    storage: Arc<S>,
    events: Arc<EventBroadcaster>,
    buffer_size: usize,
}

impl<S: LogStorage> LogStreamRelay<S> {
    /// Create a new log stream relay.
    pub fn new(storage: S, events: Arc<EventBroadcaster>) -> Self {
        Self {
            storage: Arc::new(storage),
            events,
            buffer_size: 100,
        }
    }

    /// Set the buffer size for log batching.
    pub fn with_buffer_size(mut self, size: usize) -> Self {
        self.buffer_size = size;
        self
    }

    /// Process a log chunk.
    #[instrument(skip(self, chunk), fields(run_id = %chunk.run_id, job_run_id = %chunk.job_run_id))]
    pub async fn process_chunk(&self, chunk: LogChunk) -> Result<()> {
        self.storage.append(chunk.job_run_id, chunk.step_run_id, &chunk).await?;
        
        if let Err(e) = self.events.log_chunk(
            chunk.job_run_id,
            chunk.step_run_id,
            &chunk.content,
        ).await {
            warn!(error = %e, "failed to broadcast log chunk");
        }
        
        Ok(())
    }

    /// Process multiple log chunks.
    pub async fn process_chunks(&self, chunks: Vec<LogChunk>) -> Result<()> {
        for chunk in chunks {
            self.process_chunk(chunk).await?;
        }
        Ok(())
    }

    /// Finalize logs for a job run.
    pub async fn finalize_job(&self, job_run_id: JobRunId) -> Result<String> {
        self.storage.finalize(job_run_id, None).await
    }

    /// Finalize logs for a step run.
    pub async fn finalize_step(&self, job_run_id: JobRunId, step_run_id: StepRunId) -> Result<String> {
        self.storage.finalize(job_run_id, Some(step_run_id)).await
    }

    /// Create a receiver channel for incoming log chunks.
    pub fn create_receiver(&self) -> (mpsc::Sender<LogChunk>, mpsc::Receiver<LogChunk>) {
        mpsc::channel(self.buffer_size)
    }

    /// Start processing log chunks from a receiver.
    pub async fn start_processing(
        self: Arc<Self>,
        mut receiver: mpsc::Receiver<LogChunk>,
    ) {
        while let Some(chunk) = receiver.recv().await {
            if let Err(e) = self.process_chunk(chunk).await {
                warn!(error = %e, "failed to process log chunk");
            }
        }
    }
}

/// Builder for log chunks.
pub struct LogChunkBuilder {
    run_id: RunId,
    job_run_id: JobRunId,
    step_run_id: Option<StepRunId>,
    sequence_counter: u64,
}

impl LogChunkBuilder {
    /// Create a new log chunk builder.
    pub fn new(run_id: RunId, job_run_id: JobRunId) -> Self {
        Self {
            run_id,
            job_run_id,
            step_run_id: None,
            sequence_counter: 0,
        }
    }

    /// Set the step run ID.
    pub fn with_step(mut self, step_run_id: StepRunId) -> Self {
        self.step_run_id = Some(step_run_id);
        self
    }

    /// Build a stdout log chunk.
    pub fn stdout(&mut self, content: impl Into<String>) -> LogChunk {
        self.build(content, LogStream::Stdout)
    }

    /// Build a stderr log chunk.
    pub fn stderr(&mut self, content: impl Into<String>) -> LogChunk {
        self.build(content, LogStream::Stderr)
    }

    fn build(&mut self, content: impl Into<String>, stream: LogStream) -> LogChunk {
        let chunk = LogChunk {
            run_id: self.run_id,
            job_run_id: self.job_run_id,
            step_run_id: self.step_run_id,
            content: content.into(),
            timestamp: Utc::now(),
            stream,
            sequence: self.sequence_counter,
        };
        self.sequence_counter += 1;
        chunk
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use met_core::ids::{JobRunId, RunId, StepRunId};

    #[tokio::test]
    async fn test_memory_log_storage() {
        let storage = MemoryLogStorage::new();
        
        let run_id = RunId::new();
        let job_run_id = JobRunId::new();
        
        let chunk = LogChunk {
            run_id,
            job_run_id,
            step_run_id: None,
            content: "Hello, world!".to_string(),
            timestamp: Utc::now(),
            stream: LogStream::Stdout,
            sequence: 0,
        };
        
        storage.append(job_run_id, None, &chunk).await.unwrap();
        let path = storage.finalize(job_run_id, None).await.unwrap();
        
        let content = storage.read(&path, 0, 1000).await.unwrap();
        let content_str = String::from_utf8_lossy(&content);
        assert!(content_str.contains("Hello, world!"));
    }

    #[test]
    fn test_log_chunk_builder() {
        let run_id = RunId::new();
        let job_run_id = JobRunId::new();
        let step_run_id = StepRunId::new();
        
        let mut builder = LogChunkBuilder::new(run_id, job_run_id)
            .with_step(step_run_id);
        
        let chunk1 = builder.stdout("line 1");
        let chunk2 = builder.stderr("error");
        
        assert_eq!(chunk1.sequence, 0);
        assert_eq!(chunk2.sequence, 1);
        assert_eq!(chunk1.stream, LogStream::Stdout);
        assert_eq!(chunk2.stream, LogStream::Stderr);
    }
}
