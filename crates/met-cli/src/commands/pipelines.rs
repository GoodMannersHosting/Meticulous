use crate::api_client::{ApiClient, Result};
use crate::context::ResolvedContext;
use crate::output::{
    build_table, format_status, print_kv, print_serialized, print_success, print_table,
    status_icon,
};
use crate::OutputFormat;
use comfy_table::Cell;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize)]
pub struct Pipeline {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub enabled: bool,
    pub last_run_status: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PipelineListResponse {
    pub data: Vec<Pipeline>,
    pub pagination: Pagination,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Pagination {
    pub has_more: bool,
    pub count: usize,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TriggerResponse {
    pub run_id: String,
    pub run_number: i64,
    pub status: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PipelineDiff {
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub modified: Vec<String>,
}

pub async fn list(
    client: &ApiClient,
    ctx: &ResolvedContext,
    format: OutputFormat,
) -> Result<()> {
    let project = ctx.require_project()?;

    #[derive(Serialize)]
    struct Query<'a> {
        project_id: &'a str,
    }

    let response: PipelineListResponse = client
        .get_with_query("/pipelines", &Query { project_id: project })
        .await?;

    match format {
        OutputFormat::Table => {
            if response.data.is_empty() {
                println!("No pipelines found.");
                return Ok(());
            }
            let mut table = build_table(&["Name", "Slug", "Enabled", "Last Run"]);
            for p in &response.data {
                let last_status = p
                    .last_run_status
                    .as_deref()
                    .map(|s| format!("{} {}", status_icon(s), format_status(s)))
                    .unwrap_or_else(|| "-".to_string());
                table.add_row(vec![
                    Cell::new(&p.name),
                    Cell::new(&p.slug),
                    Cell::new(if p.enabled { "yes" } else { "no" }),
                    Cell::new(last_status),
                ]);
            }
            print_table(&table);
            println!("\n{} pipeline(s)", response.pagination.count);
        }
        _ => print_serialized(&response, format)?,
    }
    Ok(())
}

pub async fn show(client: &ApiClient, id: &str, format: OutputFormat) -> Result<()> {
    let pipeline: Pipeline = client.get(&format!("/pipelines/{}", id)).await?;

    match format {
        OutputFormat::Table => {
            println!("Pipeline: {}", pipeline.name);
            print_kv("ID", &pipeline.id);
            print_kv("Slug", &pipeline.slug);
            print_kv(
                "Description",
                pipeline.description.as_deref().unwrap_or("-"),
            );
            print_kv(
                "Enabled",
                if pipeline.enabled { "yes" } else { "no" },
            );
            print_kv("Created", &pipeline.created_at);
            print_kv("Updated", &pipeline.updated_at);
        }
        _ => print_serialized(&pipeline, format)?,
    }
    Ok(())
}

pub async fn validate(path: &std::path::Path, format: OutputFormat) -> Result<()> {
    use met_parser::{MockWorkflowProvider, PipelineParser};

    let path_str = path.display().to_string();
    if !path.exists() {
        return Err(crate::api_client::ApiError::Other(format!(
            "File not found: {}",
            path_str
        )));
    }

    let yaml = std::fs::read_to_string(path).map_err(|e| {
        crate::api_client::ApiError::Other(format!("Failed to read file: {}", e))
    })?;

    let provider = MockWorkflowProvider::new();
    let parser = PipelineParser::new(&provider);

    #[derive(Serialize)]
    struct ValidationResult {
        valid: bool,
        path: String,
        errors: Vec<String>,
    }

    let result = match parser.parse(&yaml).await {
        Ok(_) => ValidationResult {
            valid: true,
            path: path_str.clone(),
            errors: Vec::new(),
        },
        Err(errors) => ValidationResult {
            valid: false,
            path: path_str.clone(),
            errors: errors.iter().map(|e| e.to_string()).collect(),
        },
    };

    match format {
        OutputFormat::Table => {
            if result.valid {
                print_success(&format!("Pipeline configuration is valid: {}", path_str));
            } else {
                crate::output::print_error(&format!(
                    "Pipeline configuration is invalid: {}",
                    path_str
                ));
                for error in &result.errors {
                    println!("  - {}", error);
                }
            }
        }
        _ => print_serialized(&result, format)?,
    }
    Ok(())
}

pub async fn trigger(
    client: &ApiClient,
    id: &str,
    branch: Option<String>,
    commit: Option<String>,
    variables: Vec<(String, String)>,
    format: OutputFormat,
) -> Result<()> {
    #[derive(Serialize)]
    struct TriggerRequest {
        branch: Option<String>,
        commit_sha: Option<String>,
        variables: Option<HashMap<String, String>>,
    }

    let vars = if variables.is_empty() {
        None
    } else {
        Some(variables.into_iter().collect())
    };

    let response: TriggerResponse = client
        .post(
            &format!("/pipelines/{}/trigger", id),
            &TriggerRequest {
                branch,
                commit_sha: commit,
                variables: vars,
            },
        )
        .await?;

    match format {
        OutputFormat::Table => {
            print_success(&format!(
                "Pipeline triggered! Run #{} ({})",
                response.run_number, response.run_id
            ));
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

pub async fn diff(
    client: &ApiClient,
    id: &str,
    base: Option<&str>,
    format: OutputFormat,
) -> Result<()> {
    #[derive(Serialize)]
    struct Query<'a> {
        #[serde(skip_serializing_if = "Option::is_none")]
        base: Option<&'a str>,
    }

    let resp: PipelineDiff = client
        .get_with_query(&format!("/pipelines/{}/diff", id), &Query { base })
        .await?;

    match format {
        OutputFormat::Table => {
            if resp.added.is_empty() && resp.removed.is_empty() && resp.modified.is_empty() {
                println!("No changes detected.");
                return Ok(());
            }
            for name in &resp.added {
                println!("  + {}", name);
            }
            for name in &resp.removed {
                println!("  - {}", name);
            }
            for name in &resp.modified {
                println!("  ~ {}", name);
            }
        }
        _ => print_serialized(&resp, format)?,
    }
    Ok(())
}
