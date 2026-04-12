//! `met suggest --assess` — AI-assisted pipeline security assessment (ADR-009).

use crate::OutputFormat;
use crate::api_client::{ApiError, Result};
use std::path::Path;

const ASSESS_SYSTEM_PROMPT: &str = r#"You are a security auditor reviewing a CI/CD pipeline definition for Meticulous.

Rules:
- Assume breach. Every external input is hostile until proven otherwise.
- Never compliment the pipeline. Only report issues and risks.
- Recommend the most restrictive option that preserves functionality.
- Flag uncertainty explicitly: "UNCERTAIN: ..." when confidence is below 90%.
- Reference SLSA levels, MITRE ATT&CK techniques, and CWE IDs where applicable.
- Do not be sycophantic. Do not soften findings.

Output a structured risk report in this format:

## Critical Findings
<numbered list>

## High Risk
<numbered list>

## Medium Risk
<numbered list>

## Recommendations
<numbered list of actionable next steps>
"#;

pub async fn assess(path: &Path, _format: OutputFormat) -> Result<()> {
    let yaml = tokio::fs::read_to_string(path)
        .await
        .map_err(|e| ApiError::Other(format!("read error: {e}")))?;

    let _pipeline: met_parser::RawPipeline = serde_yaml::from_str(&yaml)
        .map_err(|e| ApiError::Other(format!("YAML parse error: {e}")))?;

    eprintln!("Security assessment mode (--assess)");
    eprintln!("Pipeline: {path}", path = path.display());
    eprintln!();

    // TODO: call configured LLM API with ASSESS_SYSTEM_PROMPT + pipeline YAML
    eprintln!("LLM backend not configured. System prompt for manual assessment:\n");
    eprintln!("{ASSESS_SYSTEM_PROMPT}");
    eprintln!("--- Pipeline definition ---\n");
    eprintln!("{yaml}");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_prompt_content() {
        assert!(ASSESS_SYSTEM_PROMPT.contains("Assume breach"));
        assert!(ASSESS_SYSTEM_PROMPT.contains("SLSA"));
        assert!(ASSESS_SYSTEM_PROMPT.contains("MITRE"));
        assert!(ASSESS_SYSTEM_PROMPT.contains("sycophantic"));
    }
}
