//! Output formatting for CLI commands.

use crate::api_client::ApiError;
use crate::OutputFormat;
use serde::Serialize;

pub fn print<T: Serialize>(data: &T, format: OutputFormat) -> Result<(), ApiError> {
    match format {
        OutputFormat::Json => {
            let output = serde_json::to_string_pretty(data)
                .map_err(|e| ApiError::InvalidResponse(format!("JSON serialization failed: {}", e)))?;
            println!("{}", output);
        }
        OutputFormat::Yaml => {
            let output = serde_yaml::to_string(data)
                .map_err(|e| ApiError::InvalidResponse(format!("YAML serialization failed: {}", e)))?;
            println!("{}", output);
        }
        OutputFormat::Table => {
            let output = serde_json::to_string_pretty(data)
                .map_err(|e| ApiError::InvalidResponse(format!("JSON serialization failed: {}", e)))?;
            println!("{}", output);
        }
    }
    Ok(())
}

pub fn print_table_header(columns: &[&str]) {
    let header: Vec<String> = columns.iter().map(|c| c.to_uppercase()).collect();
    println!("{}", header.join("\t"));
    println!("{}", "-".repeat(columns.len() * 15));
}

pub fn print_table_row(values: &[&str]) {
    println!("{}", values.join("\t"));
}

pub fn print_success(message: &str) {
    println!("✓ {}", message);
}

pub fn print_error(message: &str) {
    eprintln!("✗ {}", message);
}

pub fn print_info(message: &str) {
    println!("ℹ {}", message);
}

pub fn format_duration(ms: u64) -> String {
    let secs = ms / 1000;
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    }
}

pub fn format_timestamp(ts: &str) -> String {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(ts) {
        dt.format("%Y-%m-%d %H:%M:%S").to_string()
    } else {
        ts.to_string()
    }
}

pub fn status_emoji(status: &str) -> &'static str {
    match status.to_lowercase().as_str() {
        "succeeded" => "✓",
        "failed" => "✗",
        "running" => "⏵",
        "pending" | "queued" => "○",
        "cancelled" => "⊘",
        "timed_out" => "⏱",
        "skipped" => "↷",
        "online" => "●",
        "offline" => "○",
        "busy" => "◐",
        "draining" => "◑",
        _ => "?",
    }
}
