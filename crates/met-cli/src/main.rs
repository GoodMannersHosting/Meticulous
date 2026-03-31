use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod api_client;
mod auth;
mod commands;
mod config;
mod context;
mod output;

use api_client::ApiClient;
use commands::{agents, auth as auth_cmd, config as config_cmd, debug, org, pipelines, project, runs, secret, variable, workflow};
use config::CliConfig;
use context::ResolvedContext;

#[derive(Parser)]
#[command(name = "met")]
#[command(about = "Meticulous CI/CD command-line interface")]
#[command(version, propagate_version = true)]
struct Cli {
    /// API server URL
    #[arg(long, env = "MET_SERVER_URL", global = true)]
    server: Option<String>,

    /// API token for authentication
    #[arg(long, env = "MET_API_TOKEN", global = true)]
    token: Option<String>,

    /// Organization slug
    #[arg(long, env = "MET_ORG", global = true)]
    org: Option<String>,

    /// Project slug
    #[arg(long, env = "MET_PROJECT", global = true)]
    project: Option<String>,

    /// Output format
    #[arg(long, default_value = "table", global = true)]
    format: OutputFormat,

    /// Enable verbose output (request debugging)
    #[arg(short, long, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum OutputFormat {
    Table,
    Json,
    Yaml,
}

// ─── Top-level commands ──────────────────────────────────────────────

#[derive(Subcommand)]
enum Commands {
    /// Authentication and token management
    Auth {
        #[command(subcommand)]
        action: AuthCommands,
    },
    /// Organization management
    Org {
        #[command(subcommand)]
        action: OrgCommands,
    },
    /// Project management
    Project {
        #[command(subcommand)]
        action: ProjectCommands,
    },
    /// Pipeline operations
    Pipeline {
        #[command(subcommand)]
        action: PipelineCommands,
    },
    /// Run operations
    Run {
        #[command(subcommand)]
        action: RunCommands,
    },
    /// Secret management
    Secret {
        #[command(subcommand)]
        action: SecretCommands,
    },
    /// Variable management
    Variable {
        #[command(subcommand)]
        action: VariableCommands,
    },
    /// Reusable workflow operations
    Workflow {
        #[command(subcommand)]
        action: WorkflowCommands,
    },
    /// Agent management
    Agent {
        #[command(subcommand)]
        action: AgentCommands,
    },
    /// Local debugging tools
    Debug {
        #[command(subcommand)]
        action: DebugCommands,
    },
    /// CLI configuration
    Config {
        #[command(subcommand)]
        action: ConfigCommands,
    },
}

// ─── Auth ────────────────────────────────────────────────────────────

#[derive(Subcommand)]
enum AuthCommands {
    /// Login via browser (OIDC)
    Login,
    /// Clear stored credentials
    Logout,
    /// Show current authentication status
    Status,
    /// API token management
    Token {
        #[command(subcommand)]
        action: TokenCommands,
    },
}

#[derive(Subcommand)]
enum TokenCommands {
    /// Create a new API token
    Create {
        /// Token name
        #[arg(short, long)]
        name: String,
        /// Token description
        #[arg(short, long)]
        description: Option<String>,
        /// Token scopes (comma-separated)
        #[arg(short, long, value_delimiter = ',', default_value = "read")]
        scopes: Vec<String>,
        /// Expiration in days
        #[arg(short, long)]
        expires_in_days: Option<u32>,
    },
    /// List API tokens
    List,
    /// Revoke an API token
    Revoke {
        /// Token ID
        id: String,
    },
}

// ─── Org ─────────────────────────────────────────────────────────────

#[derive(Subcommand)]
enum OrgCommands {
    /// List organizations
    List,
    /// Switch active organization
    Switch {
        /// Organization slug
        slug: String,
    },
    /// Show organization details
    Info {
        /// Organization slug (uses current if omitted)
        slug: Option<String>,
    },
}

// ─── Project ─────────────────────────────────────────────────────────

#[derive(Subcommand)]
enum ProjectCommands {
    /// List projects in the current organization
    List,
    /// Create a new project
    Create {
        /// Project name
        name: String,
        /// Project description
        #[arg(short, long)]
        description: Option<String>,
        /// Repository URL
        #[arg(short, long)]
        repo_url: Option<String>,
    },
    /// Show project details
    Info {
        /// Project slug (uses current if omitted)
        slug: Option<String>,
    },
    /// Switch active project
    Switch {
        /// Project slug
        slug: String,
    },
}

// ─── Pipeline ────────────────────────────────────────────────────────

#[derive(Subcommand)]
enum PipelineCommands {
    /// List pipelines
    List,
    /// Show pipeline details
    Show {
        /// Pipeline ID or slug
        id: String,
    },
    /// Validate a pipeline file
    Validate {
        /// Path to pipeline file
        path: PathBuf,
    },
    /// Trigger a pipeline run
    Trigger {
        /// Pipeline ID
        id: String,
        /// Branch to run
        #[arg(short, long)]
        branch: Option<String>,
        /// Commit SHA
        #[arg(long)]
        commit: Option<String>,
        /// Variables (KEY=VALUE)
        #[arg(short = 'V', long = "var")]
        variables: Vec<String>,
    },
    /// Show diff of pipeline configuration
    Diff {
        /// Pipeline ID
        id: String,
        /// Base ref to compare against
        #[arg(long)]
        base: Option<String>,
    },
}

// ─── Run ─────────────────────────────────────────────────────────────

#[derive(Subcommand)]
enum RunCommands {
    /// List runs
    List {
        /// Pipeline ID
        #[arg(short, long)]
        pipeline: String,
        /// Maximum number of runs to show
        #[arg(short, long, default_value = "10")]
        limit: u32,
    },
    /// Show run status
    Status {
        /// Run ID
        id: String,
    },
    /// Stream run logs
    Logs {
        /// Run ID
        id: String,
        /// Follow log output
        #[arg(short, long)]
        follow: bool,
        /// Filter by job name
        #[arg(short, long)]
        job: Option<String>,
        /// Filter by step name
        #[arg(short, long)]
        step: Option<String>,
        /// Number of lines to show from the end
        #[arg(short, long)]
        tail: Option<u32>,
    },
    /// Cancel a running pipeline
    Cancel {
        /// Run ID
        id: String,
    },
    /// Retry a failed run
    Retry {
        /// Run ID
        id: String,
    },
    /// List run artifacts
    Artifacts {
        /// Run ID
        id: String,
    },
}

// ─── Secret ──────────────────────────────────────────────────────────

#[derive(Subcommand)]
enum SecretCommands {
    /// List secrets
    List,
    /// Set a secret
    Set {
        /// Secret name
        name: String,
        /// Secret value (omit to read from stdin)
        value: Option<String>,
    },
    /// Delete a secret
    Delete {
        /// Secret name
        name: String,
    },
}

// ─── Variable ────────────────────────────────────────────────────────

#[derive(Subcommand)]
enum VariableCommands {
    /// List variables
    List,
    /// Set a variable
    Set {
        /// Variable name
        name: String,
        /// Variable value
        value: String,
        /// Mark as sensitive (value hidden in logs)
        #[arg(short, long)]
        sensitive: bool,
    },
    /// Delete a variable
    Delete {
        /// Variable name
        name: String,
    },
}

// ─── Workflow ────────────────────────────────────────────────────────

#[derive(Subcommand)]
enum WorkflowCommands {
    /// List workflows
    List {
        /// Filter by scope (global, org, project)
        #[arg(short, long)]
        scope: Option<String>,
    },
    /// Show workflow details
    Show {
        /// Workflow slug
        slug: String,
    },
    /// List workflow versions
    Versions {
        /// Workflow slug
        slug: String,
    },
}

// ─── Agent ───────────────────────────────────────────────────────────

#[derive(Subcommand)]
enum AgentCommands {
    /// List agents
    List {
        /// Filter by status
        #[arg(long)]
        status: Option<String>,
        /// Filter by pool
        #[arg(long)]
        pool: Option<String>,
    },
    /// Show agent details
    Info {
        /// Agent ID
        id: String,
    },
    /// Revoke an agent
    Revoke {
        /// Agent ID
        id: String,
    },
    /// Join token management
    JoinToken {
        #[command(subcommand)]
        action: JoinTokenCommands,
    },
}

#[derive(Subcommand)]
enum JoinTokenCommands {
    /// Create a new join token
    Create {
        /// Token name
        #[arg(short, long)]
        name: Option<String>,
        /// Agent pool
        #[arg(short, long)]
        pool: Option<String>,
        /// Expiration in hours
        #[arg(short, long)]
        expires_in_hours: Option<u32>,
    },
    /// List join tokens
    List,
    /// Revoke a join token
    Revoke {
        /// Token ID
        id: String,
    },
}

// ─── Debug ───────────────────────────────────────────────────────────

#[derive(Subcommand)]
enum DebugCommands {
    /// Run a pipeline locally
    Run {
        /// Path to pipeline file
        path: Option<PathBuf>,
        /// Variables (KEY=VALUE)
        #[arg(short = 'V', long = "var")]
        variables: Vec<String>,
        /// Parse and show execution plan without running
        #[arg(long)]
        dry_run: bool,
    },
    /// Open an interactive shell in a job environment
    Shell,
    /// Replay a failed run locally
    Replay {
        /// Run ID to replay
        run_id: String,
    },
}

// ─── Config ──────────────────────────────────────────────────────────

#[derive(Subcommand)]
enum ConfigCommands {
    /// Show current configuration
    Show,
    /// Set a configuration value
    Set {
        /// Config key (e.g. server.url, context.org)
        key: String,
        /// Config value
        value: String,
    },
    /// Initialize config file with defaults
    Init,
}

// ─── Main ────────────────────────────────────────────────────────────

fn parse_key_value(s: &str) -> Option<(String, String)> {
    let (k, v) = s.split_once('=')?;
    Some((k.to_string(), v.to_string()))
}

fn resolve_token(explicit: Option<String>) -> Option<String> {
    explicit.or_else(auth::load_token)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    if cli.verbose {
        tracing_subscriber::fmt()
            .with_env_filter("met_cli=debug,met_parser=debug")
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_target(false)
            .with_level(false)
            .without_time()
            .init();
    }

    let cfg = CliConfig::load();
    let ctx = ResolvedContext::resolve(
        &cfg,
        cli.server.as_deref(),
        cli.org.as_deref(),
        cli.project.as_deref(),
    );

    let token = resolve_token(cli.token.clone());
    let client = ApiClient::new(&ctx.server_url, token, cli.verbose);
    let format = cli.format;

    let result = match cli.command {
        // ── Auth ─────────────────────────────────────────────
        Commands::Auth { action } => match action {
            AuthCommands::Login => auth_cmd::login(&client, &ctx.server_url).await,
            AuthCommands::Logout => auth_cmd::logout().await,
            AuthCommands::Status => auth_cmd::status(&client, format).await,
            AuthCommands::Token { action } => match action {
                TokenCommands::Create {
                    name,
                    description,
                    scopes,
                    expires_in_days,
                } => {
                    auth_cmd::token_create(
                        &client,
                        &name,
                        description.as_deref(),
                        &scopes,
                        expires_in_days,
                        format,
                    )
                    .await
                }
                TokenCommands::List => auth_cmd::token_list(&client, format).await,
                TokenCommands::Revoke { id } => {
                    auth_cmd::token_revoke(&client, &id, format).await
                }
            },
        },

        // ── Org ──────────────────────────────────────────────
        Commands::Org { action } => match action {
            OrgCommands::List => org::list(&client, format).await,
            OrgCommands::Switch { slug } => org::switch(&slug).await,
            OrgCommands::Info { slug } => {
                let slug = slug
                    .as_deref()
                    .or(ctx.org.as_deref())
                    .ok_or_else(|| {
                        api_client::ApiError::Config(
                            "No organization specified. Use --org or provide a slug.".into(),
                        )
                    })?;
                org::info(&client, slug, format).await
            }
        },

        // ── Project ──────────────────────────────────────────
        Commands::Project { action } => match action {
            ProjectCommands::List => project::list(&client, &ctx, format).await,
            ProjectCommands::Create {
                name,
                description,
                repo_url,
            } => {
                project::create(
                    &client,
                    &ctx,
                    &name,
                    description.as_deref(),
                    repo_url.as_deref(),
                    format,
                )
                .await
            }
            ProjectCommands::Info { slug } => {
                let slug = slug
                    .as_deref()
                    .or(ctx.project.as_deref())
                    .ok_or_else(|| {
                        api_client::ApiError::Config(
                            "No project specified. Use --project or provide a slug.".into(),
                        )
                    })?;
                project::info(&client, &ctx, slug, format).await
            }
            ProjectCommands::Switch { slug } => project::switch(&slug).await,
        },

        // ── Pipeline ─────────────────────────────────────────
        Commands::Pipeline { action } => match action {
            PipelineCommands::List => pipelines::list(&client, &ctx, format).await,
            PipelineCommands::Show { id } => pipelines::show(&client, &id, format).await,
            PipelineCommands::Validate { path } => {
                pipelines::validate(&path, format).await
            }
            PipelineCommands::Trigger {
                id,
                branch,
                commit,
                variables,
            } => {
                let vars: Vec<(String, String)> =
                    variables.iter().filter_map(|v| parse_key_value(v)).collect();
                pipelines::trigger(&client, &id, branch, commit, vars, format).await
            }
            PipelineCommands::Diff { id, base } => {
                pipelines::diff(&client, &id, base.as_deref(), format).await
            }
        },

        // ── Run ──────────────────────────────────────────────
        Commands::Run { action } => match action {
            RunCommands::List { pipeline, limit } => {
                runs::list(&client, &pipeline, limit, format).await
            }
            RunCommands::Status { id } => runs::status(&client, &id, format).await,
            RunCommands::Logs {
                id,
                follow,
                job,
                step,
                tail,
            } => {
                runs::logs(&client, &id, follow, job.as_deref(), step.as_deref(), tail).await
            }
            RunCommands::Cancel { id } => runs::cancel(&client, &id, format).await,
            RunCommands::Retry { id } => runs::retry(&client, &id, format).await,
            RunCommands::Artifacts { id } => runs::artifacts(&client, &id, format).await,
        },

        // ── Secret ───────────────────────────────────────────
        Commands::Secret { action } => match action {
            SecretCommands::List => secret::list(&client, &ctx, format).await,
            SecretCommands::Set { name, value } => {
                let value = match value {
                    Some(v) => v,
                    None => {
                        use std::io::Read;
                        let mut buf = String::new();
                        eprintln!("Enter secret value (then press Ctrl+D):");
                        std::io::stdin().read_to_string(&mut buf).map_err(|e| {
                            api_client::ApiError::Other(format!("Failed to read stdin: {}", e))
                        })?;
                        buf.trim_end().to_string()
                    }
                };
                secret::set(&client, &ctx, &name, &value, format).await
            }
            SecretCommands::Delete { name } => {
                secret::delete(&client, &ctx, &name, format).await
            }
        },

        // ── Variable ─────────────────────────────────────────
        Commands::Variable { action } => match action {
            VariableCommands::List => variable::list(&client, &ctx, format).await,
            VariableCommands::Set {
                name,
                value,
                sensitive,
            } => variable::set(&client, &ctx, &name, &value, sensitive, format).await,
            VariableCommands::Delete { name } => {
                variable::delete(&client, &ctx, &name, format).await
            }
        },

        // ── Workflow ─────────────────────────────────────────
        Commands::Workflow { action } => match action {
            WorkflowCommands::List { scope } => {
                workflow::list(&client, &ctx, scope.as_deref(), format).await
            }
            WorkflowCommands::Show { slug } => {
                workflow::show(&client, &slug, format).await
            }
            WorkflowCommands::Versions { slug } => {
                workflow::versions(&client, &slug, format).await
            }
        },

        // ── Agent ────────────────────────────────────────────
        Commands::Agent { action } => match action {
            AgentCommands::List { status, pool } => {
                agents::list(&client, status, pool, format).await
            }
            AgentCommands::Info { id } => agents::info(&client, &id, format).await,
            AgentCommands::Revoke { id } => agents::revoke(&client, &id, format).await,
            AgentCommands::JoinToken { action } => match action {
                JoinTokenCommands::Create {
                    name,
                    pool,
                    expires_in_hours,
                } => {
                    agents::join_token_create(
                        &client,
                        name.as_deref(),
                        pool.as_deref(),
                        expires_in_hours,
                        format,
                    )
                    .await
                }
                JoinTokenCommands::List => agents::join_token_list(&client, format).await,
                JoinTokenCommands::Revoke { id } => {
                    agents::join_token_revoke(&client, &id, format).await
                }
            },
        },

        // ── Debug ────────────────────────────────────────────
        Commands::Debug { action } => match action {
            DebugCommands::Run {
                path,
                variables,
                dry_run,
            } => {
                let vars: Vec<(String, String)> =
                    variables.iter().filter_map(|v| parse_key_value(v)).collect();
                debug::run(path, vars, dry_run).await
            }
            DebugCommands::Shell => debug::shell().await,
            DebugCommands::Replay { run_id } => debug::replay(&run_id).await,
        },

        // ── Config ───────────────────────────────────────────
        Commands::Config { action } => match action {
            ConfigCommands::Show => config_cmd::show(format).await,
            ConfigCommands::Set { key, value } => config_cmd::set(&key, &value).await,
            ConfigCommands::Init => config_cmd::init().await,
        },
    };

    if let Err(e) = result {
        output::print_error(&format!("{}", e));
        std::process::exit(1);
    }

    Ok(())
}
