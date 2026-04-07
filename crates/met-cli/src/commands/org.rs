use crate::OutputFormat;
use crate::api_client::{ApiClient, Result};
use crate::config::CliConfig;
use crate::output::{build_table, print_kv, print_serialized, print_success, print_table};
use comfy_table::Cell;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Organization {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub plan: Option<String>,
    pub member_count: Option<i64>,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OrgListResponse {
    pub data: Vec<Organization>,
}

pub async fn list(client: &ApiClient, format: OutputFormat) -> Result<()> {
    let resp: OrgListResponse = client.get("/orgs").await?;

    match format {
        OutputFormat::Table => {
            if resp.data.is_empty() {
                println!("No organizations found.");
                return Ok(());
            }
            let mut table = build_table(&["Name", "Slug", "Plan", "Members"]);
            for o in &resp.data {
                table.add_row(vec![
                    Cell::new(&o.name),
                    Cell::new(&o.slug),
                    Cell::new(o.plan.as_deref().unwrap_or("-")),
                    Cell::new(
                        o.member_count
                            .map(|c| c.to_string())
                            .unwrap_or_else(|| "-".into()),
                    ),
                ]);
            }
            print_table(&table);
        }
        _ => print_serialized(&resp, format)?,
    }
    Ok(())
}

pub async fn switch(org_slug: &str) -> Result<()> {
    let mut config = CliConfig::load();
    config.context.org = Some(org_slug.to_string());
    config.context.project = None;
    config
        .save_global()
        .map_err(|e| crate::api_client::ApiError::Other(format!("Failed to save config: {}", e)))?;
    print_success(&format!("Switched to organization '{}'", org_slug));
    println!("  Project context has been cleared. Use `met project switch` to select a project.");
    Ok(())
}

pub async fn info(client: &ApiClient, org_slug: &str, format: OutputFormat) -> Result<()> {
    let org: Organization = client.get(&format!("/orgs/{}", org_slug)).await?;

    match format {
        OutputFormat::Table => {
            println!("Organization: {}", org.name);
            print_kv("ID", &org.id);
            print_kv("Slug", &org.slug);
            print_kv("Plan", org.plan.as_deref().unwrap_or("-"));
            if let Some(count) = org.member_count {
                print_kv("Members", &count.to_string());
            }
            print_kv("Created", &org.created_at);
        }
        _ => print_serialized(&org, format)?,
    }
    Ok(())
}
