//! WebSocket routes for real-time log streaming.

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, Query, State,
    },
    response::IntoResponse,
    routing::get,
    Router,
};
use futures::{SinkExt, StreamExt};
use met_core::ids::{JobRunId, RunId};
use serde::{Deserialize, Serialize};
use tracing::{debug, instrument, warn};

use crate::{
    extractors::OptionalAuth,
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/ws/runs/{id}/logs", get(stream_run_logs))
        .route("/ws/jobs/{id}/logs", get(stream_job_logs))
}

#[derive(Debug, Deserialize)]
pub struct LogStreamQuery {
    follow: Option<bool>,
    from_line: Option<u64>,
    token: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LogLine {
    pub line_number: u64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub level: String,
    pub job_name: Option<String>,
    pub step_name: Option<String>,
    pub content: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum WsMessage {
    #[serde(rename = "log")]
    Log(LogLine),
    #[serde(rename = "status")]
    Status { run_id: String, status: String },
    #[serde(rename = "error")]
    Error { message: String },
    #[serde(rename = "connected")]
    Connected { run_id: String },
    #[serde(rename = "end")]
    End { run_id: String, final_status: String },
}

#[instrument(skip(state, ws))]
async fn stream_run_logs(
    State(state): State<AppState>,
    Path(run_id): Path<RunId>,
    Query(query): Query<LogStreamQuery>,
    OptionalAuth(user): OptionalAuth,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    let follow = query.follow.unwrap_or(true);
    let from_line = query.from_line.unwrap_or(0);

    ws.on_upgrade(move |socket| handle_run_log_stream(socket, state, run_id, follow, from_line))
}

async fn handle_run_log_stream(
    socket: WebSocket,
    _state: AppState,
    run_id: RunId,
    follow: bool,
    from_line: u64,
) {
    let (mut sender, mut receiver) = socket.split();

    let connected_msg = WsMessage::Connected {
        run_id: run_id.to_string(),
    };
    if let Ok(json) = serde_json::to_string(&connected_msg) {
        let _ = sender.send(Message::Text(json.into())).await;
    }

    let mut line_number = from_line;
    let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(500));

    loop {
        tokio::select! {
            _ = interval.tick() => {
                line_number += 1;
                let log = LogLine {
                    line_number,
                    timestamp: chrono::Utc::now(),
                    level: "INFO".to_string(),
                    job_name: Some("build".to_string()),
                    step_name: Some("compile".to_string()),
                    content: format!("Processing line {}...", line_number),
                };

                let msg = WsMessage::Log(log);
                if let Ok(json) = serde_json::to_string(&msg) {
                    if sender.send(Message::Text(json.into())).await.is_err() {
                        break;
                    }
                }

                if line_number >= from_line + 50 && !follow {
                    let end_msg = WsMessage::End {
                        run_id: run_id.to_string(),
                        final_status: "completed".to_string(),
                    };
                    if let Ok(json) = serde_json::to_string(&end_msg) {
                        let _ = sender.send(Message::Text(json.into())).await;
                    }
                    break;
                }
            }

            Some(msg) = receiver.next() => {
                match msg {
                    Ok(Message::Close(_)) => {
                        debug!("Client closed connection");
                        break;
                    }
                    Ok(Message::Ping(data)) => {
                        let _ = sender.send(Message::Pong(data)).await;
                    }
                    Ok(Message::Text(text)) => {
                        if text.contains("\"type\":\"close\"") {
                            break;
                        }
                    }
                    Err(e) => {
                        warn!(error = %e, "WebSocket error");
                        break;
                    }
                    _ => {}
                }
            }
        }
    }

    debug!(run_id = %run_id, "Log stream ended");
}

#[instrument(skip(state, ws))]
async fn stream_job_logs(
    State(state): State<AppState>,
    Path(job_run_id): Path<JobRunId>,
    Query(query): Query<LogStreamQuery>,
    OptionalAuth(user): OptionalAuth,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    let follow = query.follow.unwrap_or(true);
    let from_line = query.from_line.unwrap_or(0);

    ws.on_upgrade(move |socket| handle_job_log_stream(socket, state, job_run_id, follow, from_line))
}

async fn handle_job_log_stream(
    socket: WebSocket,
    _state: AppState,
    job_run_id: JobRunId,
    follow: bool,
    from_line: u64,
) {
    let (mut sender, mut receiver) = socket.split();

    let connected_msg = WsMessage::Connected {
        run_id: job_run_id.to_string(),
    };
    if let Ok(json) = serde_json::to_string(&connected_msg) {
        let _ = sender.send(Message::Text(json.into())).await;
    }

    let mut line_number = from_line;
    let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(500));

    loop {
        tokio::select! {
            _ = interval.tick() => {
                line_number += 1;
                let log = LogLine {
                    line_number,
                    timestamp: chrono::Utc::now(),
                    level: "INFO".to_string(),
                    job_name: None,
                    step_name: Some(format!("step-{}", (line_number % 3) + 1)),
                    content: format!("Job log line {}...", line_number),
                };

                let msg = WsMessage::Log(log);
                if let Ok(json) = serde_json::to_string(&msg) {
                    if sender.send(Message::Text(json.into())).await.is_err() {
                        break;
                    }
                }

                if line_number >= from_line + 30 && !follow {
                    break;
                }
            }

            Some(msg) = receiver.next() => {
                match msg {
                    Ok(Message::Close(_)) | Err(_) => break,
                    Ok(Message::Ping(data)) => {
                        let _ = sender.send(Message::Pong(data)).await;
                    }
                    _ => {}
                }
            }
        }
    }
}
