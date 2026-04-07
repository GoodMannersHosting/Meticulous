use crate::OutputFormat;
use crate::api_client::{ApiClient, Result};
use crate::output::{
    build_table, format_duration, format_status, format_timestamp, print_kv, print_serialized,
    print_success, print_table, status_icon,
};
use comfy_table::Cell;
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

#[derive(Debug, Serialize, Deserialize)]
pub struct ArtifactEntry {
    pub name: String,
    pub size_bytes: u64,
    pub content_type: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ArtifactListResponse {
    pub data: Vec<ArtifactEntry>,
}

pub async fn list(
    client: &ApiClient,
    pipeline_id: &str,
    limit: u32,
    format: OutputFormat,
) -> Result<()> {
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
            if response.data.is_empty() {
                println!("No runs found.");
                return Ok(());
            }
            let mut table = build_table(&["Run#", "Status", "Branch", "Duration", "Started"]);
            for r in &response.data {
                let duration = r
                    .duration_ms
                    .map(format_duration)
                    .unwrap_or_else(|| "-".to_string());
                let started = r
                    .started_at
                    .as_deref()
                    .map(format_timestamp)
                    .unwrap_or_else(|| "-".to_string());
                table.add_row(vec![
                    Cell::new(format!("#{}", r.run_number)),
                    Cell::new(format!(
                        "{} {}",
                        status_icon(&r.status),
                        format_status(&r.status)
                    )),
                    Cell::new(r.branch.as_deref().unwrap_or("-")),
                    Cell::new(duration),
                    Cell::new(started),
                ]);
            }
            print_table(&table);
            println!("\n{} run(s)", response.pagination.count);
        }
        _ => print_serialized(&response, format)?,
    }
    Ok(())
}

pub async fn status(client: &ApiClient, id: &str, format: OutputFormat) -> Result<()> {
    let run: Run = client.get(&format!("/runs/{}", id)).await?;

    match format {
        OutputFormat::Table => {
            println!(
                "Run #{} — {} {}",
                run.run_number,
                status_icon(&run.status),
                format_status(&run.status)
            );
            print_kv("ID", &run.id);
            print_kv("Pipeline", &run.pipeline_id);
            print_kv("Branch", run.branch.as_deref().unwrap_or("-"));
            print_kv("Commit", run.commit_sha.as_deref().unwrap_or("-"));
            print_kv("Triggered By", &run.triggered_by);
            if let Some(duration) = run.duration_ms {
                print_kv("Duration", &format_duration(duration));
            }
            if let Some(ref started) = run.started_at {
                print_kv("Started", &format_timestamp(started));
            }
            if let Some(ref finished) = run.finished_at {
                print_kv("Finished", &format_timestamp(finished));
            }
        }
        _ => print_serialized(&run, format)?,
    }
    Ok(())
}

pub async fn logs(
    client: &ApiClient,
    id: &str,
    follow: bool,
    job: Option<&str>,
    step: Option<&str>,
    tail: Option<u32>,
) -> Result<()> {
    let mut query_parts = vec![format!("follow={}", follow)];
    if let Some(j) = job {
        query_parts.push(format!("job={}", j));
    }
    if let Some(s) = step {
        query_parts.push(format!("step={}", s));
    }
    if let Some(t) = tail {
        query_parts.push(format!("tail={}", t));
    }
    let query = query_parts.join("&");

    let ws_url = client.ws_url(&format!("/ws/runs/{}/logs?{}", id, query));

    let (ws_stream, _) = tokio_tungstenite::connect_async(&ws_url)
        .await
        .map_err(|e| {
            crate::api_client::ApiError::InvalidResponse(format!(
                "WebSocket connection failed: {}",
                e
            ))
        })?;

    let (_, mut read) = ws_stream.split();

    while let Some(msg) = read.next().await {
        match msg {
            Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => {
                if let Ok(log_msg) = serde_json::from_str::<LogMessage>(&text) {
                    match log_msg {
                        LogMessage::Log(line) => {
                            let job_name = line.job_name.as_deref().unwrap_or("");
                            let step_name = line.step_name.as_deref().unwrap_or("");
                            println!("[{} {}] {}", job_name, step_name, line.content);
                        }
                        LogMessage::Connected { run_id } => {
                            eprintln!("Connected to log stream for run {}", run_id);
                        }
                        LogMessage::End { final_status, .. } => {
                            eprintln!("\n--- Run completed: {} ---", final_status);
                            break;
                        }
                        LogMessage::Error { message } => {
                            eprintln!("Error: {}", message);
                            break;
                        }
                        LogMessage::Status {
                            status: new_status, ..
                        } => {
                            eprintln!(
                                "Status: {} {}",
                                status_icon(&new_status),
                                format_status(&new_status)
                            );
                        }
                    }
                }
            }
            Ok(tokio_tungstenite::tungstenite::Message::Close(_)) => break,
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
    End {
        run_id: String,
        final_status: String,
    },
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
    let response: CancelResponse = client.post(&format!("/runs/{}/cancel", id), &()).await?;

    match format {
        OutputFormat::Table => {
            print_success(&response.message);
            println!(
                "  Status: {} {}",
                status_icon(&response.status),
                format_status(&response.status)
            );
        }
        _ => print_serialized(&response, format)?,
    }
    Ok(())
}

pub async fn retry(client: &ApiClient, id: &str, format: OutputFormat) -> Result<()> {
    let response: RetryResponse = client.post(&format!("/runs/{}/retry", id), &()).await?;

    match format {
        OutputFormat::Table => {
            print_success(&format!(
                "Run retried! New run #{} ({})",
                response.run_number, response.new_run_id
            ));
        }
        _ => print_serialized(&response, format)?,
    }
    Ok(())
}

pub async fn artifacts(client: &ApiClient, id: &str, format: OutputFormat) -> Result<()> {
    let resp: ArtifactListResponse = client.get(&format!("/runs/{}/artifacts", id)).await?;

    match format {
        OutputFormat::Table => {
            if resp.data.is_empty() {
                println!("No artifacts for this run.");
                return Ok(());
            }
            let mut table = build_table(&["Name", "Size", "Type", "Created"]);
            for a in &resp.data {
                table.add_row(vec![
                    Cell::new(&a.name),
                    Cell::new(format_bytes(a.size_bytes)),
                    Cell::new(a.content_type.as_deref().unwrap_or("-")),
                    Cell::new(format_timestamp(&a.created_at)),
                ]);
            }
            print_table(&table);
        }
        _ => print_serialized(&resp, format)?,
    }
    Ok(())
}

fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}
