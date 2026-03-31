use crate::api_client::{ApiClient, Result};
use crate::context::ResolvedContext;
use crate::output::{build_table, format_timestamp, print_serialized, print_success, print_table};
use crate::OutputFormat;
use comfy_table::Cell;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Secret {
    pub name: String,
    pub updated_at: String,
    pub created_at: String,
    pub updated_by: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SecretListResponse {
    pub data: Vec<Secret>,
}

pub async fn list(
    client: &ApiClient,
    ctx: &ResolvedContext,
    format: OutputFormat,
) -> Result<()> {
    let org = ctx.require_org()?;
    let project = ctx.require_project()?;

    let resp: SecretListResponse = client
        .get(&format!("/orgs/{}/projects/{}/secrets", org, project))
        .await?;

    match format {
        OutputFormat::Table => {
            if resp.data.is_empty() {
                println!("No secrets found.");
                return Ok(());
            }
            let mut table = build_table(&["Name", "Updated", "Updated By"]);
            for s in &resp.data {
                table.add_row(vec![
                    Cell::new(&s.name),
                    Cell::new(format_timestamp(&s.updated_at)),
                    Cell::new(s.updated_by.as_deref().unwrap_or("-")),
                ]);
            }
            print_table(&table);
        }
        _ => print_serialized(&resp, format)?,
    }
    Ok(())
}

pub async fn set(
    client: &ApiClient,
    ctx: &ResolvedContext,
    name: &str,
    value: &str,
    format: OutputFormat,
) -> Result<()> {
    let org = ctx.require_org()?;
    let project = ctx.require_project()?;

    #[derive(Serialize)]
    struct Request<'a> {
        value: &'a str,
    }

    let _: serde_json::Value = client
        .put(
            &format!("/orgs/{}/projects/{}/secrets/{}", org, project, name),
            &Request { value },
        )
        .await?;

    match format {
        OutputFormat::Table => {
            print_success(&format!("Secret '{}' set", name));
        }
        _ => {
            let msg = serde_json::json!({"name": name, "status": "set"});
            print_serialized(&msg, format)?;
        }
    }
    Ok(())
}

pub async fn delete(
    client: &ApiClient,
    ctx: &ResolvedContext,
    name: &str,
    format: OutputFormat,
) -> Result<()> {
    let org = ctx.require_org()?;
    let project = ctx.require_project()?;

    client
        .delete(&format!(
            "/orgs/{}/projects/{}/secrets/{}",
            org, project, name
        ))
        .await?;

    match format {
        OutputFormat::Table => {
            print_success(&format!("Secret '{}' deleted", name));
        }
        _ => {
            let msg = serde_json::json!({"name": name, "status": "deleted"});
            print_serialized(&msg, format)?;
        }
    }
    Ok(())
}
