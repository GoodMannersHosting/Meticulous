//! Remote pipeline validation endpoint (ADR-019, Phase 3.3).
//!
//! Parses pipeline YAML against live project state and returns structured
//! diagnostics with fuzzy typo suggestions.

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::post,
};
use met_core::fuzzy;
use met_core::ids::ProjectId;
use met_store::repos::{BuiltinSecretsRepo, EnvironmentRepo, ProjectRepo};
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::{
    error::{ApiError, ApiResult},
    extractors::Auth,
    project_access::effective_project_role_in_user_org,
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new().route(
        "/projects/{project_id}/pipelines/check",
        post(check_pipeline),
    )
}

#[derive(Debug, Deserialize)]
struct CheckRequest {
    definition: String,
    #[serde(default)]
    r#ref: Option<String>,
    #[serde(default)]
    environment: Option<String>,
}

#[derive(Debug, Serialize)]
struct CheckResponse {
    valid: bool,
    diagnostics: Vec<CheckDiagnostic>,
    summary: CheckSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CheckDiagnostic {
    code: String,
    severity: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    location: Option<DiagLocation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    suggestion: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    doc_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DiagLocation {
    line: Option<usize>,
    column: Option<usize>,
}

#[derive(Debug, Serialize)]
struct CheckSummary {
    errors: u32,
    warnings: u32,
    info: u32,
}

#[instrument(skip(state, req))]
async fn check_pipeline(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(project_id): Path<ProjectId>,
    Json(req): Json<CheckRequest>,
) -> ApiResult<Json<CheckResponse>> {
    let _role = effective_project_role_in_user_org(state.db(), &user, project_id).await?;

    let mut diagnostics = Vec::new();

    // Stage 1: Parse
    let pipeline: met_parser::RawPipeline = match serde_yaml::from_str(&req.definition) {
        Ok(p) => p,
        Err(e) => {
            diagnostics.push(CheckDiagnostic {
                code: "PARSE".into(),
                severity: "error".into(),
                message: format!("YAML parse error: {e}"),
                location: None,
                suggestion: None,
                doc_url: None,
            });
            return Ok(Json(build_response(diagnostics)));
        }
    };

    // Stage 1b: Lint
    let lint_diags = met_lint::lint_raw_pipeline(&pipeline);
    for d in lint_diags {
        diagnostics.push(CheckDiagnostic {
            code: d.rule_id,
            severity: match d.severity {
                met_lint::Severity::Error => "error".into(),
                met_lint::Severity::Warning => "warning".into(),
                met_lint::Severity::Info => "info".into(),
            },
            message: d.message,
            location: None,
            suggestion: None,
            doc_url: d.remediation_url,
        });
    }

    // Stage 3: Secret reference validation
    let project = ProjectRepo::new(state.db()).get(project_id).await?;
    let secrets = BuiltinSecretsRepo::new(state.db())
        .list_for_project(project.org_id.into(), project_id)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    let secret_names: Vec<&str> = secrets.iter().map(|s| s.path.as_str()).collect();

    for (env_name, secret_ref) in &pipeline.secrets {
        let ref_name = secret_ref
            .stored
            .as_ref()
            .map(|s| s.name.as_str())
            .or_else(|| secret_ref.builtin.as_ref().map(|b| b.name.as_str()));

        if let Some(name) = ref_name {
            if !secret_names.contains(&name) {
                let suggestions = fuzzy::suggest(name, &secret_names, 3);
                diagnostics.push(CheckDiagnostic {
                    code: "CHK-003".into(),
                    severity: "error".into(),
                    message: format!(
                        "secret '{name}' not found in project scope (referenced as {env_name})"
                    ),
                    location: None,
                    suggestion: fuzzy::format_suggestions(&suggestions),
                    doc_url: Some("https://docs.meticulous.example.com/diagnostics/CHK-003".into()),
                });
            }
        }
    }

    // Stage 5: Environment validation
    if let Some(ref env_name) = req.environment {
        let env_repo = EnvironmentRepo::new(state.db());
        let env = env_repo
            .get_by_name(project_id, env_name)
            .await
            .map_err(|e| ApiError::internal(e.to_string()))?;
        match env {
            None => {
                diagnostics.push(CheckDiagnostic {
                    code: "CHK-009".into(),
                    severity: "error".into(),
                    message: format!("environment '{env_name}' not found"),
                    location: None,
                    suggestion: None,
                    doc_url: Some("https://docs.meticulous.example.com/diagnostics/CHK-009".into()),
                });
            }
            Some(env_row) => {
                if let Some(ref trigger_ref) = req.r#ref {
                    if !EnvironmentRepo::branch_allowed(&env_row, trigger_ref) {
                        diagnostics.push(CheckDiagnostic {
                            code: "CHK-007".into(),
                            severity: "error".into(),
                            message: format!(
                                "environment '{env_name}' blocks ref '{trigger_ref}' (allowed: {:?})",
                                env_row.allowed_branches.as_deref().unwrap_or(&[])
                            ),
                            location: None,
                            suggestion: None,
                            doc_url: Some("https://docs.meticulous.example.com/diagnostics/CHK-007".into()),
                        });
                    }
                }
                if env_row.require_approval {
                    diagnostics.push(CheckDiagnostic {
                        code: "CHK-008".into(),
                        severity: "info".into(),
                        message: format!(
                            "environment '{env_name}' requires approval ({} approver(s), {}h timeout)",
                            env_row.required_approvers, env_row.approval_timeout_hours
                        ),
                        location: None,
                        suggestion: None,
                        doc_url: Some("https://docs.meticulous.example.com/diagnostics/CHK-008".into()),
                    });
                }
            }
        }
    }

    Ok(Json(build_response(diagnostics)))
}

fn build_response(diagnostics: Vec<CheckDiagnostic>) -> CheckResponse {
    let errors = diagnostics.iter().filter(|d| d.severity == "error").count() as u32;
    let warnings = diagnostics
        .iter()
        .filter(|d| d.severity == "warning")
        .count() as u32;
    let info = diagnostics.iter().filter(|d| d.severity == "info").count() as u32;
    let valid = errors == 0;
    CheckResponse {
        valid,
        diagnostics,
        summary: CheckSummary {
            errors,
            warnings,
            info,
        },
    }
}
