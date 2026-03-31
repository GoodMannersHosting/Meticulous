//! Run command handlers.

use crate::api_client::{ApiClient, Result};
use crate::output::{format_duration, format_timestamp, print, print_success, print_table_header, print_table_row, status_emoji};
use crate::OutputFormat;
use futures::StreamExt;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Run {
    pub id: String,
    pub pipeline_id: String,
    pub status: String,
    pub run_number: i64,
    pub triggered_by: String,
    pub branch: Option<String>,
    pub commit_sha: Option<String>,
    pub created_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RunListResponse {
    pub data: Vec<Run>,
    pub pagination: Pagination,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Pagination {
    pub has_more: bool,
    pub count: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CancelResponse {
    pub run_id: String,
    pub status: String,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RetryResponse {
    pub original_run_id: String,
    pub new_run_id: String,
    pub run_number: i64,
}

pub async fn list(client: &ApiClient, pipeline_id: &str, limit: u32, format: OutputFormat) -> Result<()> {
    #[derive(Serialize)]
    struct Query<'a> {
        pipeline_id: &'a str,
        limit: u32,
    }

    let response: RunListResponse = client
        .get_with_query("/runs", &Query { pipeline_id, limit })
        .await?;

    match format {
        OutputFormat::Table => {
            print_table_header(&["RUN#", "STATUS", "BRANCH", "DURATION", "STARTED"]);
            for r in &response.data {
                let duration = r.duration_ms.map(format_duration).unwrap_or_else(|| "-".to_string());
                let started = r.started_at.as_deref().map(format_timestamp).unwrap_or_else(|| "-".to_string());
                print_table_row(&[
                    &format!("#{}", r.run_number),
                    &format!("{} {}", status_emoji(&r.status), r.status),
                    r.branch.as_deref().unwrap_or("-"),
                    &duration,
                    &started,
                ]);
            }
            println!("\nShowing {} run(s)", response.pagination.count);
        }
        _ => {
            print(&response, format)?;
        }
    }

    Ok(())
}

pub async fn show(client: &ApiClient, id: &str, format: OutputFormat) -> Result<()> {
    let run: Run = client.get(&format!("/runs/{}", id)).await?;

    match format {
        OutputFormat::Table => {
            println!("Run #{}", run.run_number);
            println!("  ID:          {}", run.id);
            println!("  Status:      {} {}", status_emoji(&run.status), run.status);
            println!("  Branch:      {}", run.branch.as_deref().unwrap_or("-"));
            println!("  Commit:      {}", run.commit_sha.as_deref().unwrap_or("-"));
            println!("  Triggered:   {}", run.triggered_by);
            if let Some(duration) = run.duration_ms {
                println!("  Duration:    {}", format_duration(duration));
            }
            if let Some(ref started) = run.started_at {
                println!("  Started:     {}", format_timestamp(started));
            }
            if let Some(ref finished) = run.finished_at {
                println!("  Finished:    {}", format_timestamp(finished));
            }
        }
        _ => {
            print(&run, format)?;
        }
    }

    Ok(())
}

pub async fn logs(client: &ApiClient, id: &str, follow: bool) -> Result<()> {
    let ws_url = client.ws_url(&format!("/ws/runs/{}/logs?follow={}", id, follow));

    println!("Connecting to {}...", ws_url);

    let (ws_stream, _) = tokio_tungstenite::connect_async(&ws_url).await.map_err(|e| {
        crate::api_client::ApiError::InvalidResponse(format!("WebSocket connection failed: {}", e))
    })?;

    let (_, mut read) = ws_stream.split();

    while let Some(msg) = read.next().await {
        match msg {
            Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => {
                if let Ok(log_msg) = serde_json::from_str::<LogMessage>(&text) {
                    match log_msg {
                        LogMessage::Log(line) => {
                            let job = line.job_name.as_deref().unwrap_or("");
                            let step = line.step_name.as_deref().unwrap_or("");
                            println!("[{} {}] {}", job, step, line.content);
                        }
                        LogMessage::Connected { run_id } => {
                            println!("Connected to log stream for run {}", run_id);
                        }
                        LogMessage::End { final_status, .. } => {
                            println!("\n--- Run completed with status: {} ---", final_status);
                            break;
                        }
                        LogMessage::Error { message } => {
                            eprintln!("Error: {}", message);
                            break;
                        }
                        LogMessage::Status { status, .. } => {
                            println!("Status: {}", status);
                        }
                    }
                }
            }
            Ok(tokio_tungstenite::tungstenite::Message::Close(_)) => {
                break;
            }
            Err(e) => {
                eprintln!("WebSocket error: {}", e);
                break;
            }
            _ => {}
        }
    }

    Ok(())
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum LogMessage {
    #[serde(rename = "log")]
    Log(LogLine),
    #[serde(rename = "connected")]
    Connected { run_id: String },
    #[serde(rename = "end")]
    End { run_id: String, final_status: String },
    #[serde(rename = "error")]
    Error { message: String },
    #[serde(rename = "status")]
    Status { run_id: String, status: String },
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct LogLine {
    line_number: u64,
    content: String,
    job_name: Option<String>,
    step_name: Option<String>,
}

pub async fn cancel(client: &ApiClient, id: &str, format: OutputFormat) -> Result<()> {
    let response: CancelResponse = client
        .post(&format!("/runs/{}/cancel", id), &())
        .await?;

    match format {
        OutputFormat::Table => {
            print_success(&response.message);
            println!("Status: {} {}", status_emoji(&response.status), response.status);
        }
        _ => {
            print(&response, format)?;
        }
    }

    Ok(())
}

pub async fn retry(client: &ApiClient, id: &str, format: OutputFormat) -> Result<()> {
    let response: RetryResponse = client
        .post(&format!("/runs/{}/retry", id), &())
        .await?;

    match format {
        OutputFormat::Table => {
            print_success(&format!(
                "Run retried! New run #{} ({})",
                response.run_number, response.new_run_id
            ));
        }
        _ => {
            print(&response, format)?;
        }
    }

    Ok(())
}
