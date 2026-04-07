//! CEL (Common Expression Language) evaluation for conditional execution.
//!
//! Supports conditions like:
//! - `success()` - all dependencies succeeded
//! - `failure()` - any dependency failed
//! - `always()` - run regardless of dependency status
//! - `cancelled()` - run was cancelled
//! - `variables.foo == 'bar'`
//! - `jobs.build.outputs.version != ''`

use indexmap::IndexMap;
use met_core::ids::JobId;
use tracing::{debug, warn};

use crate::context::ExecutionContext;
use crate::error::{EngineError, Result};
use crate::state::RunState;

/// Context for CEL evaluation.
pub struct CelContext {
    /// Whether all dependencies succeeded.
    pub all_deps_succeeded: bool,
    /// Whether any dependency failed.
    pub any_dep_failed: bool,
    /// Whether the run was cancelled.
    pub cancelled: bool,
    /// Pipeline variables.
    pub variables: IndexMap<String, String>,
    /// Job outputs keyed by job name.
    pub job_outputs: IndexMap<String, IndexMap<String, String>>,
}

impl CelContext {
    /// Create a CEL context from execution state.
    pub async fn from_state(
        exec_ctx: &ExecutionContext,
        run_state: &RunState,
        dependencies: &[JobId],
    ) -> Self {
        let mut all_deps_succeeded = true;
        let mut any_dep_failed = false;

        for dep_id in dependencies {
            if !run_state.is_job_success(dep_id).await {
                all_deps_succeeded = false;
            }
            if run_state.failed_jobs().await.contains(dep_id) {
                any_dep_failed = true;
            }
        }

        let cancelled = run_state.is_cancellation_requested().await;
        let variables = exec_ctx.variables().await;

        let mut job_outputs = IndexMap::new();
        for job in exec_ctx.pipeline().jobs.iter() {
            if let Some(outputs) = exec_ctx.get_job_outputs(&job.id).await {
                job_outputs.insert(job.name.clone(), outputs);
            }
        }

        Self {
            all_deps_succeeded,
            any_dep_failed,
            cancelled,
            variables,
            job_outputs,
        }
    }
}

/// Evaluate a condition expression.
pub fn evaluate_condition(condition: &str, ctx: &CelContext) -> Result<bool> {
    let condition = condition.trim();

    if condition.is_empty() || condition == "true" {
        return Ok(true);
    }
    if condition == "false" {
        return Ok(false);
    }

    if let Some(result) = evaluate_builtin_function(condition, ctx) {
        return Ok(result);
    }

    match evaluate_cel_expression(condition, ctx) {
        Ok(result) => Ok(result),
        Err(e) => {
            warn!(condition, error = %e, "CEL evaluation failed, defaulting to false");
            Err(EngineError::ConditionEvaluation {
                job: String::new(),
                reason: e.to_string(),
            })
        }
    }
}

fn evaluate_builtin_function(condition: &str, ctx: &CelContext) -> Option<bool> {
    match condition {
        "success()" => Some(ctx.all_deps_succeeded),
        "failure()" => Some(ctx.any_dep_failed),
        "always()" => Some(true),
        "cancelled()" => Some(ctx.cancelled),
        _ => None,
    }
}

fn evaluate_cel_expression(condition: &str, ctx: &CelContext) -> std::result::Result<bool, String> {
    use cel_interpreter::{Context, Program, Value};
    use std::sync::Arc;

    let condition = preprocess_builtin_functions(condition, ctx);

    let program = Program::compile(&condition).map_err(|e| format!("CEL compile error: {e}"))?;

    let mut cel_ctx = Context::default();

    let variables_map: std::collections::HashMap<cel_interpreter::objects::Key, Value> = ctx
        .variables
        .iter()
        .map(|(k, v)| {
            (
                cel_interpreter::objects::Key::String(Arc::from(k.clone())),
                Value::String(Arc::from(v.clone())),
            )
        })
        .collect();
    let _ = cel_ctx.add_variable("variables", Value::Map(variables_map.into()));

    let mut jobs_map: std::collections::HashMap<cel_interpreter::objects::Key, Value> =
        std::collections::HashMap::new();
    for (job_name, outputs) in &ctx.job_outputs {
        let outputs_map: std::collections::HashMap<cel_interpreter::objects::Key, Value> = outputs
            .iter()
            .map(|(k, v)| {
                (
                    cel_interpreter::objects::Key::String(Arc::from(k.clone())),
                    Value::String(Arc::from(v.clone())),
                )
            })
            .collect();
        let job_obj: std::collections::HashMap<cel_interpreter::objects::Key, Value> = [(
            cel_interpreter::objects::Key::String(Arc::from("outputs".to_string())),
            Value::Map(outputs_map.into()),
        )]
        .into_iter()
        .collect();
        jobs_map.insert(
            cel_interpreter::objects::Key::String(Arc::from(job_name.clone())),
            Value::Map(job_obj.into()),
        );
    }
    let _ = cel_ctx.add_variable("jobs", Value::Map(jobs_map.into()));

    let result = program
        .execute(&cel_ctx)
        .map_err(|e| format!("CEL execution error: {e}"))?;

    match result {
        Value::Bool(b) => {
            debug!(condition, result = b, "CEL condition evaluated");
            Ok(b)
        }
        other => Err(format!(
            "CEL expression must return boolean, got: {:?}",
            other
        )),
    }
}

fn preprocess_builtin_functions(condition: &str, ctx: &CelContext) -> String {
    condition
        .replace(
            "success()",
            if ctx.all_deps_succeeded {
                "true"
            } else {
                "false"
            },
        )
        .replace(
            "failure()",
            if ctx.any_dep_failed { "true" } else { "false" },
        )
        .replace("always()", "true")
        .replace("cancelled()", if ctx.cancelled { "true" } else { "false" })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_ctx(all_succeeded: bool, any_failed: bool) -> CelContext {
        CelContext {
            all_deps_succeeded: all_succeeded,
            any_dep_failed: any_failed,
            cancelled: false,
            variables: IndexMap::new(),
            job_outputs: IndexMap::new(),
        }
    }

    #[test]
    fn test_builtin_success() {
        let ctx = test_ctx(true, false);
        assert!(evaluate_condition("success()", &ctx).unwrap());

        let ctx = test_ctx(false, true);
        assert!(!evaluate_condition("success()", &ctx).unwrap());
    }

    #[test]
    fn test_builtin_failure() {
        let ctx = test_ctx(false, true);
        assert!(evaluate_condition("failure()", &ctx).unwrap());

        let ctx = test_ctx(true, false);
        assert!(!evaluate_condition("failure()", &ctx).unwrap());
    }

    #[test]
    fn test_builtin_always() {
        let ctx = test_ctx(false, true);
        assert!(evaluate_condition("always()", &ctx).unwrap());
    }

    #[test]
    fn test_empty_condition() {
        let ctx = test_ctx(false, false);
        assert!(evaluate_condition("", &ctx).unwrap());
        assert!(evaluate_condition("true", &ctx).unwrap());
        assert!(!evaluate_condition("false", &ctx).unwrap());
    }

    #[test]
    fn test_variable_condition() {
        let mut ctx = test_ctx(true, false);
        ctx.variables
            .insert("env".to_string(), "production".to_string());

        assert!(evaluate_condition("variables.env == 'production'", &ctx).unwrap());
        assert!(!evaluate_condition("variables.env == 'staging'", &ctx).unwrap());
    }
}
