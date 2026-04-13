//! Variable interpolation and resolution.
//!
//! Handles `${VAR}` syntax for variable references in pipeline definitions.

use crate::error::{ErrorCode, ParseDiagnostics, SourceLocation};
use indexmap::IndexMap;
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

/// Pattern for variable references: ${name} or $name
static VAR_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\$\{([a-zA-Z_][a-zA-Z0-9_]*)\}|\$([a-zA-Z_][a-zA-Z0-9_]*)").unwrap()
});

/// Pattern for expression syntax: ${{ expression }}
static EXPR_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\$\{\{\s*(.+?)\s*\}\}").unwrap());

/// Built-in context variables available in all pipelines.
pub static BUILTIN_VARS: &[&str] = &[
    "MET_RUN_ID",
    "MET_PIPELINE_NAME",
    "MET_PIPELINE_ID",
    "MET_PROJECT_ID",
    "MET_PROJECT_NAME",
    "MET_COMMIT_SHA",
    "MET_COMMIT_SHORT_SHA",
    "MET_COMMIT_MESSAGE",
    "MET_COMMIT_AUTHOR",
    "MET_BRANCH",
    "MET_TAG",
    "MET_EVENT_TYPE",
    "MET_TRIGGER_TYPE",
    "MET_WORKSPACE",
    "MET_JOB_ID",
    "MET_JOB_NAME",
    "MET_STEP_ID",
    "MET_STEP_NAME",
];

/// Variable context for resolution.
#[derive(Debug, Default)]
pub struct VariableContext {
    /// Pipeline-level variables.
    pub vars: IndexMap<String, String>,
    /// Declared secrets (names only, for validation).
    pub secrets: HashSet<String>,
    /// Workflow inputs (for workflow scope).
    pub inputs: IndexMap<String, String>,
    /// Step outputs from previous steps (for runtime resolution).
    pub step_outputs: IndexMap<String, IndexMap<String, String>>,
    /// Pipeline `workflows[].id` values (for `${{ workflows.X.outputs.* }}` validation).
    pub workflow_invocations: HashSet<String>,
    /// Declared output names per workflow invocation id (from reusable workflow YAML). Empty set skips name checks.
    pub workflow_declared_outputs: HashMap<String, HashSet<String>>,
}

impl VariableContext {
    /// Create a new context with pipeline variables and secrets.
    pub fn new(vars: IndexMap<String, String>, secrets: HashSet<String>) -> Self {
        Self {
            vars,
            secrets,
            inputs: IndexMap::new(),
            step_outputs: IndexMap::new(),
            workflow_invocations: HashSet::new(),
            workflow_declared_outputs: HashMap::new(),
        }
    }

    /// Add workflow inputs to the context.
    pub fn with_inputs(mut self, inputs: IndexMap<String, String>) -> Self {
        self.inputs = inputs;
        self
    }

    /// Pipeline workflow invocation ids available for `workflows.<id>.outputs.*` references.
    pub fn with_workflow_invocations(mut self, ids: HashSet<String>) -> Self {
        self.workflow_invocations = ids;
        self
    }

    /// Declared `outputs` names for each `workflows[].id` (workflow- and step-level declarations).
    pub fn with_workflow_declared_outputs(mut self, map: HashMap<String, HashSet<String>>) -> Self {
        self.workflow_declared_outputs = map;
        self
    }

    /// Check if a variable is defined in any scope.
    pub fn is_defined(&self, name: &str) -> bool {
        BUILTIN_VARS.contains(&name)
            || self.vars.contains_key(name)
            || self.inputs.contains_key(name)
            || self.secrets.contains(name)
    }

    /// Check if a name refers to a secret.
    pub fn is_secret(&self, name: &str) -> bool {
        self.secrets.contains(name)
    }

    /// Resolve a variable reference (for static values only).
    /// Returns None for runtime-only variables (secrets, builtins).
    pub fn resolve(&self, name: &str) -> Option<&str> {
        self.inputs
            .get(name)
            .or_else(|| self.vars.get(name))
            .map(String::as_str)
    }
}

/// Result of variable extraction.
#[derive(Debug)]
pub struct VarExtraction {
    /// Variable references found.
    pub vars: Vec<VarRef>,
    /// Expression references found.
    pub expressions: Vec<ExprRef>,
}

/// A variable reference in a string.
#[derive(Debug, Clone)]
pub struct VarRef {
    /// Variable name.
    pub name: String,
    /// Start position in the original string.
    pub start: usize,
    /// End position in the original string.
    pub end: usize,
}

/// An expression reference in a string.
#[derive(Debug, Clone)]
pub struct ExprRef {
    /// Expression content.
    pub expr: String,
    /// Start position in the original string.
    pub start: usize,
    /// End position in the original string.
    pub end: usize,
}

/// Extract all variable references from a string.
pub fn extract_vars(s: &str) -> VarExtraction {
    let mut vars = Vec::new();
    let mut expressions = Vec::new();

    for cap in VAR_PATTERN.captures_iter(s) {
        let m = cap.get(0).unwrap();
        let name = cap.get(1).or_else(|| cap.get(2)).unwrap().as_str();
        vars.push(VarRef {
            name: name.to_string(),
            start: m.start(),
            end: m.end(),
        });
    }

    for cap in EXPR_PATTERN.captures_iter(s) {
        let m = cap.get(0).unwrap();
        let expr = cap.get(1).unwrap().as_str();
        expressions.push(ExprRef {
            expr: expr.to_string(),
            start: m.start(),
            end: m.end(),
        });
    }

    VarExtraction { vars, expressions }
}

/// Validate all variable references in a string (Meticulous `${VAR}` / `$VAR` and `${{ … }}`).
pub fn validate_refs(
    s: &str,
    ctx: &VariableContext,
    diagnostics: &mut ParseDiagnostics,
    location: SourceLocation,
) {
    let extraction = extract_vars(s);

    for var_ref in &extraction.vars {
        if !ctx.is_defined(&var_ref.name) {
            let mut loc = location.clone();
            loc.column += var_ref.start;

            diagnostics.push(
                crate::error::ParseError::new(
                    ErrorCode::E4001,
                    format!("undefined variable: {}", var_ref.name),
                )
                .with_source(loc)
                .with_hint(format!(
                    "available variables: {}",
                    ctx.vars
                        .keys()
                        .chain(ctx.inputs.keys())
                        .take(5)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                )),
            );
        }
    }

    for expr_ref in &extraction.expressions {
        validate_expression(&expr_ref.expr, ctx, diagnostics, location.clone());
    }
}

/// Validate references in a `run:` script body.
///
/// `$VAR` / `${VAR}` are shell syntax (often assigned inside the same script). Only `${{ … }}`
/// Meticulous expressions are checked here (e.g. `inputs.repo`, `vars.X`).
pub fn validate_refs_in_run_script(
    s: &str,
    ctx: &VariableContext,
    diagnostics: &mut ParseDiagnostics,
    location: SourceLocation,
) {
    let extraction = extract_vars(s);

    for expr_ref in &extraction.expressions {
        validate_expression(&expr_ref.expr, ctx, diagnostics, location.clone());
    }
}

/// Validate an expression (basic check for now).
fn validate_expression(
    expr: &str,
    ctx: &VariableContext,
    diagnostics: &mut ParseDiagnostics,
    location: SourceLocation,
) {
    let parts: Vec<&str> = expr.split('.').collect();
    if parts.is_empty() {
        return;
    }

    match parts[0] {
        "workflows" => {
            if parts.len() == 4 && parts[2] == "outputs" {
                let inv = parts[1];
                let out_name = parts[3];
                if !ctx.workflow_invocations.contains(inv) {
                    diagnostics.push(
                        crate::error::ParseError::new(
                            ErrorCode::E4001,
                            format!("unknown workflow invocation id in outputs reference: {inv}"),
                        )
                        .with_source(location),
                    );
                } else if let Some(declared) = ctx.workflow_declared_outputs.get(inv)
                    && !declared.is_empty()
                    && !declared.contains(out_name)
                {
                    let mut sample: Vec<&String> = declared.iter().collect();
                    sample.sort();
                    diagnostics.push(
                        crate::error::ParseError::new(
                            ErrorCode::E4001,
                            format!(
                                "workflow invocation '{inv}' has no declared output '{out_name}'"
                            ),
                        )
                        .with_source(location.clone())
                        .with_hint(format!(
                            "declared outputs for '{inv}': {}",
                            sample
                                .into_iter()
                                .map(|s| s.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        )),
                    );
                }
            }
        }
        "inputs" => {
            if parts.len() > 1 && !ctx.inputs.contains_key(parts[1]) {
                diagnostics.push(
                    crate::error::ParseError::new(
                        ErrorCode::E4001,
                        format!("undefined input: {}", parts[1]),
                    )
                    .with_source(location),
                );
            }
        }
        "vars" => {
            if parts.len() > 1 && !ctx.vars.contains_key(parts[1]) {
                diagnostics.push(
                    crate::error::ParseError::new(
                        ErrorCode::E4001,
                        format!("undefined variable: {}", parts[1]),
                    )
                    .with_source(location),
                );
            }
        }
        "secrets" => {
            if parts.len() > 1 && !ctx.secrets.contains(parts[1]) {
                diagnostics.push(
                    crate::error::ParseError::new(
                        ErrorCode::E4002,
                        format!("undefined secret: {}", parts[1]),
                    )
                    .with_source(location),
                );
            }
        }
        "steps" | "jobs" | "env" | "trigger" => {
            // These are runtime contexts, can't validate statically
        }
        _ => {
            // Check if it's a direct variable reference
            if !ctx.is_defined(parts[0]) {
                diagnostics.push(
                    crate::error::ParseError::new(
                        ErrorCode::E4001,
                        format!("undefined reference: {}", parts[0]),
                    )
                    .with_source(location),
                );
            }
        }
    }
}

/// Replace `${{ inputs.KEY }}` and `${{ vars.KEY }}` using workflow invocation inputs and pipeline vars.
///
/// Leaves other `${{ ... }}` blocks unchanged (e.g. `${{ secrets.X }}` for runtime / env).
pub fn interpolate_workflow_templates(
    s: &str,
    pipeline_vars: &IndexMap<String, String>,
    workflow_inputs: &IndexMap<String, String>,
) -> String {
    let extraction = extract_vars(s);
    let mut result = s.to_string();
    for expr in extraction.expressions.iter().rev() {
        let trimmed = expr.expr.trim();
        let replacement = if let Some(key) = trimmed.strip_prefix("inputs.") {
            workflow_inputs.get(key)
        } else if let Some(key) = trimmed.strip_prefix("vars.") {
            pipeline_vars.get(key)
        } else {
            None
        };
        if let Some(rep) = replacement {
            result.replace_range(expr.start..expr.end, rep);
        }
    }
    result
}

/// Interpolate variables in a string with known values.
/// Leaves unresolvable references (secrets, builtins) as-is.
pub fn interpolate(s: &str, ctx: &VariableContext) -> String {
    let mut result = s.to_string();

    for cap in VAR_PATTERN.captures_iter(s) {
        let full_match = cap.get(0).unwrap().as_str();
        let name = cap.get(1).or_else(|| cap.get(2)).unwrap().as_str();

        if let Some(value) = ctx.resolve(name) {
            result = result.replace(full_match, value);
        }
    }

    result
}

/// Check if a string contains any variable references.
pub fn has_refs(s: &str) -> bool {
    VAR_PATTERN.is_match(s) || EXPR_PATTERN.is_match(s)
}

/// Check if a string contains secret references.
pub fn has_secret_refs(s: &str, ctx: &VariableContext) -> bool {
    for cap in VAR_PATTERN.captures_iter(s) {
        let name = cap.get(1).or_else(|| cap.get(2)).unwrap().as_str();
        if ctx.is_secret(name) {
            return true;
        }
    }

    for cap in EXPR_PATTERN.captures_iter(s) {
        let expr = cap.get(1).unwrap().as_str();
        if expr.starts_with("secrets.") {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;

    #[test]
    fn test_extract_vars() {
        let s = "Hello ${NAME}, your ID is $ID";
        let extraction = extract_vars(s);

        assert_eq!(extraction.vars.len(), 2);
        assert_eq!(extraction.vars[0].name, "NAME");
        assert_eq!(extraction.vars[1].name, "ID");
    }

    #[test]
    fn test_extract_expressions() {
        let s = "Image: ${{ inputs.image }}:${{ vars.TAG }}";
        let extraction = extract_vars(s);

        assert_eq!(extraction.expressions.len(), 2);
        assert_eq!(extraction.expressions[0].expr, "inputs.image");
        assert_eq!(extraction.expressions[1].expr, "vars.TAG");
    }

    #[test]
    fn test_interpolate() {
        let mut ctx = VariableContext::default();
        ctx.vars.insert("NAME".to_string(), "World".to_string());
        ctx.vars.insert("VERSION".to_string(), "1.0".to_string());

        let result = interpolate("Hello ${NAME} v${VERSION}", &ctx);
        assert_eq!(result, "Hello World v1.0");
    }

    #[test]
    fn test_interpolate_workflow_templates() {
        let mut vars = IndexMap::new();
        vars.insert("TAG".to_string(), "v2".to_string());
        let mut inputs = IndexMap::new();
        inputs.insert("image".to_string(), "app:latest".to_string());

        let s = "pull ${{ inputs.image }} rel ${{ vars.TAG }} keep ${{ secrets.TOKEN }}";
        let out = interpolate_workflow_templates(s, &vars, &inputs);
        assert_eq!(out, "pull app:latest rel v2 keep ${{ secrets.TOKEN }}");
    }

    #[test]
    fn test_validate_refs() {
        let mut ctx = VariableContext::default();
        ctx.vars.insert("DEFINED".to_string(), "value".to_string());
        ctx.secrets.insert("SECRET".to_string());

        let mut diag = ParseDiagnostics::new();
        validate_refs(
            "${DEFINED} ${UNDEFINED}",
            &ctx,
            &mut diag,
            SourceLocation::new(1, 1),
        );

        assert!(diag.has_errors());
        assert_eq!(diag.len(), 1);
        assert!(diag.all()[0].message.contains("UNDEFINED"));
    }

    #[test]
    fn test_validate_refs_in_run_script_allows_shell_locals() {
        let mut ctx = VariableContext::new(IndexMap::new(), HashSet::new());
        ctx.inputs.insert("repo".to_string(), "o/r".to_string());

        let script = indoc! {r#"
            WS="${METICULOUS_WORKSPACE:?}"
            SRC="${WS}/src"
            echo "clone ${{ inputs.repo }} into ${SRC}"
        "#};

        let mut diag = ParseDiagnostics::new();
        validate_refs_in_run_script(script, &ctx, &mut diag, SourceLocation::new(1, 1));
        assert!(
            !diag.has_errors(),
            "shell WS/SRC must not be validated as pipeline vars: {:?}",
            diag.all()
        );
    }

    #[test]
    fn test_validate_refs_in_run_script_still_checks_expressions() {
        let ctx = VariableContext::new(IndexMap::new(), HashSet::new());

        let mut diag = ParseDiagnostics::new();
        validate_refs_in_run_script(
            "echo ${{ inputs.missing }}",
            &ctx,
            &mut diag,
            SourceLocation::new(1, 1),
        );
        assert!(diag.has_errors());
        let msg = diag.all()[0].message.as_str();
        assert!(msg.contains("missing"), "unexpected diagnostic: {msg}");
    }

    #[test]
    fn test_has_secret_refs() {
        let mut ctx = VariableContext::default();
        ctx.secrets.insert("PASSWORD".to_string());

        assert!(has_secret_refs("${PASSWORD}", &ctx));
        assert!(has_secret_refs("${{ secrets.API_KEY }}", &ctx));
        assert!(!has_secret_refs("${REGULAR_VAR}", &ctx));
    }
}
