//! `met pipeline check` — remote pipeline validation (ADR-019).

use crate::OutputFormat;
use crate::api_client::{ApiClient, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Serialize)]
struct CheckRequest {
    definition: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    r#ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    environment: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CheckResponse {
    valid: bool,
    diagnostics: Vec<CheckDiagnostic>,
    summary: CheckSummary,
}

#[derive(Debug, Serialize, Deserialize)]
struct CheckDiagnostic {
    code: String,
    severity: String,
    message: String,
    suggestion: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CheckSummary {
    errors: u32,
    warnings: u32,
    info: u32,
}

pub async fn check(
    client: &ApiClient,
    path: &Path,
    project_slug: &str,
    environment: Option<&str>,
    git_ref: Option<&str>,
    format: OutputFormat,
) -> Result<()> {
    let yaml = tokio::fs::read_to_string(path)
        .await
        .map_err(|e| crate::api_client::ApiError::Other(format!("read error: {e}")))?;

    let body = CheckRequest {
        definition: yaml,
        r#ref: git_ref.map(String::from),
        environment: environment.map(String::from),
    };

    let url = format!("/api/v1/projects/{project_slug}/pipelines/check");
    let resp: CheckResponse = client.post(&url, &body).await?;

    match format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&resp.diagnostics).unwrap_or_default()
            );
        }
        _ => {
            if resp.diagnostics.is_empty() {
                eprintln!("No issues found.");
            } else {
                for d in &resp.diagnostics {
                    let icon = match d.severity.as_str() {
                        "error" => "✗",
                        "warning" => "⚠",
                        _ => "ℹ",
                    };
                    eprintln!("{icon} [{code}] {msg}", code = d.code, msg = d.message);
                    if let Some(s) = &d.suggestion {
                        eprintln!("  → {s}");
                    }
                }
            }
            eprintln!(
                "\n{errors} error(s), {warnings} warning(s), {info} info",
                errors = resp.summary.errors,
                warnings = resp.summary.warnings,
                info = resp.summary.info,
            );
        }
    }

    if !resp.valid {
        std::process::exit(1);
    }

    Ok(())
}
