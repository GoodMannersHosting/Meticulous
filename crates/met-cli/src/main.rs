//! Meticulous CLI for developers.
//!
//! The CLI provides commands for interacting with the Meticulous API,
//! running pipelines locally, and validating configurations.

use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod api_client;
mod commands;
mod output;

use api_client::ApiClient;
use commands::{agents, config, pipelines, runs};

#[derive(Parser)]
#[command(name = "met")]
#[command(about = "Meticulous CI/CD command-line interface")]
#[command(version)]
struct Cli {
    /// API server URL
    #[arg(long, env = "MET_API_URL", default_value = "http://localhost:8080")]
    api_url: String,

    /// API token for authentication
    #[arg(long, env = "MET_API_TOKEN")]
    token: Option<String>,

    /// Output format (json, table, or yaml)
    #[arg(long, default_value = "table")]
    format: OutputFormat,

    /// Enable debug mode
    #[arg(long)]
    debug: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
enum OutputFormat {
    Table,
    Json,
    Yaml,
}

#[derive(Subcommand)]
enum Commands {
    /// Pipeline commands
    Pipeline {
        #[command(subcommand)]
        action: PipelineCommands,
    },
    /// Run commands
    Run {
        #[command(subcommand)]
        action: RunCommands,
    },
    /// Agent commands
    Agent {
        #[command(subcommand)]
        action: AgentCommands,
    },
    /// Configuration commands
    Config {
        #[command(subcommand)]
        action: ConfigCommands,
    },
}

#[derive(Subcommand)]
enum PipelineCommands {
    /// List pipelines
    List {
        /// Project ID
        #[arg(short, long)]
        project: String,
    },
    /// Show pipeline details
    Show {
        /// Pipeline ID or slug
        id: String,
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
    },
}

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
    /// Show run details
    Show {
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
    },
    /// Cancel a run
    Cancel {
        /// Run ID
        id: String,
    },
    /// Retry a failed run
    Retry {
        /// Run ID
        id: String,
    },
}

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
    Show {
        /// Agent ID
        id: String,
    },
    /// Drain an agent (stop accepting new jobs)
    Drain {
        /// Agent ID
        id: String,
    },
    /// Resume a draining agent
    Resume {
        /// Agent ID
        id: String,
    },
}

#[derive(Subcommand)]
enum ConfigCommands {
    /// Validate pipeline configuration
    Validate {
        /// Path to pipeline file
        path: PathBuf,
    },
    /// Parse and show resolved pipeline
    Parse {
        /// Path to pipeline file
        path: PathBuf,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    if cli.debug {
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

    let client = ApiClient::new(&cli.api_url, cli.token.clone());
    let format = cli.format;

    let result = match cli.command {
        Commands::Pipeline { action } => match action {
            PipelineCommands::List { project } => {
                pipelines::list(&client, &project, format).await
            }
            PipelineCommands::Show { id } => {
                pipelines::show(&client, &id, format).await
            }
            PipelineCommands::Trigger { id, branch, commit } => {
                pipelines::trigger(&client, &id, branch, commit, format).await
            }
        },
        Commands::Run { action } => match action {
            RunCommands::List { pipeline, limit } => {
                runs::list(&client, &pipeline, limit, format).await
            }
            RunCommands::Show { id } => {
                runs::show(&client, &id, format).await
            }
            RunCommands::Logs { id, follow } => {
                runs::logs(&client, &id, follow).await
            }
            RunCommands::Cancel { id } => {
                runs::cancel(&client, &id, format).await
            }
            RunCommands::Retry { id } => {
                runs::retry(&client, &id, format).await
            }
        },
        Commands::Agent { action } => match action {
            AgentCommands::List { status, pool } => {
                agents::list(&client, status, pool, format).await
            }
            AgentCommands::Show { id } => {
                agents::show(&client, &id, format).await
            }
            AgentCommands::Drain { id } => {
                agents::drain(&client, &id, format).await
            }
            AgentCommands::Resume { id } => {
                agents::resume(&client, &id, format).await
            }
        },
        Commands::Config { action } => match action {
            ConfigCommands::Validate { path } => {
                config::validate(&path, format).await
            }
            ConfigCommands::Parse { path } => {
                config::parse(&path, format).await
            }
        },
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }

    Ok(())
}
