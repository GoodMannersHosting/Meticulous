//! Deterministic pipeline linter (ADR-009).
//!
//! `met-lint` is a pure, sync, offline rule engine. It has no network I/O and
//! no async dependencies. Rules produce structured [`Diagnostic`] output.

pub mod rules;

use serde::{Deserialize, Serialize};

/// Severity levels for lint diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

/// A structured diagnostic emitted by a lint rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    pub rule_id: String,
    pub severity: Severity,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub col: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remediation_url: Option<String>,
}

impl Diagnostic {
    pub fn error(rule_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            rule_id: rule_id.into(),
            severity: Severity::Error,
            message: message.into(),
            file: None,
            line: None,
            col: None,
            remediation_url: None,
        }
    }

    pub fn warning(rule_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            rule_id: rule_id.into(),
            severity: Severity::Warning,
            message: message.into(),
            file: None,
            line: None,
            col: None,
            remediation_url: None,
        }
    }
}

/// Run all lint rules against a parsed pipeline definition.
pub fn lint_raw_pipeline(pipeline: &met_parser::RawPipeline) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for wf in &pipeline.workflows {
        rules::supply_chain::check_workflow_invocation(wf, &mut diags);
    }
    diags
}

/// Run all lint rules against a parsed workflow definition (individual jobs).
pub fn lint_raw_workflow(workflow: &met_parser::RawWorkflowDef) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for job in &workflow.jobs {
        rules::supply_chain::check_job_environment(job, &mut diags);
    }
    diags
}
