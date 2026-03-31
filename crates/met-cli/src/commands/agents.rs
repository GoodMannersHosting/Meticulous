//! Agent command handlers.

use crate::api_client::{ApiClient, Result};
use crate::output::{print, print_success, print_table_header, print_table_row, status_emoji};
use crate::OutputFormat;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Agent {
    pub id: String,
    pub name: String,
    pub status: String,
    pub pool: Option<String>,
    pub tags: Vec<String>,
    pub os: String,
    pub arch: String,
    pub version: String,
    pub max_jobs: i32,
    pub running_jobs: i32,
    pub available_capacity: i32,
    pub last_heartbeat_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AgentListResponse {
    pub data: Vec<Agent>,
    pub pagination: Pagination,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Pagination {
    pub has_more: bool,
    pub count: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AgentActionResponse {
    pub agent_id: String,
    pub status: String,
    pub message: String,
}

pub async fn list(
    client: &ApiClient,
    status: Option<String>,
    pool: Option<String>,
    format: OutputFormat,
) -> Result<()> {
    #[derive(Serialize)]
    struct Query {
        #[serde(skip_serializing_if = "Option::is_none")]
        status: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pool: Option<String>,
    }

    let response: AgentListResponse = client
        .get_with_query("/agents", &Query { status, pool })
        .await?;

    match format {
        OutputFormat::Table => {
            print_table_header(&["NAME", "STATUS", "POOL", "CAPACITY", "OS/ARCH"]);
            for a in &response.data {
                let capacity = format!("{}/{}", a.running_jobs, a.max_jobs);
                let platform = format!("{}/{}", a.os, a.arch);
                print_table_row(&[
                    &a.name,
                    &format!("{} {}", status_emoji(&a.status), a.status),
                    a.pool.as_deref().unwrap_or("-"),
                    &capacity,
                    &platform,
                ]);
            }
            println!("\nShowing {} agent(s)", response.pagination.count);
        }
        _ => {
            print(&response, format)?;
        }
    }

    Ok(())
}

pub async fn show(client: &ApiClient, id: &str, format: OutputFormat) -> Result<()> {
    let agent: Agent = client.get(&format!("/agents/{}", id)).await?;

    match format {
        OutputFormat::Table => {
            println!("Agent: {}", agent.name);
            println!("  ID:           {}", agent.id);
            println!("  Status:       {} {}", status_emoji(&agent.status), agent.status);
            println!("  Pool:         {}", agent.pool.as_deref().unwrap_or("-"));
            println!("  Tags:         {}", if agent.tags.is_empty() { "-".to_string() } else { agent.tags.join(", ") });
            println!("  Platform:     {}/{}", agent.os, agent.arch);
            println!("  Version:      {}", agent.version);
            println!("  Capacity:     {}/{} jobs", agent.running_jobs, agent.max_jobs);
            if let Some(ref hb) = agent.last_heartbeat_at {
                println!("  Last Heartbeat: {}", hb);
            }
        }
        _ => {
            print(&agent, format)?;
        }
    }

    Ok(())
}

pub async fn drain(client: &ApiClient, id: &str, format: OutputFormat) -> Result<()> {
    let response: AgentActionResponse = client
        .post(&format!("/agents/{}/drain", id), &())
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

pub async fn resume(client: &ApiClient, id: &str, format: OutputFormat) -> Result<()> {
    let response: AgentActionResponse = client
        .post(&format!("/agents/{}/resume", id), &())
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
