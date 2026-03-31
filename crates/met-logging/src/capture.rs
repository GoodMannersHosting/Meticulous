//! Log capture from container execution.
//!
//! Provides async streaming of stdout/stderr from job containers.

use chrono::{DateTime, Utc};
use met_core::ids::{JobId, RunId, StepId};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::mpsc;
use tracing::{debug, error, instrument};

/// Configuration for log capture.
#[derive(Debug, Clone)]
pub struct LogCaptureConfig {
    /// Maximum line length before truncation.
    pub max_line_length: usize,
    /// Buffer size for channel.
    pub buffer_size: usize,
    /// Whether to capture timestamps from source.
    pub capture_timestamps: bool,
}

impl Default for LogCaptureConfig {
    fn default() -> Self {
        Self {
            max_line_length: 64 * 1024, // 64KB max line
            buffer_size: 1024,
            capture_timestamps: true,
        }
    }
}

/// Source of a log line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogSource {
    /// Standard output.
    Stdout,
    /// Standard error.
    Stderr,
    /// System-generated message.
    System,
}

/// A captured log line with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapturedLine {
    /// Timestamp when the line was captured.
    pub timestamp: DateTime<Utc>,
    /// Source of the line.
    pub source: LogSource,
    /// The log content.
    pub content: String,
    /// Line number within the step.
    pub line_number: u64,
    /// Associated run ID.
    pub run_id: RunId,
    /// Associated job ID.
    pub job_id: JobId,
    /// Associated step ID.
    pub step_id: StepId,
}

/// Log capture handle for streaming container output.
pub struct LogCapture {
    config: LogCaptureConfig,
    run_id: RunId,
    job_id: JobId,
    step_id: StepId,
    line_counter: Arc<std::sync::atomic::AtomicU64>,
}

impl LogCapture {
    /// Create a new log capture instance.
    pub fn new(
        config: LogCaptureConfig,
        run_id: RunId,
        job_id: JobId,
        step_id: StepId,
    ) -> Self {
        Self {
            config,
            run_id,
            job_id,
            step_id,
            line_counter: Arc::new(std::sync::atomic::AtomicU64::new(1)),
        }
    }

    /// Capture a single line.
    pub fn capture_line(&self, content: String, source: LogSource) -> CapturedLine {
        let line_number = self
            .line_counter
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        let content = if content.len() > self.config.max_line_length {
            let mut truncated = content[..self.config.max_line_length].to_string();
            truncated.push_str("... [truncated]");
            truncated
        } else {
            content
        };

        CapturedLine {
            timestamp: Utc::now(),
            source,
            content,
            line_number,
            run_id: self.run_id,
            job_id: self.job_id,
            step_id: self.step_id,
        }
    }

    /// Stream lines from an async reader (e.g., stdout/stderr).
    #[instrument(skip(self, reader), fields(run_id = %self.run_id, step_id = %self.step_id))]
    pub async fn stream_reader<R>(
        &self,
        reader: R,
        source: LogSource,
        tx: mpsc::Sender<CapturedLine>,
    ) where
        R: tokio::io::AsyncRead + Unpin,
    {
        let mut reader = BufReader::new(reader);
        let mut line = String::new();

        loop {
            line.clear();
            match reader.read_line(&mut line).await {
                Ok(0) => {
                    debug!("EOF reached for {:?}", source);
                    break;
                }
                Ok(_) => {
                    let content = line.trim_end_matches('\n').to_string();
                    let captured = self.capture_line(content, source);
                    if tx.send(captured).await.is_err() {
                        debug!("Receiver dropped, stopping capture");
                        break;
                    }
                }
                Err(e) => {
                    error!("Error reading log: {}", e);
                    break;
                }
            }
        }
    }

    /// Create a system log line (for agent-generated messages).
    pub fn system_message(&self, message: impl Into<String>) -> CapturedLine {
        self.capture_line(message.into(), LogSource::System)
    }

    /// Get the current line count.
    pub fn line_count(&self) -> u64 {
        self.line_counter.load(std::sync::atomic::Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[tokio::test]
    async fn test_capture_line() {
        let config = LogCaptureConfig::default();
        let capture = LogCapture::new(
            config,
            RunId::new(),
            JobId::new(),
            StepId::new(),
        );

        let line = capture.capture_line("Hello, world!".to_string(), LogSource::Stdout);
        assert_eq!(line.content, "Hello, world!");
        assert_eq!(line.line_number, 1);
        assert_eq!(line.source, LogSource::Stdout);

        let line2 = capture.capture_line("Second line".to_string(), LogSource::Stderr);
        assert_eq!(line2.line_number, 2);
    }

    #[tokio::test]
    async fn test_truncation() {
        let config = LogCaptureConfig {
            max_line_length: 10,
            ..Default::default()
        };
        let capture = LogCapture::new(
            config,
            RunId::new(),
            JobId::new(),
            StepId::new(),
        );

        let line = capture.capture_line("This is a very long line".to_string(), LogSource::Stdout);
        assert!(line.content.ends_with("... [truncated]"));
    }

    #[tokio::test]
    async fn test_stream_reader() {
        let config = LogCaptureConfig::default();
        let capture = LogCapture::new(
            config,
            RunId::new(),
            JobId::new(),
            StepId::new(),
        );

        let data = "Line 1\nLine 2\nLine 3\n";
        let reader = Cursor::new(data);

        let (tx, mut rx) = mpsc::channel(10);
        capture.stream_reader(reader, LogSource::Stdout, tx).await;

        let mut lines = vec![];
        while let Ok(line) = rx.try_recv() {
            lines.push(line);
        }

        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].content, "Line 1");
        assert_eq!(lines[1].content, "Line 2");
        assert_eq!(lines[2].content, "Line 3");
    }
}
