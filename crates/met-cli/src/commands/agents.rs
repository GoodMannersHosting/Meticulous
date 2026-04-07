use crate::OutputFormat;
use crate::api_client::{ApiClient, Result};
use crate::output::{
    build_table, format_status, format_timestamp, print_kv, print_serialized, print_success,
    print_table, status_icon,
};
use comfy_table::Cell;
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

#[derive(Debug, Serialize, Deserialize)]
pub struct JoinToken {
    pub id: String,
    pub token_prefix: String,
    pub name: Option<String>,
    pub pool: Option<String>,
    pub expires_at: Option<String>,
    pub created_at: String,
    pub used_count: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JoinTokenList {
    pub data: Vec<JoinToken>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateJoinTokenResponse {
    pub id: String,
    pub token: String,
    pub name: Option<String>,
}

pub async fn list(
    client: &ApiClient,
    status_filter: Option<String>,
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
        .get_with_query(
            "/agents",
            &Query {
                status: status_filter,
                pool,
            },
        )
        .await?;

    match format {
        OutputFormat::Table => {
            if response.data.is_empty() {
                println!("No agents found.");
                return Ok(());
            }
            let mut table = build_table(&["Name", "Status", "Pool", "Capacity", "OS/Arch"]);
            for a in &response.data {
                let capacity = format!("{}/{}", a.running_jobs, a.max_jobs);
                let platform = format!("{}/{}", a.os, a.arch);
                table.add_row(vec![
                    Cell::new(&a.name),
                    Cell::new(format!(
                        "{} {}",
                        status_icon(&a.status),
                        format_status(&a.status)
                    )),
                    Cell::new(a.pool.as_deref().unwrap_or("-")),
                    Cell::new(capacity),
                    Cell::new(platform),
                ]);
            }
            print_table(&table);
            println!("\n{} agent(s)", response.pagination.count);
        }
        _ => print_serialized(&response, format)?,
    }
    Ok(())
}

pub async fn info(client: &ApiClient, id: &str, format: OutputFormat) -> Result<()> {
    let agent: Agent = client.get(&format!("/agents/{}", id)).await?;

    match format {
        OutputFormat::Table => {
            println!(
                "Agent: {} — {} {}",
                agent.name,
                status_icon(&agent.status),
                format_status(&agent.status)
            );
            print_kv("ID", &agent.id);
            print_kv("Pool", agent.pool.as_deref().unwrap_or("-"));
            let tags_display = if agent.tags.is_empty() {
                "-".to_string()
            } else {
                agent.tags.join(", ")
            };
            print_kv("Tags", &tags_display);
            print_kv("Platform", &format!("{}/{}", agent.os, agent.arch));
            print_kv("Version", &agent.version);
            print_kv(
                "Capacity",
                &format!("{}/{} jobs", agent.running_jobs, agent.max_jobs),
            );
            if let Some(ref hb) = agent.last_heartbeat_at {
                print_kv("Last Heartbeat", &format_timestamp(hb));
            }
            print_kv("Created", &format_timestamp(&agent.created_at));
        }
        _ => print_serialized(&agent, format)?,
    }
    Ok(())
}

pub async fn revoke(client: &ApiClient, id: &str, format: OutputFormat) -> Result<()> {
    let response: AgentActionResponse = client.post(&format!("/agents/{}/revoke", id), &()).await?;

    match format {
        OutputFormat::Table => {
            print_success(&response.message);
        }
        _ => print_serialized(&response, format)?,
    }
    Ok(())
}

pub async fn join_token_create(
    client: &ApiClient,
    name: Option<&str>,
    pool: Option<&str>,
    expires_in_hours: Option<u32>,
    format: OutputFormat,
) -> Result<()> {
    #[derive(Serialize)]
    struct Request<'a> {
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pool: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        expires_in_hours: Option<u32>,
    }

    let resp: CreateJoinTokenResponse = client
        .post(
            "/agents/join-tokens",
            &Request {
                name,
                pool,
                expires_in_hours,
            },
        )
        .await?;

    match format {
        OutputFormat::Table => {
            print_success("Join token created");
            println!();
            println!("  Token: {}", resp.token);
            println!();
            println!("  Save this token now — it will not be shown again.");
            println!("  Use: met-agent --join-token <TOKEN> --server <URL>");
        }
        _ => print_serialized(&resp, format)?,
    }
    Ok(())
}

pub async fn join_token_list(client: &ApiClient, format: OutputFormat) -> Result<()> {
    let resp: JoinTokenList = client.get("/agents/join-tokens").await?;

    match format {
        OutputFormat::Table => {
            if resp.data.is_empty() {
                println!("No join tokens found.");
                return Ok(());
            }
            let mut table = build_table(&["Name", "Prefix", "Pool", "Used", "Expires"]);
            for t in &resp.data {
                table.add_row(vec![
                    Cell::new(t.name.as_deref().unwrap_or("-")),
                    Cell::new(&t.token_prefix),
                    Cell::new(t.pool.as_deref().unwrap_or("-")),
                    Cell::new(t.used_count.to_string()),
                    Cell::new(
                        t.expires_at
                            .as_deref()
                            .map(format_timestamp)
                            .unwrap_or_else(|| "never".to_string()),
                    ),
                ]);
            }
            print_table(&table);
        }
        _ => print_serialized(&resp, format)?,
    }
    Ok(())
}

pub async fn join_token_revoke(client: &ApiClient, id: &str, format: OutputFormat) -> Result<()> {
    client
        .delete(&format!("/agents/join-tokens/{}", id))
        .await?;

    match format {
        OutputFormat::Table => {
            print_success(&format!("Join token '{}' revoked", id));
        }
        _ => {
            let msg = serde_json::json!({"revoked": id});
            print_serialized(&msg, format)?;
        }
    }
    Ok(())
}
