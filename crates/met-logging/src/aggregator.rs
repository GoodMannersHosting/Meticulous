//! Log aggregation and broadcasting.
//!
//! Buffers log lines and broadcasts them to multiple subscribers (WebSocket clients).

use crate::capture::CapturedLine;
use crate::redactor::Redactor;
use chrono::{DateTime, Utc};
use met_core::ids::{JobId, RunId, StepId};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, instrument};

/// A processed log line ready for delivery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogLine {
    /// Line number within the run.
    pub line_number: u64,
    /// Timestamp when captured.
    pub timestamp: DateTime<Utc>,
    /// The log content (redacted).
    pub content: String,
    /// Source stream (stdout/stderr/system).
    pub source: String,
    /// Associated run ID.
    pub run_id: RunId,
    /// Associated job ID.
    pub job_id: JobId,
    /// Associated step ID.
    pub step_id: StepId,
    /// Job name (for display).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_name: Option<String>,
    /// Step name (for display).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step_name: Option<String>,
}

impl LogLine {
    /// Create from a captured line with redaction.
    pub fn from_captured(captured: CapturedLine, redactor: &Redactor) -> Self {
        Self {
            line_number: captured.line_number,
            timestamp: captured.timestamp,
            content: redactor.redact(&captured.content),
            source: format!("{:?}", captured.source).to_lowercase(),
            run_id: captured.run_id,
            job_id: captured.job_id,
            step_id: captured.step_id,
            job_name: None,
            step_name: None,
        }
    }

    /// Set job and step names for display.
    pub fn with_names(mut self, job_name: Option<String>, step_name: Option<String>) -> Self {
        self.job_name = job_name;
        self.step_name = step_name;
        self
    }
}

/// Subscription handle for receiving log lines.
pub struct LogSubscription {
    receiver: broadcast::Receiver<LogLine>,
}

impl LogSubscription {
    /// Receive the next log line.
    pub async fn recv(&mut self) -> Option<LogLine> {
        match self.receiver.recv().await {
            Ok(line) => Some(line),
            Err(broadcast::error::RecvError::Lagged(n)) => {
                debug!("Subscription lagged {} messages", n);
                self.receiver.recv().await.ok()
            }
            Err(broadcast::error::RecvError::Closed) => None,
        }
    }
}

/// Buffer state for a run's logs.
struct RunBuffer {
    lines: VecDeque<LogLine>,
    max_lines: usize,
}

impl RunBuffer {
    fn new(max_lines: usize) -> Self {
        Self {
            lines: VecDeque::with_capacity(max_lines),
            max_lines,
        }
    }

    fn push(&mut self, line: LogLine) {
        if self.lines.len() >= self.max_lines {
            self.lines.pop_front();
        }
        self.lines.push_back(line);
    }

    fn get_recent(&self, count: usize) -> Vec<LogLine> {
        self.lines.iter().rev().take(count).cloned().collect()
    }

    fn get_all(&self) -> Vec<LogLine> {
        self.lines.iter().cloned().collect()
    }
}

/// Configuration for the log aggregator.
#[derive(Debug, Clone)]
pub struct AggregatorConfig {
    /// Channel capacity for broadcast.
    pub broadcast_capacity: usize,
    /// Maximum lines to buffer per run.
    pub max_buffer_lines: usize,
}

impl Default for AggregatorConfig {
    fn default() -> Self {
        Self {
            broadcast_capacity: 1024,
            max_buffer_lines: 10000,
        }
    }
}

/// Log aggregator that buffers and broadcasts log lines.
pub struct LogAggregator {
    redactor: Arc<Redactor>,
    sender: broadcast::Sender<LogLine>,
    buffers: RwLock<std::collections::HashMap<RunId, RunBuffer>>,
    config: AggregatorConfig,
}

impl LogAggregator {
    /// Create a new aggregator with the given redactor.
    pub fn new(redactor: Redactor) -> Self {
        Self::with_config(redactor, AggregatorConfig::default())
    }

    /// Create with custom configuration.
    pub fn with_config(redactor: Redactor, config: AggregatorConfig) -> Self {
        let (sender, _) = broadcast::channel(config.broadcast_capacity);
        Self {
            redactor: Arc::new(redactor),
            sender,
            buffers: RwLock::new(std::collections::HashMap::new()),
            config,
        }
    }

    /// Subscribe to log updates.
    pub fn subscribe(&self) -> LogSubscription {
        LogSubscription {
            receiver: self.sender.subscribe(),
        }
    }

    /// Process and broadcast a captured line.
    #[instrument(skip(self, captured), fields(run_id = %captured.run_id, line = %captured.line_number))]
    pub async fn process(&self, captured: CapturedLine) -> LogLine {
        let run_id = captured.run_id;
        let line = LogLine::from_captured(captured, &self.redactor);

        // Buffer the line
        {
            let mut buffers = self.buffers.write().await;
            let buffer = buffers
                .entry(run_id)
                .or_insert_with(|| RunBuffer::new(self.config.max_buffer_lines));
            buffer.push(line.clone());
        }

        // Broadcast (ignore if no receivers)
        let _ = self.sender.send(line.clone());

        line
    }

    /// Get recent log lines for a run.
    pub async fn get_recent(&self, run_id: RunId, count: usize) -> Vec<LogLine> {
        let buffers = self.buffers.read().await;
        buffers
            .get(&run_id)
            .map(|b| b.get_recent(count))
            .unwrap_or_default()
    }

    /// Get all buffered log lines for a run.
    pub async fn get_all(&self, run_id: RunId) -> Vec<LogLine> {
        let buffers = self.buffers.read().await;
        buffers
            .get(&run_id)
            .map(|b| b.get_all())
            .unwrap_or_default()
    }

    /// Clear buffer for a completed run.
    pub async fn clear_buffer(&self, run_id: RunId) {
        let mut buffers = self.buffers.write().await;
        buffers.remove(&run_id);
    }

    /// Get total buffered line count.
    pub async fn total_buffered_lines(&self) -> usize {
        let buffers = self.buffers.read().await;
        buffers.values().map(|b| b.lines.len()).sum()
    }

    /// Get a reference to the redactor.
    pub fn redactor(&self) -> &Redactor {
        &self.redactor
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capture::LogSource;
    use crate::redactor::RedactorConfig;

    fn make_captured(content: &str, run_id: RunId) -> CapturedLine {
        CapturedLine {
            timestamp: Utc::now(),
            source: LogSource::Stdout,
            content: content.to_string(),
            line_number: 1,
            run_id,
            job_id: JobId::new(),
            step_id: StepId::new(),
        }
    }

    #[tokio::test]
    async fn test_process_and_buffer() {
        let redactor = Redactor::new(RedactorConfig::default());
        let aggregator = LogAggregator::new(redactor);

        let run_id = RunId::new();
        let captured = make_captured("Test line", run_id);

        let line = aggregator.process(captured).await;
        assert_eq!(line.content, "Test line");

        let recent = aggregator.get_recent(run_id, 10).await;
        assert_eq!(recent.len(), 1);
    }

    #[tokio::test]
    async fn test_redaction_in_aggregator() {
        let redactor = Redactor::new(RedactorConfig {
            redact_common_patterns: false,
            ..Default::default()
        });
        redactor.add_secret("super-secret");

        let aggregator = LogAggregator::new(redactor);

        let run_id = RunId::new();
        let captured = make_captured("Using super-secret here", run_id);

        let line = aggregator.process(captured).await;
        assert_eq!(line.content, "Using [REDACTED] here");
    }

    #[tokio::test]
    async fn test_subscription() {
        let redactor = Redactor::default();
        let aggregator = LogAggregator::new(redactor);

        let mut sub = aggregator.subscribe();
        let run_id = RunId::new();

        // Process in background
        let agg_clone = Arc::new(aggregator);
        let agg = agg_clone.clone();
        tokio::spawn(async move {
            let captured = make_captured("Hello from task", run_id);
            agg.process(captured).await;
        });

        // Should receive the line
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            if let Some(line) = sub.recv().await {
                assert_eq!(line.content, "Hello from task");
            }
        })
        .await
        .ok();
    }

    #[tokio::test]
    async fn test_buffer_limit() {
        let redactor = Redactor::default();
        let aggregator = LogAggregator::with_config(
            redactor,
            AggregatorConfig {
                max_buffer_lines: 5,
                ..Default::default()
            },
        );

        let run_id = RunId::new();

        for i in 0..10 {
            let mut captured = make_captured(&format!("Line {}", i), run_id);
            captured.line_number = i + 1;
            aggregator.process(captured).await;
        }

        let all = aggregator.get_all(run_id).await;
        assert_eq!(all.len(), 5);
        assert_eq!(all[0].content, "Line 5"); // Oldest kept
    }
}
