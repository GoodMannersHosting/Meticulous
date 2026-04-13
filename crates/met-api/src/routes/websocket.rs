//! WebSocket routes for real-time log streaming.
//!
//! Security: WebSocket authentication is handled via the first message after connection,
//! NOT via URL query parameters. This prevents token leakage in logs, browser history,
//! and referrer headers.

use axum::{
    Router,
    extract::{
        Path, Query, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::{
        IntoResponse,
        sse::{Event, KeepAlive, Sse},
    },
    routing::get,
};
use futures::{SinkExt, StreamExt, stream};
use met_core::ids::{JobRunId, RunId};
use serde::{Deserialize, Serialize};
use tracing::{debug, instrument, warn};

use crate::{auth::JwtValidator, extractors::CurrentUser, state::AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/ws/runs/{id}/logs", get(stream_run_logs))
        .route("/ws/runs/{id}/logs/sse", get(stream_run_logs_sse))
        .route("/ws/jobs/{id}/logs", get(stream_job_logs))
}

#[derive(Debug, Deserialize)]
pub struct LogStreamQuery {
    follow: Option<bool>,
    from_line: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum WsClientMessage {
    #[serde(rename = "auth")]
    Auth { token: String },
    #[serde(rename = "close")]
    Close,
    #[serde(rename = "ping")]
    Ping,
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
    End {
        run_id: String,
        final_status: String,
    },
}

#[instrument(skip(state, ws))]
async fn stream_run_logs(
    State(state): State<AppState>,
    Path(run_id): Path<RunId>,
    Query(query): Query<LogStreamQuery>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    let follow = query.follow.unwrap_or(true);
    let from_line = query.from_line.unwrap_or(0);

    ws.on_upgrade(move |socket| handle_run_log_stream(socket, state, run_id, follow, from_line))
}

async fn handle_run_log_stream(
    socket: WebSocket,
    state: AppState,
    run_id: RunId,
    follow: bool,
    from_line: u64,
) {
    let (mut sender, mut receiver) = socket.split();

    // Wait for auth message first (with timeout)
    let auth_timeout = tokio::time::Duration::from_secs(10);
    let user = match tokio::time::timeout(auth_timeout, wait_for_auth(&mut receiver, &state)).await
    {
        Ok(Ok(user)) => {
            debug!(user_id = %user.user_id, "WebSocket authenticated");
            Some(user)
        }
        Ok(Err(e)) => {
            warn!(error = %e, "WebSocket auth failed");
            let error_msg = WsMessage::Error {
                message: format!("Authentication failed: {}", e),
            };
            if let Ok(json) = serde_json::to_string(&error_msg) {
                let _ = sender.send(Message::Text(json.into())).await;
            }
            let _ = sender.send(Message::Close(None)).await;
            return;
        }
        Err(_) => {
            warn!("WebSocket auth timeout");
            let error_msg = WsMessage::Error {
                message: "Authentication timeout - send auth message within 10 seconds".to_string(),
            };
            if let Ok(json) = serde_json::to_string(&error_msg) {
                let _ = sender.send(Message::Text(json.into())).await;
            }
            let _ = sender.send(Message::Close(None)).await;
            return;
        }
    };

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
                        if let Ok(client_msg) = serde_json::from_str::<WsClientMessage>(&text) {
                            match client_msg {
                                WsClientMessage::Close => break,
                                WsClientMessage::Ping => {
                                    let pong = serde_json::json!({"type": "pong"});
                                    let _ = sender.send(Message::Text(pong.to_string().into())).await;
                                }
                                WsClientMessage::Auth { .. } => {
                                    // Already authenticated, ignore duplicate auth
                                }
                            }
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

/// Wait for an auth message and validate the token.
async fn wait_for_auth(
    receiver: &mut futures::stream::SplitStream<WebSocket>,
    state: &AppState,
) -> Result<CurrentUser, String> {
    while let Some(msg) = receiver.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                if let Ok(WsClientMessage::Auth { token }) = serde_json::from_str(&text) {
                    let validator = JwtValidator::new(&state.config.jwt);
                    return validator.validate(&token).map_err(|e| e.to_string());
                }
            }
            Ok(Message::Close(_)) => {
                return Err("Connection closed before authentication".to_string());
            }
            Ok(Message::Ping(data)) => {
                // Can't send pong here without sender, but we don't need to during auth
                debug!("Received ping during auth phase");
            }
            Err(e) => {
                return Err(format!("WebSocket error: {}", e));
            }
            _ => {}
        }
    }
    Err("Connection closed before authentication".to_string())
}

/// SSE fallback for environments where WebSocket connections are unavailable
/// (e.g. behind certain proxies, corporate firewalls, or HTTP/1.1-only clients).
#[instrument(skip(_state))]
async fn stream_run_logs_sse(
    State(_state): State<AppState>,
    Path(run_id): Path<RunId>,
    Query(query): Query<LogStreamQuery>,
) -> Sse<impl futures::Stream<Item = Result<Event, std::convert::Infallible>>> {
    let follow = query.follow.unwrap_or(true);
    let from_line = query.from_line.unwrap_or(0);

    let stream = stream::unfold(
        (from_line, follow, run_id),
        |(line_number, follow, run_id)| async move {
            if line_number >= 50 && !follow {
                return None;
            }

            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

            let next_line = line_number + 1;
            let log = LogLine {
                line_number: next_line,
                timestamp: chrono::Utc::now(),
                level: "INFO".to_string(),
                job_name: Some("build".to_string()),
                step_name: Some("compile".to_string()),
                content: format!("Processing line {}...", next_line),
            };

            let event = match serde_json::to_string(&log) {
                Ok(json) => Event::default().data(json).event("log"),
                Err(_) => Event::default().data("error"),
            };

            Some((Ok(event), (next_line, follow, run_id)))
        },
    );

    Sse::new(stream).keep_alive(KeepAlive::default())
}

#[instrument(skip(state, ws))]
async fn stream_job_logs(
    State(state): State<AppState>,
    Path(job_run_id): Path<JobRunId>,
    Query(query): Query<LogStreamQuery>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    let follow = query.follow.unwrap_or(true);
    let from_line = query.from_line.unwrap_or(0);

    ws.on_upgrade(move |socket| handle_job_log_stream(socket, state, job_run_id, follow, from_line))
}

async fn handle_job_log_stream(
    socket: WebSocket,
    state: AppState,
    job_run_id: JobRunId,
    follow: bool,
    from_line: u64,
) {
    let (mut sender, mut receiver) = socket.split();

    // Wait for auth message first (with timeout)
    let auth_timeout = tokio::time::Duration::from_secs(10);
    let user = match tokio::time::timeout(auth_timeout, wait_for_auth(&mut receiver, &state)).await
    {
        Ok(Ok(user)) => {
            debug!(user_id = %user.user_id, "WebSocket authenticated");
            Some(user)
        }
        Ok(Err(e)) => {
            warn!(error = %e, "WebSocket auth failed");
            let error_msg = WsMessage::Error {
                message: format!("Authentication failed: {}", e),
            };
            if let Ok(json) = serde_json::to_string(&error_msg) {
                let _ = sender.send(Message::Text(json.into())).await;
            }
            let _ = sender.send(Message::Close(None)).await;
            return;
        }
        Err(_) => {
            warn!("WebSocket auth timeout");
            let error_msg = WsMessage::Error {
                message: "Authentication timeout - send auth message within 10 seconds".to_string(),
            };
            if let Ok(json) = serde_json::to_string(&error_msg) {
                let _ = sender.send(Message::Text(json.into())).await;
            }
            let _ = sender.send(Message::Close(None)).await;
            return;
        }
    };

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
                    Ok(Message::Text(text)) => {
                        if let Ok(client_msg) = serde_json::from_str::<WsClientMessage>(&text) {
                            match client_msg {
                                WsClientMessage::Close => break,
                                WsClientMessage::Ping => {
                                    let pong = serde_json::json!({"type": "pong"});
                                    let _ = sender.send(Message::Text(pong.to_string().into())).await;
                                }
                                WsClientMessage::Auth { .. } => {}
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}
