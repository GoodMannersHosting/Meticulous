use crate::api_client::{ApiClient, Result};
use crate::context::ResolvedContext;
use crate::output::{
    build_table, format_timestamp, print_kv, print_serialized, print_table,
};
use crate::OutputFormat;
use comfy_table::Cell;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Workflow {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub scope: String,
    pub description: Option<String>,
    pub latest_version: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WorkflowListResponse {
    pub data: Vec<Workflow>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WorkflowVersion {
    pub version: String,
    pub sha: Option<String>,
    pub created_at: String,
    pub changelog: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WorkflowVersionsResponse {
    pub data: Vec<WorkflowVersion>,
}

pub async fn list(
    client: &ApiClient,
    ctx: &ResolvedContext,
    scope: Option<&str>,
    format: OutputFormat,
) -> Result<()> {
    let org = ctx.require_org()?;

    #[derive(Serialize)]
    struct Query<'a> {
        org: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        scope: Option<&'a str>,
    }

    let resp: WorkflowListResponse = client
        .get_with_query("/workflows", &Query { org, scope })
        .await?;

    match format {
        OutputFormat::Table => {
            if resp.data.is_empty() {
                println!("No workflows found.");
                return Ok(());
            }
            let mut table = build_table(&["Name", "Slug", "Scope", "Version"]);
            for w in &resp.data {
                table.add_row(vec![
                    Cell::new(&w.name),
                    Cell::new(&w.slug),
                    Cell::new(&w.scope),
                    Cell::new(w.latest_version.as_deref().unwrap_or("-")),
                ]);
            }
            print_table(&table);
        }
        _ => print_serialized(&resp, format)?,
    }
    Ok(())
}

pub async fn show(client: &ApiClient, slug: &str, format: OutputFormat) -> Result<()> {
    let wf: Workflow = client.get(&format!("/workflows/{}", slug)).await?;

    match format {
        OutputFormat::Table => {
            println!("Workflow: {}", wf.name);
            print_kv("ID", &wf.id);
            print_kv("Slug", &wf.slug);
            print_kv("Scope", &wf.scope);
            print_kv(
                "Description",
                wf.description.as_deref().unwrap_or("-"),
            );
            print_kv(
                "Latest Version",
                wf.latest_version.as_deref().unwrap_or("-"),
            );
            print_kv("Updated", &format_timestamp(&wf.updated_at));
        }
        _ => print_serialized(&wf, format)?,
    }
    Ok(())
}

pub async fn versions(client: &ApiClient, slug: &str, format: OutputFormat) -> Result<()> {
    let resp: WorkflowVersionsResponse = client
        .get(&format!("/workflows/{}/versions", slug))
        .await?;

    match format {
        OutputFormat::Table => {
            if resp.data.is_empty() {
                println!("No versions found for workflow '{}'.", slug);
                return Ok(());
            }
            let mut table = build_table(&["Version", "SHA", "Created", "Changelog"]);
            for v in &resp.data {
                table.add_row(vec![
                    Cell::new(&v.version),
                    Cell::new(v.sha.as_deref().unwrap_or("-")),
                    Cell::new(format_timestamp(&v.created_at)),
                    Cell::new(
                        v.changelog
                            .as_deref()
                            .unwrap_or("-")
                            .chars()
                            .take(50)
                            .collect::<String>(),
                    ),
                ]);
            }
            print_table(&table);
        }
        _ => print_serialized(&resp, format)?,
    }
    Ok(())
}
