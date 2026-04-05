//! Per-step log spooling and live streaming to the controller.
//!
//! Workflow output is written to a workspace spool file and sent over `StreamLogs` with
//! bounded channels so a slow controller applies backpressure (child stdout/stderr may block).

use std::path::{Path, PathBuf};

use met_proto::agent::v1::agent_service_client::AgentServiceClient;
use met_proto::agent::v1::{LogChunk, LogStream};
use met_proto::common::v1::Timestamp;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::Channel;
use tonic::Request;
use tracing::warn;

use crate::error::{AgentError, Result};

#[derive(Debug, Clone, Copy)]
enum LineKind {
    Stdout,
    Stderr,
}

enum StreamMsg {
    Line {
        kind: LineKind,
        /// One line without trailing `\n` (we add it for the spool and chunk).
        line: String,
    },
    Telemetry {
        stream: LogStream,
        /// UTF-8 JSON payload (exec binary, syscall, runtime script).
        json: String,
    },
}

/// Cloneable handle used from stdout/stderr reader tasks. Sends apply backpressure when full.
#[derive(Clone)]
pub struct StepLogPipe {
    tx: mpsc::Sender<StreamMsg>,
}

impl StepLogPipe {
    async fn send_line(&self, kind: LineKind, line: &str) -> Result<()> {
        self.tx
            .send(StreamMsg::Line {
                kind,
                line: line.to_string(),
            })
            .await
            .map_err(|_| {
                AgentError::LogStream(
                    "log pipeline closed — controller may be unreachable".to_string(),
                )
            })
    }

    pub async fn send_stdout_line(&self, line: &str) -> Result<()> {
        self.send_line(LineKind::Stdout, line).await
    }

    pub async fn send_stderr_line(&self, line: &str) -> Result<()> {
        self.send_line(LineKind::Stderr, line).await
    }

    /// Non-log telemetry on the same `StreamLogs` sequence space (not spooled to disk).
    pub async fn send_telemetry(&self, stream: LogStream, json: &str) -> Result<()> {
        self.tx
            .send(StreamMsg::Telemetry {
                stream,
                json: json.to_string(),
            })
            .await
            .map_err(|_| {
                AgentError::LogStream(
                    "log pipeline closed — controller may be unreachable".to_string(),
                )
            })
    }
}

/// Owns the coordinator task; call [`Self::finish`] to await upload and ACK.
pub struct StepLogSession {
    line_tx: Option<mpsc::Sender<StreamMsg>>,
    upload: Option<tokio::task::JoinHandle<std::result::Result<tonic::Response<met_proto::agent::v1::LogAck>, tonic::Status>>>,
    writer_done: Option<tokio::task::JoinHandle<Result<()>>>,
}

impl StepLogSession {
    /// Start spool + `StreamLogs` pipeline.
    pub fn spawn(
        mut client: AgentServiceClient<Channel>,
        job_run_id: String,
        step_run_id: String,
        spool_path: PathBuf,
        line_buffer: usize,
        log_chunk_buffer: usize,
    ) -> Result<Self> {
        let (line_tx, line_rx) = mpsc::channel::<StreamMsg>(line_buffer);
        let (grpc_tx, grpc_rx) = mpsc::channel::<LogChunk>(log_chunk_buffer);

        let stream = ReceiverStream::new(grpc_rx);
        let upload = tokio::spawn(async move { client.stream_logs(Request::new(stream)).await });

        let job_run_id_w = job_run_id.clone();
        let step_run_id_w = step_run_id.clone();

        let writer_done = tokio::spawn(async move {
            pump_stream_messages(spool_path, line_rx, grpc_tx, job_run_id_w, step_run_id_w).await
        });

        Ok(Self {
            line_tx: Some(line_tx),
            upload: Some(upload),
            writer_done: Some(writer_done),
        })
    }

    pub fn pipe(&self) -> Option<StepLogPipe> {
        self.line_tx.as_ref().map(|tx| StepLogPipe { tx: tx.clone() })
    }

    /// Waits until all [`StepLogPipe`] clones are dropped, spool is flushed, stream completes, and returns `last_sequence` from `LogAck`.
    pub async fn finish(mut self) -> Result<i64> {
        self.line_tx.take();

        let writer = self
            .writer_done
            .take()
            .ok_or_else(|| AgentError::Internal("step log writer handle missing".to_string()))?;
        writer.await.map_err(|e| {
            AgentError::LogStream(format!("log writer task panicked or cancelled: {e}"))
        })??;

        let upload = self
            .upload
            .take()
            .ok_or_else(|| AgentError::Internal("stream_logs task handle missing".to_string()))?;
        let resp = upload.await.map_err(|e| {
            AgentError::LogStream(format!("stream_logs task panicked or cancelled: {e}"))
        })?
        .map_err(AgentError::Grpc)?;

        Ok(resp.into_inner().last_sequence)
    }
}

async fn pump_stream_messages(
    spool_path: PathBuf,
    mut msg_rx: mpsc::Receiver<StreamMsg>,
    grpc_tx: mpsc::Sender<LogChunk>,
    job_run_id: String,
    step_run_id: String,
) -> Result<()> {
    if let Some(parent) = spool_path.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(|e| {
            AgentError::Workspace(format!("create log spool parent {}: {e}", parent.display()))
        })?;
    }

    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&spool_path)
        .await
        .map_err(|e| {
            AgentError::Workspace(format!("open log spool {}: {e}", spool_path.display()))
        })?;

    let mut seq: i64 = 0;

    while let Some(msg) = msg_rx.recv().await {
        seq += 1;
        let ts = chrono::Utc::now();
        let timestamp = Some(Timestamp {
            seconds: ts.timestamp(),
            nanos: ts.timestamp_subsec_nanos() as i32,
        });

        match msg {
            StreamMsg::Line { kind, line } => {
                let with_newline = format!("{line}\n");
                file.write_all(with_newline.as_bytes())
                    .await
                    .map_err(|e| AgentError::Workspace(format!("write log spool: {e}")))?;

                let stream_ty = match kind {
                    LineKind::Stdout => LogStream::Stdout,
                    LineKind::Stderr => LogStream::Stderr,
                };

                let chunk = LogChunk {
                    job_run_id: job_run_id.clone(),
                    step_run_id: step_run_id.clone(),
                    content: with_newline.into_bytes(),
                    stream: stream_ty as i32,
                    sequence: seq,
                    timestamp,
                };

                grpc_tx.send(chunk).await.map_err(|_| {
                    AgentError::LogStream(
                        "failed to send log chunk (controller disconnected?)".to_string(),
                    )
                })?;
            }
            StreamMsg::Telemetry { stream, json } => {
                let chunk = LogChunk {
                    job_run_id: job_run_id.clone(),
                    step_run_id: step_run_id.clone(),
                    content: json.into_bytes(),
                    stream: stream as i32,
                    sequence: seq,
                    timestamp,
                };
                grpc_tx.send(chunk).await.map_err(|_| {
                    AgentError::LogStream(
                        "failed to send telemetry chunk (controller disconnected?)".to_string(),
                    )
                })?;
            }
        }
    }

    drop(grpc_tx);
    Ok(())
}

impl Drop for StepLogSession {
    fn drop(&mut self) {
        if self.line_tx.is_some() {
            warn!("StepLogSession dropped without finish() — cancelling log upload");
            if let Some(h) = self.upload.take() {
                h.abort();
            }
            if let Some(h) = self.writer_done.take() {
                h.abort();
            }
        }
    }
}

/// Resolved path for `.meticulous/logs/{step_run_id}.log` under the job workspace.
pub fn step_log_spool_path(workspace: &Path, step_run_id: &str) -> PathBuf {
    workspace
        .join(".meticulous")
        .join("logs")
        .join(format!("{step_run_id}.log"))
}
