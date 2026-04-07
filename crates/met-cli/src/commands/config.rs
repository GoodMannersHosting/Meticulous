use crate::OutputFormat;
use crate::api_client::Result;
use crate::config::{CliConfig, global_config_path};
use crate::output::{print_info, print_success};

pub async fn show(format: OutputFormat) -> Result<()> {
    let config = CliConfig::load();

    match format {
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(&config)
                .map_err(|e| crate::api_client::ApiError::InvalidResponse(e.to_string()))?;
            println!("{}", json);
        }
        OutputFormat::Yaml => {
            let yaml = serde_yaml::to_string(&config)
                .map_err(|e| crate::api_client::ApiError::InvalidResponse(e.to_string()))?;
            print!("{}", yaml);
        }
        OutputFormat::Table => {
            if let Some(path) = global_config_path() {
                println!("Config file: {}", path.display());
                println!();
            }
            println!("[server]");
            println!("  url = \"{}\"", config.server.url);
            println!();
            println!("[context]");
            println!(
                "  org = {}",
                config
                    .context
                    .org
                    .as_deref()
                    .map(|s| format!("\"{}\"", s))
                    .unwrap_or_else(|| "(not set)".to_string())
            );
            println!(
                "  project = {}",
                config
                    .context
                    .project
                    .as_deref()
                    .map(|s| format!("\"{}\"", s))
                    .unwrap_or_else(|| "(not set)".to_string())
            );
            println!();
            println!("[output]");
            println!("  format = \"{}\"", config.output.format);
            println!("  color = \"{}\"", config.output.color);
        }
    }
    Ok(())
}

pub async fn set(key: &str, value: &str) -> Result<()> {
    let mut config = CliConfig::load();

    match key {
        "server.url" => config.server.url = value.to_string(),
        "context.org" => config.context.org = Some(value.to_string()),
        "context.project" => config.context.project = Some(value.to_string()),
        "output.format" => config.output.format = value.to_string(),
        "output.color" => config.output.color = value.to_string(),
        _ => {
            return Err(crate::api_client::ApiError::Config(format!(
                "Unknown config key '{}'. Valid keys: server.url, context.org, context.project, output.format, output.color",
                key
            )));
        }
    }

    config
        .save_global()
        .map_err(|e| crate::api_client::ApiError::Other(format!("Failed to save config: {}", e)))?;

    print_success(&format!("Set {} = \"{}\"", key, value));
    Ok(())
}

pub async fn init() -> Result<()> {
    let path = global_config_path().ok_or_else(|| {
        crate::api_client::ApiError::Other("Could not determine config directory".into())
    })?;

    if path.exists() {
        print_info(&format!("Config file already exists at {}", path.display()));
        return Ok(());
    }

    let config = CliConfig::default();
    config.save_global().map_err(|e| {
        crate::api_client::ApiError::Other(format!("Failed to write config: {}", e))
    })?;

    print_success(&format!("Config initialized at {}", path.display()));
    Ok(())
}
