use crate::api_client::{ApiClient, Result};
use crate::config::CliConfig;
use crate::context::ResolvedContext;
use crate::output::{build_table, print_kv, print_serialized, print_success, print_table};
use crate::OutputFormat;
use comfy_table::Cell;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub org_id: String,
    pub repo_url: Option<String>,
    pub default_branch: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProjectListResponse {
    pub data: Vec<Project>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateProjectResponse {
    pub id: String,
    pub name: String,
    pub slug: String,
}

pub async fn list(
    client: &ApiClient,
    ctx: &ResolvedContext,
    format: OutputFormat,
) -> Result<()> {
    let org = ctx.require_org()?;

    #[derive(Serialize)]
    struct Query<'a> {
        org: &'a str,
    }

    let resp: ProjectListResponse = client
        .get_with_query("/projects", &Query { org })
        .await?;

    match format {
        OutputFormat::Table => {
            if resp.data.is_empty() {
                println!("No projects found in organization '{}'.", org);
                return Ok(());
            }
            let mut table = build_table(&["Name", "Slug", "Repo", "Branch"]);
            for p in &resp.data {
                table.add_row(vec![
                    Cell::new(&p.name),
                    Cell::new(&p.slug),
                    Cell::new(p.repo_url.as_deref().unwrap_or("-")),
                    Cell::new(p.default_branch.as_deref().unwrap_or("-")),
                ]);
            }
            print_table(&table);
        }
        _ => print_serialized(&resp, format)?,
    }
    Ok(())
}

pub async fn create(
    client: &ApiClient,
    ctx: &ResolvedContext,
    name: &str,
    description: Option<&str>,
    repo_url: Option<&str>,
    format: OutputFormat,
) -> Result<()> {
    let org = ctx.require_org()?;

    #[derive(Serialize)]
    struct Request<'a> {
        org: &'a str,
        name: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        repo_url: Option<&'a str>,
    }

    let resp: CreateProjectResponse = client
        .post(
            "/projects",
            &Request {
                org,
                name,
                description,
                repo_url,
            },
        )
        .await?;

    match format {
        OutputFormat::Table => {
            print_success(&format!("Project '{}' created ({})", resp.name, resp.slug));
        }
        _ => print_serialized(&resp, format)?,
    }
    Ok(())
}

pub async fn info(
    client: &ApiClient,
    ctx: &ResolvedContext,
    slug: &str,
    format: OutputFormat,
) -> Result<()> {
    let org = ctx.require_org()?;
    let project: Project = client
        .get(&format!("/orgs/{}/projects/{}", org, slug))
        .await?;

    match format {
        OutputFormat::Table => {
            println!("Project: {}", project.name);
            print_kv("ID", &project.id);
            print_kv("Slug", &project.slug);
            print_kv(
                "Description",
                project.description.as_deref().unwrap_or("-"),
            );
            print_kv("Repo", project.repo_url.as_deref().unwrap_or("-"));
            print_kv(
                "Default Branch",
                project.default_branch.as_deref().unwrap_or("main"),
            );
            print_kv("Created", &project.created_at);
        }
        _ => print_serialized(&project, format)?,
    }
    Ok(())
}

pub async fn switch(project_slug: &str) -> Result<()> {
    let mut config = CliConfig::load();
    config.context.project = Some(project_slug.to_string());
    config
        .save_global()
        .map_err(|e| crate::api_client::ApiError::Other(format!("Failed to save config: {}", e)))?;
    print_success(&format!("Switched to project '{}'", project_slug));
    Ok(())
}
