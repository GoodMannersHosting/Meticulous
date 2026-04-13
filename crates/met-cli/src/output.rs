use crate::OutputFormat;
use crate::api_client::ApiError;
use colored::Colorize;
use comfy_table::{Cell, ContentArrangement, Table, presets::UTF8_FULL_CONDENSED};
use indicatif::{ProgressBar, ProgressStyle};
use serde::Serialize;
use std::time::Duration;

pub fn print_serialized<T: Serialize>(data: &T, format: OutputFormat) -> Result<(), ApiError> {
    match format {
        OutputFormat::Json => {
            let output = serde_json::to_string_pretty(data)
                .map_err(|e| ApiError::InvalidResponse(format!("JSON error: {}", e)))?;
            println!("{}", output);
        }
        OutputFormat::Yaml => {
            let output = serde_yaml::to_string(data)
                .map_err(|e| ApiError::InvalidResponse(format!("YAML error: {}", e)))?;
            print!("{}", output);
        }
        OutputFormat::Table => {
            let output = serde_json::to_string_pretty(data)
                .map_err(|e| ApiError::InvalidResponse(format!("JSON error: {}", e)))?;
            println!("{}", output);
        }
    }
    Ok(())
}

pub fn build_table(headers: &[&str]) -> Table {
    let mut table = Table::new();
    table.load_preset(UTF8_FULL_CONDENSED);
    table.set_content_arrangement(ContentArrangement::Dynamic);
    table.set_header(headers.iter().map(|h| Cell::new(h.to_uppercase())));
    table
}

pub fn print_table(table: &Table) {
    println!("{table}");
}

pub fn print_kv(label: &str, value: &str) {
    println!("  {:<16}{}", format!("{}:", label).dimmed(), value);
}

pub fn print_success(message: &str) {
    println!("{} {}", "✓".green().bold(), message);
}

pub fn print_error(message: &str) {
    eprintln!("{} {}", "✗".red().bold(), message);
}

pub fn print_warning(message: &str) {
    eprintln!("{} {}", "!".yellow().bold(), message);
}

pub fn print_info(message: &str) {
    println!("{} {}", "ℹ".blue(), message);
}

#[allow(dead_code)]
pub fn spinner(message: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
            .template("{spinner} {msg}")
            .expect("invalid spinner template"),
    );
    pb.set_message(message.to_string());
    pb.enable_steady_tick(Duration::from_millis(100));
    pb
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

pub fn format_status(status: &str) -> String {
    let s = status.to_lowercase();
    match s.as_str() {
        "succeeded" | "success" | "completed" => format!("{}", status.green()),
        "failed" | "error" => format!("{}", status.red()),
        "running" | "in_progress" => format!("{}", status.cyan()),
        "pending" | "queued" | "waiting" => format!("{}", status.yellow()),
        "cancelled" | "canceled" => format!("{}", status.dimmed()),
        "timed_out" => format!("{}", status.red().dimmed()),
        "skipped" => format!("{}", status.dimmed()),
        "online" => format!("{}", status.green()),
        "offline" => format!("{}", status.red().dimmed()),
        "busy" => format!("{}", status.yellow()),
        "draining" => format!("{}", status.yellow().dimmed()),
        _ => status.to_string(),
    }
}

pub fn status_icon(status: &str) -> &'static str {
    match status.to_lowercase().as_str() {
        "succeeded" | "success" | "completed" => "✓",
        "failed" | "error" => "✗",
        "running" | "in_progress" => "⏵",
        "pending" | "queued" | "waiting" => "○",
        "cancelled" | "canceled" => "⊘",
        "timed_out" => "⏱",
        "skipped" => "↷",
        "online" => "●",
        "offline" => "○",
        "busy" => "◐",
        "draining" => "◑",
        _ => "?",
    }
}
