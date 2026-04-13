//! CI mode bootstrap: creates a known admin user + service account, seeds fake but plausible
//! data (projects, pipelines, runs), and emits the service-account API token to stdout so the
//! calling pipeline step can capture it with `met-output`.
//!
//! Enabled via `MET_CI_MODE=true`. NEVER use in production — the credentials and data are
//! intentionally predictable.

use met_core::ids::{OrganizationId, UserId};
use met_core::models::{
    CreateApiToken, CreateOrganization, CreatePipeline, CreateProject, OwnerType,
    ResourceVisibility,
};
use met_store::PgPool;
use met_store::repos::{ApiTokenRepo, OrganizationRepo, PipelineRepo, ProjectRepo, UserRepo};
use serde_json::json;

use crate::auth::{generate_token, hash_password};

const CI_ORG_NAME: &str = "Meticulous CI";
const CI_ORG_SLUG: &str = "meticulous-ci";
const CI_ADMIN_USERNAME: &str = "admin";
const CI_ADMIN_EMAIL: &str = "admin@ci.meticulous.local";
const CI_SA_USERNAME: &str = "ci-service-account";
const CI_SA_EMAIL: &str = "ci-sa@ci.meticulous.local";
const DEFAULT_CI_PASSWORD: &str = "ci-bootstrap";

/// Run the CI bootstrap. Idempotent: skips if org already exists.
///
/// Prints the plain service-account API token to stdout (prefixed with `MET_CI_TOKEN=`) so
/// agent pipeline steps can capture it via `met-output var`.
pub async fn run(db: &PgPool, password: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    let org_repo = OrganizationRepo::new(db);
    let user_repo = UserRepo::new(db);

    // Idempotency: if any org exists already, skip.
    let existing = org_repo.list(1, 0).await?;
    if !existing.is_empty() {
        tracing::info!("CI bootstrap: org already exists, skipping");
        return Ok(());
    }

    tracing::warn!("CI MODE ACTIVE — creating bootstrap org/users/data. DO NOT run in production.");

    // Create org
    let org = org_repo
        .create(&CreateOrganization {
            name: CI_ORG_NAME.to_string(),
            slug: CI_ORG_SLUG.to_string(),
        })
        .await?;
    let org_id = org.id;

    // Create admin user
    let admin_password = password.unwrap_or(DEFAULT_CI_PASSWORD);
    let admin_hash = hash_password(admin_password)?;
    let admin = user_repo
        .create(
            org_id,
            CI_ADMIN_USERNAME,
            CI_ADMIN_EMAIL,
            Some("CI Admin"),
            Some(&admin_hash),
            true,  // is_admin
            false, // service_account
            false, // password_must_change
        )
        .await?;
    tracing::info!(user_id = %admin.id, "CI bootstrap: admin user created");

    // Create service account
    let sa = user_repo
        .create(
            org_id,
            CI_SA_USERNAME,
            CI_SA_EMAIL,
            Some("CI Service Account"),
            None,  // no password
            false, // is_admin
            true,  // service_account
            false,
        )
        .await?;
    tracing::info!(user_id = %sa.id, "CI bootstrap: service account created");

    // Mint a long-lived API token for the service account
    let (plain_token, prefix, token_hash) = generate_token();
    let token_repo = ApiTokenRepo::new(db);
    let _token = token_repo
        .create(
            sa.id,
            &CreateApiToken {
                name: "ci-pipeline-token".to_string(),
                description: Some("Auto-created by CI bootstrap".to_string()),
                scopes: vec!["*".to_string()],
                project_ids: None,
                pipeline_ids: None,
                expires_in: None,
            },
            &token_hash,
            &prefix,
        )
        .await?;

    // Emit for pipeline capture
    println!("MET_CI_TOKEN={plain_token}");
    println!("MET_CI_ADMIN_PASSWORD={admin_password}");

    // Seed plausible fake data
    seed_data(db, org_id, admin.id).await?;

    tracing::info!("CI bootstrap complete");
    Ok(())
}

/// Seed fake-but-plausible projects and pipelines for UI testing.
async fn seed_data(
    db: &PgPool,
    org_id: OrganizationId,
    owner_id: UserId,
) -> Result<(), Box<dyn std::error::Error>> {
    let project_repo = ProjectRepo::new(db);
    let pipeline_repo = PipelineRepo::new(db);

    let seed_projects = [
        (
            "Platform Services",
            "platform-services",
            "Core infrastructure services",
        ),
        (
            "Data Pipeline",
            "data-pipeline",
            "ETL and streaming data jobs",
        ),
        (
            "Frontend",
            "frontend",
            "Web application builds and deployments",
        ),
    ];

    let simple_def = json!({
        "name": "placeholder",
        "triggers": {"manual": {}},
        "workflows": []
    });

    for (name, slug, description) in &seed_projects {
        let project = project_repo
            .create(
                org_id,
                &CreateProject {
                    name: name.to_string(),
                    slug: slug.to_string(),
                    description: Some(description.to_string()),
                    owner_type: OwnerType::User,
                    owner_id: owner_id.to_string(),
                    visibility: ResourceVisibility::Authenticated,
                },
            )
            .await?;

        // Create two pipelines per project
        for (pipeline_name, pipeline_slug) in &[
            (format!("{name} CI"), format!("{slug}-ci")),
            (format!("{name} Deploy"), format!("{slug}-deploy")),
        ] {
            let mut def = simple_def.clone();
            def["name"] = json!(pipeline_name);
            pipeline_repo
                .create(
                    project.id,
                    &CreatePipeline {
                        name: pipeline_name.clone(),
                        slug: pipeline_slug.clone(),
                        description: Some(format!("Seeded pipeline for {name}")),
                        definition: def,
                        definition_path: None,
                        scm_provider: None,
                        scm_repository: None,
                        scm_ref: None,
                        scm_path: None,
                        scm_credentials_secret_path: None,
                        scm_revision: None,
                        visibility: ResourceVisibility::Authenticated,
                    },
                )
                .await?;
        }

        tracing::debug!(project_id = %project.id, project = %name, "CI seeded project");
    }

    Ok(())
}
