//! `met run --local` — run pipeline jobs locally in OCI containers (ADR-018).

use crate::OutputFormat;
use crate::api_client::{ApiClient, ApiError, Result};
use std::path::Path;

pub async fn run_local(
    _client: &ApiClient,
    path: &Path,
    job: Option<&str>,
    network: bool,
    _format: OutputFormat,
) -> Result<()> {
    let yaml = tokio::fs::read_to_string(path)
        .await
        .map_err(|e| ApiError::Other(format!("read error: {e}")))?;

    let pipeline: met_parser::RawPipeline = serde_yaml::from_str(&yaml)
        .map_err(|e| ApiError::Other(format!("YAML parse error: {e}")))?;

    let wf_count = pipeline.workflows.len();

    println!("Pipeline: {}", pipeline.name);
    println!("Workflows: {wf_count}");
    println!(
        "Network: {}",
        if network { "host" } else { "none (isolated)" }
    );
    if let Some(j) = job {
        println!("Targeting job: {j}");
    }

    println!("\nMET_LOCAL=true — secrets are not injected.");

    // TODO: implement full local execution loop:
    // 1. FileSystemWorkflowProvider resolves workflow refs from local disk
    // 2. Parse + expand pipeline IR
    // 3. Topological sort jobs
    // 4. For each job: pull OCI image (if environment set), run steps via ContainerBackend
    // 5. Variables from YAML only (no server); secrets empty
    // 6. Network mode: --network=none unless --network flag

    eprintln!("\nLocal execution engine is not yet fully implemented.");
    eprintln!("Pipeline parsed successfully with {wf_count} workflow invocations.");

    Ok(())
}
