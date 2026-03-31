use crate::api_client::Result;
use crate::output::{print_info, print_warning};
use met_parser::{MockWorkflowProvider, PipelineParser, StepCommand};
use std::path::PathBuf;

pub async fn run(
    path: Option<PathBuf>,
    variables: Vec<(String, String)>,
    dry_run: bool,
) -> Result<()> {
    let path = path.unwrap_or_else(|| PathBuf::from(".meticulous/pipeline.yaml"));

    if !path.exists() {
        return Err(crate::api_client::ApiError::Other(format!(
            "Pipeline file not found: {}",
            path.display()
        )));
    }

    let yaml = std::fs::read_to_string(&path).map_err(|e| {
        crate::api_client::ApiError::Other(format!("Failed to read {}: {}", path.display(), e))
    })?;

    let provider = MockWorkflowProvider::new();
    let parser = PipelineParser::new(&provider);
    let pipeline = parser.parse(&yaml).await.map_err(|errors| {
        let msgs: Vec<String> = errors.iter().map(|e| e.to_string()).collect();
        crate::api_client::ApiError::Other(format!("Parse errors:\n  {}", msgs.join("\n  ")))
    })?;

    if !variables.is_empty() {
        print_info(&format!("Overriding {} variable(s)", variables.len()));
    }

    println!("Pipeline: {}", pipeline.name);
    println!("Source:   {}", path.display());
    println!("Jobs:     {}", pipeline.jobs.len());
    println!();

    for (i, job) in pipeline.jobs.iter().enumerate() {
        let deps = if job.depends_on.is_empty() {
            String::new()
        } else {
            let dep_names: Vec<String> = job.depends_on.iter().map(|d| d.to_string()).collect();
            format!(" (depends on: {})", dep_names.join(", "))
        };

        println!("  Job {}: {}{}", i + 1, job.name, deps);
        println!("    Timeout: {}s", job.timeout.as_secs());

        if let Some(ref condition) = job.condition {
            println!("    Condition: {}", condition);
        }

        for (j, step) in job.steps.iter().enumerate() {
            let cmd_desc = match &step.command {
                StepCommand::Run { script, .. } => {
                    let preview: String = script.chars().take(60).collect();
                    if script.len() > 60 {
                        format!("run: {}...", preview)
                    } else {
                        format!("run: {}", preview)
                    }
                }
                StepCommand::Action { name, .. } => {
                    format!("action: {}", name)
                }
            };
            println!("    Step {}: {} ({})", j + 1, step.name, cmd_desc);
        }
        println!();
    }

    if dry_run {
        print_info("Dry run complete — no containers were started.");
        return Ok(());
    }

    print_warning("Local container execution is not yet implemented.");
    print_info("Pipeline parsed and validated successfully.");
    print_info("Use --dry-run to validate pipeline definitions without execution.");

    Ok(())
}

pub async fn shell() -> Result<()> {
    print_warning("Interactive debug shell is not yet implemented.");
    print_info("This will launch an interactive shell inside a job's container environment.");
    print_info("For now, use `docker run -it <image> /bin/sh` to replicate the job environment.");
    Ok(())
}

pub async fn replay(run_id: &str) -> Result<()> {
    print_warning("Run replay is not yet implemented.");
    print_info(&format!(
        "This will re-execute run '{}' locally using its original configuration and inputs.",
        run_id
    ));
    print_info("The replay feature requires the local container runtime (coming soon).");
    Ok(())
}
