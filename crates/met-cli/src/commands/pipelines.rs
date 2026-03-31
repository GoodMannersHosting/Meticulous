//! Pipeline command handlers.

use crate::api_client::{ApiClient, Result};
use crate::output::{print, print_success, print_table_header, print_table_row, status_emoji};
use crate::OutputFormat;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize)]
pub struct Pipeline {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub enabled: bool,
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

pub async fn list(client: &ApiClient, project_id: &str, format: OutputFormat) -> Result<()> {
    #[derive(Serialize)]
    struct Query<'a> {
        project_id: &'a str,
    }

    let response: PipelineListResponse = client
        .get_with_query("/pipelines", &Query { project_id })
        .await?;

    match format {
        OutputFormat::Table => {
            print_table_header(&["ID", "NAME", "SLUG", "ENABLED"]);
            for p in &response.data {
                print_table_row(&[
                    &p.id,
                    &p.name,
                    &p.slug,
                    if p.enabled { "yes" } else { "no" },
                ]);
            }
            println!("\nShowing {} pipeline(s)", response.pagination.count);
        }
        _ => {
            print(&response, format)?;
        }
    }

    Ok(())
}

pub async fn show(client: &ApiClient, id: &str, format: OutputFormat) -> Result<()> {
    let pipeline: Pipeline = client.get(&format!("/pipelines/{}", id)).await?;

    match format {
        OutputFormat::Table => {
            println!("Pipeline: {}", pipeline.name);
            println!("  ID:          {}", pipeline.id);
            println!("  Slug:        {}", pipeline.slug);
            println!("  Description: {}", pipeline.description.as_deref().unwrap_or("-"));
            println!("  Enabled:     {}", if pipeline.enabled { "yes" } else { "no" });
            println!("  Created:     {}", pipeline.created_at);
        }
        _ => {
            print(&pipeline, format)?;
        }
    }

    Ok(())
}

pub async fn trigger(
    client: &ApiClient,
    id: &str,
    branch: Option<String>,
    commit: Option<String>,
    format: OutputFormat,
) -> Result<()> {
    #[derive(Serialize)]
    struct TriggerRequest {
        branch: Option<String>,
        commit_sha: Option<String>,
        variables: Option<HashMap<String, String>>,
    }

    let response: TriggerResponse = client
        .post(
            &format!("/pipelines/{}/trigger", id),
            &TriggerRequest {
                branch,
                commit_sha: commit,
                variables: None,
            },
        )
        .await?;

    match format {
        OutputFormat::Table => {
            print_success(&format!(
                "Pipeline triggered! Run #{} ({})",
                response.run_number, response.run_id
            ));
            println!("Status: {} {}", status_emoji(&response.status), response.status);
        }
        _ => {
            print(&response, format)?;
        }
    }

    Ok(())
}
