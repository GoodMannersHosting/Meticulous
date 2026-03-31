use crate::api_client::{ApiClient, Result};
use crate::context::ResolvedContext;
use crate::output::{build_table, print_serialized, print_success, print_table};
use crate::OutputFormat;
use comfy_table::Cell;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Variable {
    pub name: String,
    pub value: String,
    pub is_sensitive: bool,
    pub scope: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct VariableListResponse {
    pub data: Vec<Variable>,
}

pub async fn list(
    client: &ApiClient,
    ctx: &ResolvedContext,
    format: OutputFormat,
) -> Result<()> {
    let org = ctx.require_org()?;
    let project = ctx.require_project()?;

    let resp: VariableListResponse = client
        .get(&format!("/orgs/{}/projects/{}/variables", org, project))
        .await?;

    match format {
        OutputFormat::Table => {
            if resp.data.is_empty() {
                println!("No variables found.");
                return Ok(());
            }
            let mut table = build_table(&["Name", "Value", "Sensitive", "Scope"]);
            for v in &resp.data {
                let display_value = if v.is_sensitive {
                    "********".to_string()
                } else {
                    v.value.clone()
                };
                table.add_row(vec![
                    Cell::new(&v.name),
                    Cell::new(display_value),
                    Cell::new(if v.is_sensitive { "yes" } else { "no" }),
                    Cell::new(&v.scope),
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
    sensitive: bool,
    format: OutputFormat,
) -> Result<()> {
    let org = ctx.require_org()?;
    let project = ctx.require_project()?;

    #[derive(Serialize)]
    struct Request<'a> {
        value: &'a str,
        is_sensitive: bool,
    }

    let _: serde_json::Value = client
        .put(
            &format!("/orgs/{}/projects/{}/variables/{}", org, project, name),
            &Request {
                value,
                is_sensitive: sensitive,
            },
        )
        .await?;

    match format {
        OutputFormat::Table => {
            print_success(&format!("Variable '{}' set", name));
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
            "/orgs/{}/projects/{}/variables/{}",
            org, project, name
        ))
        .await?;

    match format {
        OutputFormat::Table => {
            print_success(&format!("Variable '{}' deleted", name));
        }
        _ => {
            let msg = serde_json::json!({"name": name, "status": "deleted"});
            print_serialized(&msg, format)?;
        }
    }
    Ok(())
}
