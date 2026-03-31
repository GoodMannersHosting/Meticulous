//! Configuration validation and parsing commands.

use crate::api_client::Result;
use crate::output::{print, print_error, print_info, print_success};
use crate::OutputFormat;
use met_parser::{MockWorkflowProvider, PipelineParser};
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Serialize)]
struct ValidationResult {
    valid: bool,
    path: String,
    errors: Vec<String>,
    warnings: Vec<String>,
}

pub async fn validate(path: &Path, format: OutputFormat) -> Result<()> {
    let path_str = path.display().to_string();

    if !path.exists() {
        print_error(&format!("File not found: {}", path_str));
        return Ok(());
    }

    let yaml = match std::fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(e) => {
            print_error(&format!("Failed to read file: {}", e));
            return Ok(());
        }
    };

    let provider = MockWorkflowProvider::new();
    let parser = PipelineParser::new(&provider);

    let result = match parser.parse(&yaml).await {
        Ok(_pipeline) => ValidationResult {
            valid: true,
            path: path_str.clone(),
            errors: Vec::new(),
            warnings: Vec::new(),
        },
        Err(errors) => ValidationResult {
            valid: false,
            path: path_str.clone(),
            errors: errors.iter().map(|e| e.to_string()).collect(),
            warnings: Vec::new(),
        },
    };

    match format {
        OutputFormat::Table => {
            if result.valid {
                print_success(&format!("Pipeline configuration is valid: {}", path_str));
            } else {
                print_error(&format!("Pipeline configuration is invalid: {}", path_str));
                for error in &result.errors {
                    println!("  - {}", error);
                }
            }
            for warning in &result.warnings {
                print_info(&format!("Warning: {}", warning));
            }
        }
        _ => {
            print(&result, format)?;
        }
    }

    Ok(())
}

pub async fn parse(path: &Path, format: OutputFormat) -> Result<()> {
    let path_str = path.display().to_string();

    if !path.exists() {
        print_error(&format!("File not found: {}", path_str));
        return Ok(());
    }

    let yaml = match std::fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(e) => {
            print_error(&format!("Failed to read file: {}", e));
            return Ok(());
        }
    };

    let provider = MockWorkflowProvider::new();
    let parser = PipelineParser::new(&provider);

    match parser.parse(&yaml).await {
        Ok(pipeline) => {
            #[derive(Serialize)]
            struct ParsedPipeline {
                id: String,
                name: String,
                source_file: Option<String>,
                jobs: Vec<ParsedJob>,
                variables: std::collections::HashMap<String, String>,
            }

            #[derive(Serialize)]
            struct ParsedJob {
                id: String,
                name: String,
                depends_on: Vec<String>,
                steps: Vec<ParsedStep>,
                timeout_secs: u64,
                has_condition: bool,
            }

            #[derive(Serialize)]
            struct ParsedStep {
                id: String,
                name: String,
                command_type: String,
                timeout_secs: u64,
            }

            let parsed = ParsedPipeline {
                id: pipeline.id.to_string(),
                name: pipeline.name.clone(),
                source_file: pipeline.source_file.clone(),
                jobs: pipeline
                    .jobs
                    .iter()
                    .map(|j| ParsedJob {
                        id: j.id.to_string(),
                        name: j.name.clone(),
                        depends_on: j.depends_on.iter().map(|d| d.to_string()).collect(),
                        steps: j
                            .steps
                            .iter()
                            .map(|s| ParsedStep {
                                id: s.id.to_string(),
                                name: s.name.clone(),
                                command_type: match &s.command {
                                    met_parser::StepCommand::Run { .. } => "run".to_string(),
                                    met_parser::StepCommand::Action { name, .. } => {
                                        format!("action:{}", name)
                                    }
                                },
                                timeout_secs: s.timeout.as_secs(),
                            })
                            .collect(),
                        timeout_secs: j.timeout.as_secs(),
                        has_condition: j.condition.is_some(),
                    })
                    .collect(),
                variables: pipeline
                    .variables
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect(),
            };

            match format {
                OutputFormat::Table => {
                    println!("Pipeline: {} ({})", parsed.name, parsed.id);
                    println!();
                    println!("Jobs ({}):", parsed.jobs.len());
                    for job in &parsed.jobs {
                        let deps = if job.depends_on.is_empty() {
                            String::new()
                        } else {
                            format!(" <- [{}]", job.depends_on.join(", "))
                        };
                        println!("  - {}{}", job.name, deps);
                        for step in &job.steps {
                            println!("    - {} ({})", step.name, step.command_type);
                        }
                    }
                    if !parsed.variables.is_empty() {
                        println!();
                        println!("Variables:");
                        for (key, value) in &parsed.variables {
                            println!("  {}: {}", key, value);
                        }
                    }
                }
                _ => {
                    print(&parsed, format)?;
                }
            }
        }
        Err(errors) => {
            print_error(&format!("Failed to parse pipeline: {}", path_str));
            for error in errors {
                println!("  - {}", error);
            }
        }
    }

    Ok(())
}
