//! Pipeline parser implementation.
//!
//! Orchestrates the 6-stage parsing pipeline:
//! 1. Deserialize YAML
//! 2. Schema validation
//! 3. Workflow resolution
//! 4. Variable resolution
//! 5. DAG construction
//! 6. IR emission

use crate::dag::{build_dag, DagNode};
use crate::error::{ErrorCode, ParseDiagnostics, ParseError, SourceLocation};
use crate::ir::{
    defaults, CacheConfig, EnvValue, HealthCheck, HealthCheckMethod, JobIR, PipelineIR,
    PoolSelector, RetryPolicy, ScheduleTrigger, SecretRef, ServiceDef, Shell, StepCommand,
    StepIR, TagTrigger, TagValue, Trigger, WebhookEvent, WebhookTrigger, WorkflowRef,
};
use crate::schema::{
    RawCacheConfig, RawHealthCheck, RawJob, RawPipeline, RawPoolSelector, RawRetryPolicy,
    RawSecretRef, RawService, RawStep, RawWorkflowDef, RawWorkflowInvocation,
};
use crate::span::{SpanTracker, SpannedYamlParser};
use crate::variable::VariableContext;
use crate::workflow::{WorkflowProvider, WorkflowResolver};
use indexmap::IndexMap;
use met_core::{JobId, PipelineId, StepId};
use std::collections::HashSet;
use tracing::{debug, instrument};

/// Parser configuration.
#[derive(Debug, Clone)]
pub struct ParserConfig {
    /// Enable strict mode (treat warnings as errors).
    pub strict: bool,
    /// Maximum number of errors before aborting.
    pub max_errors: usize,
    /// Source file path for error reporting.
    pub source_file: Option<String>,
}

impl Default for ParserConfig {
    fn default() -> Self {
        Self {
            strict: false,
            max_errors: 100,
            source_file: None,
        }
    }
}

/// Pipeline parser.
pub struct PipelineParser<'a> {
    config: ParserConfig,
    provider: &'a dyn WorkflowProvider,
    /// Span tracker for source location resolution
    span_tracker: Option<SpanTracker>,
}

impl<'a> PipelineParser<'a> {
    /// Create a new parser with the given workflow provider.
    pub fn new(provider: &'a dyn WorkflowProvider) -> Self {
        Self {
            config: ParserConfig::default(),
            provider,
            span_tracker: None,
        }
    }

    /// Set parser configuration.
    pub fn with_config(mut self, config: ParserConfig) -> Self {
        self.config = config;
        self
    }

    /// Get a source location for a key, with fallback to unknown location.
    fn get_location(&self, key: &str) -> SourceLocation {
        self.span_tracker
            .as_ref()
            .and_then(|t| t.get_span(key).cloned())
            .unwrap_or_else(|| {
                let mut loc = SourceLocation::unknown();
                if let Some(file) = &self.config.source_file {
                    loc = loc.with_file(file.clone());
                }
                loc
            })
    }

    /// Get a source location for a workflow invocation by index.
    fn get_workflow_location(&self, idx: usize, workflow_id: &str) -> SourceLocation {
        // Try specific workflow key first, then fall back to index-based or generic
        self.span_tracker
            .as_ref()
            .and_then(|t| {
                t.get_span(workflow_id)
                    .or_else(|| t.get_span(&format!("workflows[{}]", idx)))
                    .or_else(|| t.get_span("workflows"))
                    .cloned()
            })
            .unwrap_or_else(|| {
                let mut loc = SourceLocation::unknown();
                if let Some(file) = &self.config.source_file {
                    loc = loc.with_file(file.clone());
                }
                loc
            })
    }

    /// Parse a pipeline from YAML string.
    #[instrument(skip(self, yaml), fields(source = ?self.config.source_file))]
    pub async fn parse(&mut self, yaml: &str) -> Result<PipelineIR, Vec<ParseError>> {
        let mut diagnostics = ParseDiagnostics::new();

        // Stage 1: Deserialize YAML with span tracking
        debug!("stage 1: deserializing YAML");
        let raw_pipeline = match self.deserialize(yaml, &mut diagnostics) {
            Some(p) => p,
            None => return Err(diagnostics.into_iter().collect()),
        };

        // Stage 2: Schema validation
        debug!("stage 2: validating schema");
        self.validate_schema(&raw_pipeline, &mut diagnostics);

        if diagnostics.has_errors() && diagnostics.errors().count() >= self.config.max_errors {
            return Err(diagnostics.into_iter().collect());
        }

        // Stage 3: Workflow resolution
        debug!("stage 3: resolving workflows");
        let resolved_jobs = self
            .resolve_workflows(&raw_pipeline, &mut diagnostics)
            .await;

        if diagnostics.has_errors() && diagnostics.errors().count() >= self.config.max_errors {
            return Err(diagnostics.into_iter().collect());
        }

        // Stage 4: Variable resolution
        debug!("stage 4: resolving variables");
        let var_ctx = self.build_variable_context(&raw_pipeline);
        self.validate_variables(&resolved_jobs, &var_ctx, &mut diagnostics);

        if diagnostics.has_errors() && diagnostics.errors().count() >= self.config.max_errors {
            return Err(diagnostics.into_iter().collect());
        }

        // Stage 5: DAG construction
        debug!("stage 5: constructing DAG");
        let dag_nodes = self.build_dag_nodes(&resolved_jobs);
        let _dag = build_dag(&dag_nodes, &mut diagnostics);

        if diagnostics.has_errors() {
            return Err(diagnostics.into_iter().collect());
        }

        // Stage 6: Emit IR
        debug!("stage 6: emitting IR");
        let ir = self.emit_ir(&raw_pipeline, resolved_jobs, &var_ctx);

        if self.config.strict && diagnostics.warnings().count() > 0 {
            return Err(diagnostics.into_iter().collect());
        }

        Ok(ir)
    }

    /// Stage 1: Deserialize YAML with span tracking.
    fn deserialize(
        &mut self,
        yaml: &str,
        diagnostics: &mut ParseDiagnostics,
    ) -> Option<RawPipeline> {
        let mut parser = if let Some(file) = &self.config.source_file {
            SpannedYamlParser::with_file(file.clone())
        } else {
            SpannedYamlParser::new()
        };

        match parser.parse::<RawPipeline>(yaml) {
            Ok((pipeline, tracker)) => {
                // Clone the span tracker for use in later stages
                self.span_tracker = Some(SpanTracker::from_existing(tracker));
                Some(pipeline)
            }
            Err(e) => {
                let mut error = ParseError::new(ErrorCode::E1001, e.message);
                if let Some(loc) = e.location {
                    error = error.with_source(loc);
                }
                diagnostics.push(error);
                None
            }
        }
    }

    /// Stage 2: Schema validation.
    fn validate_schema(&self, pipeline: &RawPipeline, diagnostics: &mut ParseDiagnostics) {
        // Validate pipeline name
        if pipeline.name.is_empty() {
            diagnostics.error_at(
                ErrorCode::E2001,
                "pipeline name is required",
                self.get_location("name"),
            );
        }

        // Validate workflow invocations
        let mut seen_ids: HashSet<&str> = HashSet::new();
        for (idx, workflow) in pipeline.workflows.iter().enumerate() {
            let location = self.get_workflow_location(idx, &workflow.id);

            // Check required fields
            if workflow.id.is_empty() {
                diagnostics.error_at(
                    ErrorCode::E2001,
                    format!("workflow {} is missing 'id' field", idx),
                    location.clone(),
                );
            }

            if workflow.workflow.is_empty() {
                diagnostics.error_at(
                    ErrorCode::E2001,
                    format!("workflow '{}' is missing 'workflow' field", workflow.id),
                    location.clone(),
                );
            }

            // Check for duplicate IDs
            if !workflow.id.is_empty() {
                if seen_ids.contains(workflow.id.as_str()) {
                    diagnostics.error_at(
                        ErrorCode::E2005,
                        format!("duplicate workflow ID: {}", workflow.id),
                        location.clone(),
                    );
                } else {
                    seen_ids.insert(&workflow.id);
                }
            }

            // Validate ID format (alphanumeric + hyphens/underscores)
            if !workflow.id.is_empty() && !is_valid_id(&workflow.id) {
                diagnostics.error_at(
                    ErrorCode::E2006,
                    format!(
                        "invalid workflow ID '{}': must be alphanumeric with hyphens/underscores",
                        workflow.id
                    ),
                    location.clone(),
                );
            }

            // Validate timeout
            if let Some(timeout) = workflow.timeout {
                if timeout.as_secs() == 0 {
                    diagnostics.warning(
                        ErrorCode::E2007,
                        format!("workflow '{}' has zero timeout", workflow.id),
                    );
                }
            }

            // Validate retry policy
            if let Some(retry) = &workflow.retry {
                if retry.max_attempts == 0 {
                    diagnostics.error_at(
                        ErrorCode::E2007,
                        format!(
                            "workflow '{}' retry max_attempts must be at least 1",
                            workflow.id
                        ),
                        location.clone(),
                    );
                }
            }
        }

        // Validate secrets
        for (name, _secret) in &pipeline.secrets {
            if !is_valid_id(name) {
                diagnostics.error_at(
                    ErrorCode::E2006,
                    format!("invalid secret name '{}': must be alphanumeric with underscores", name),
                    self.get_location(name),
                );
            }
        }

        // Validate variables
        for (name, _value) in &pipeline.vars {
            if !is_valid_id(name) {
                diagnostics.error_at(
                    ErrorCode::E2006,
                    format!(
                        "invalid variable name '{}': must be alphanumeric with underscores",
                        name
                    ),
                    self.get_location(name),
                );
            }
        }
    }

    /// Stage 3: Resolve workflows.
    async fn resolve_workflows(
        &self,
        pipeline: &RawPipeline,
        diagnostics: &mut ParseDiagnostics,
    ) -> Vec<ResolvedWorkflow> {
        let mut resolver = WorkflowResolver::new(self.provider);
        let mut resolved = Vec::new();

        for (idx, invocation) in pipeline.workflows.iter().enumerate() {
            let location = self.get_workflow_location(idx, &invocation.id);

            if let Some((workflow_def, workflow_ref)) = resolver
                .resolve(
                    &invocation.workflow,
                    invocation.version.as_deref(),
                    diagnostics,
                    location.clone(),
                )
                .await
            {
                // Validate inputs match workflow definition
                for (input_name, _) in &invocation.inputs {
                    if !workflow_def.inputs.contains_key(input_name) {
                        diagnostics.warning(
                            ErrorCode::E2003,
                            format!(
                                "unknown input '{}' for workflow '{}'",
                                input_name, invocation.id
                            ),
                        );
                    }
                }

                // Check required inputs are provided
                for (input_name, input_def) in &workflow_def.inputs {
                    if input_def.required
                        && input_def.default.is_none()
                        && !invocation.inputs.contains_key(input_name)
                    {
                        diagnostics.error(
                            ErrorCode::E2001,
                            format!(
                                "required input '{}' not provided for workflow '{}'",
                                input_name, invocation.id
                            ),
                        );
                    }
                }

                resolved.push(ResolvedWorkflow {
                    invocation: invocation.clone(),
                    definition: workflow_def,
                    workflow_ref,
                });
            }
        }

        resolved
    }

    /// Build variable context for validation.
    fn build_variable_context(&self, pipeline: &RawPipeline) -> VariableContext {
        let secrets: HashSet<String> = pipeline.secrets.keys().cloned().collect();
        VariableContext::new(pipeline.vars.clone(), secrets)
    }

    /// Stage 4: Validate variable references.
    fn validate_variables(
        &self,
        workflows: &[ResolvedWorkflow],
        ctx: &VariableContext,
        diagnostics: &mut ParseDiagnostics,
    ) {
        for (idx, resolved) in workflows.iter().enumerate() {
            let workflow_location = self.get_workflow_location(idx, &resolved.invocation.id);

            // Build context with workflow inputs
            let inputs: IndexMap<String, String> = resolved
                .invocation
                .inputs
                .iter()
                .map(|(k, v)| {
                    let value = match v {
                        serde_yaml::Value::String(s) => s.clone(),
                        other => serde_yaml::to_string(other).unwrap_or_default(),
                    };
                    (k.clone(), value)
                })
                .collect();

            let workflow_ctx = VariableContext::new(ctx.vars.clone(), ctx.secrets.clone())
                .with_inputs(inputs);

            // Validate input values
            for (name, value) in &resolved.invocation.inputs {
                if let serde_yaml::Value::String(s) = value {
                    let loc = self.get_location(name).clone();
                    let loc = if loc.line == 0 { workflow_location.clone() } else { loc };
                    crate::variable::validate_refs(s, &workflow_ctx, diagnostics, loc);
                }
            }

            // Validate condition
            if let Some(condition) = &resolved.invocation.condition {
                let loc = self.get_location("condition");
                let loc = if loc.line == 0 { workflow_location.clone() } else { loc };
                crate::variable::validate_refs(condition, &workflow_ctx, diagnostics, loc);
            }

            // Validate cache key
            if let Some(cache) = &resolved.invocation.cache {
                let loc = self.get_location("cache");
                let loc = if loc.line == 0 { workflow_location.clone() } else { loc };
                crate::variable::validate_refs(&cache.key, &workflow_ctx, diagnostics, loc);
            }

            // Validate steps in workflow definition
            for job in &resolved.definition.jobs {
                let job_location = self.get_location(&job.id);
                let job_location = if job_location.line == 0 { workflow_location.clone() } else { job_location };

                for step in &job.steps {
                    let step_location = step
                        .id
                        .as_ref()
                        .map(|id| self.get_location(id))
                        .unwrap_or_else(|| job_location.clone());
                    let step_location = if step_location.line == 0 { job_location.clone() } else { step_location };

                    // Validate run command
                    if let Some(run) = &step.run {
                        crate::variable::validate_refs(run, &workflow_ctx, diagnostics, step_location.clone());
                    }

                    // Validate env values
                    for value in step.env.values() {
                        crate::variable::validate_refs(value, &workflow_ctx, diagnostics, step_location.clone());
                    }
                }
            }
        }
    }

    /// Build DAG nodes from resolved workflows.
    fn build_dag_nodes(&self, workflows: &[ResolvedWorkflow]) -> Vec<DagNode> {
        workflows
            .iter()
            .enumerate()
            .map(|(idx, w)| DagNode {
                id: w.invocation.id.clone(),
                name: w.invocation.name.clone(),
                depends_on: w.invocation.depends_on.clone(),
                source: self.get_workflow_location(idx, &w.invocation.id),
            })
            .collect()
    }

    /// Stage 6: Emit Pipeline IR.
    fn emit_ir(
        &self,
        pipeline: &RawPipeline,
        workflows: Vec<ResolvedWorkflow>,
        _ctx: &VariableContext,
    ) -> PipelineIR {
        let triggers = self.convert_triggers(&pipeline.triggers);
        let secret_refs = self.convert_secrets(&pipeline.secrets);
        let default_pool = pipeline.runs_on.as_ref().map(|p| self.convert_pool_selector(p));

        let jobs: Vec<JobIR> = workflows
            .into_iter()
            .flat_map(|w| self.expand_workflow_to_jobs(w, default_pool.clone()))
            .collect();

        PipelineIR {
            id: PipelineId::new(),
            name: pipeline.name.clone(),
            source_file: self.config.source_file.clone(),
            project_id: None,
            triggers,
            variables: pipeline.vars.clone(),
            secret_refs,
            jobs,
            default_pool_selector: default_pool,
        }
    }

    /// Convert raw triggers to IR.
    fn convert_triggers(&self, triggers: &crate::schema::RawTriggers) -> Vec<Trigger> {
        let mut result = Vec::new();

        if triggers.manual.is_some() {
            result.push(Trigger::Manual);
        }

        if let Some(webhook) = &triggers.webhook {
            result.push(Trigger::Webhook(WebhookTrigger {
                events: webhook
                    .events
                    .iter()
                    .filter_map(|e| match e.as_str() {
                        "push" => Some(WebhookEvent::Push),
                        "pull_request" => Some(WebhookEvent::PullRequest),
                        "pull_request_review" => Some(WebhookEvent::PullRequestReview),
                        "pull_request_comment" => Some(WebhookEvent::PullRequestComment),
                        "release" => Some(WebhookEvent::Release),
                        _ => None,
                    })
                    .collect(),
                branches: webhook.branches.clone(),
                paths: webhook.paths.clone(),
                paths_ignore: webhook.paths_ignore.clone(),
            }));
        }

        if let Some(tag) = &triggers.tag {
            result.push(Trigger::Tag(TagTrigger {
                patterns: tag.patterns.clone(),
            }));
        }

        if let Some(release) = &triggers.release {
            result.push(Trigger::Tag(TagTrigger {
                patterns: release.tag.clone(),
            }));
        }

        if let Some(schedule) = &triggers.schedule {
            result.push(Trigger::Schedule(ScheduleTrigger {
                cron: schedule.cron.clone(),
                timezone: schedule.timezone.clone(),
            }));
        }

        result
    }

    /// Convert raw secrets to IR.
    fn convert_secrets(&self, secrets: &IndexMap<String, RawSecretRef>) -> IndexMap<String, SecretRef> {
        secrets
            .iter()
            .filter_map(|(name, raw)| {
                let secret_ref = if let Some(aws) = &raw.aws {
                    SecretRef::Aws {
                        arn: aws.arn.clone(),
                        key: aws.key.clone(),
                    }
                } else if let Some(vault) = &raw.vault {
                    SecretRef::Vault {
                        path: vault.path.clone(),
                        key: vault.key.clone(),
                        mount: vault.mount.clone(),
                    }
                } else if let Some(stored) = &raw.stored {
                    SecretRef::Stored {
                        name: stored.name.clone(),
                    }
                } else if let Some(builtin) = &raw.builtin {
                    SecretRef::Builtin {
                        name: builtin.name.clone(),
                    }
                } else {
                    return None;
                };
                Some((name.clone(), secret_ref))
            })
            .collect()
    }

    /// Convert pool selector.
    fn convert_pool_selector(&self, raw: &RawPoolSelector) -> PoolSelector {
        let mut required_tags = IndexMap::new();

        for tag_map in &raw.tags {
            for (key, value) in tag_map {
                let tag_value = match value {
                    serde_yaml::Value::Bool(b) => TagValue::Bool(*b),
                    serde_yaml::Value::String(s) => TagValue::String(s.clone()),
                    serde_yaml::Value::Null => TagValue::Present,
                    _ => continue,
                };
                required_tags.insert(key.clone(), tag_value);
            }
        }

        PoolSelector {
            required_tags,
            pool_name: raw.pool.clone(),
        }
    }

    /// Expand a resolved workflow into jobs.
    fn expand_workflow_to_jobs(
        &self,
        workflow: ResolvedWorkflow,
        default_pool: Option<PoolSelector>,
    ) -> Vec<JobIR> {
        let workflow_prefix = &workflow.invocation.id;

        workflow
            .definition
            .jobs
            .iter()
            .map(|job| {
                let job_id = format!("{}_{}", workflow_prefix, job.id);
                let pool = job
                    .runs_on
                    .as_ref()
                    .map(|p| self.convert_pool_selector(p))
                    .or(default_pool.clone())
                    .unwrap_or_default();

                let steps: Vec<StepIR> = job
                    .steps
                    .iter()
                    .enumerate()
                    .map(|(idx, step)| self.convert_step(step, idx))
                    .collect();

                let services: Vec<ServiceDef> = job
                    .services
                    .iter()
                    .map(|s| self.convert_service(s))
                    .collect();

                let depends_on: Vec<JobId> = job
                    .depends_on
                    .iter()
                    .map(|dep| make_job_id(&format!("{}_{}", workflow_prefix, dep)))
                    .chain(
                        workflow
                            .invocation
                            .depends_on
                            .iter()
                            .map(|dep| make_job_id(dep)),
                    )
                    .collect();

                JobIR {
                    id: make_job_id(&job_id),
                    name: job.name.clone(),
                    depends_on,
                    pool_selector: pool,
                    steps,
                    services,
                    timeout: job.timeout.unwrap_or(
                        workflow
                            .invocation
                            .timeout
                            .unwrap_or(defaults::JOB_TIMEOUT),
                    ),
                    retry_policy: job
                        .retry
                        .as_ref()
                        .or(workflow.invocation.retry.as_ref())
                        .map(|r| self.convert_retry_policy(r)),
                    cache_config: workflow
                        .invocation
                        .cache
                        .as_ref()
                        .map(|c| self.convert_cache_config(c)),
                    condition: job
                        .condition
                        .clone()
                        .or(workflow.invocation.condition.clone()),
                    source_workflow: Some(workflow.workflow_ref.clone()),
                    env: IndexMap::new(),
                }
            })
            .collect()
    }

    /// Convert a raw step to IR.
    fn convert_step(&self, step: &RawStep, idx: usize) -> StepIR {
        let step_id = step
            .id
            .clone()
            .unwrap_or_else(|| format!("step_{}", idx));

        let command = if let Some(run) = &step.run {
            let shell = step
                .shell
                .as_deref()
                .and_then(Shell::from_str)
                .unwrap_or(Shell::platform_default());
            StepCommand::Run {
                shell,
                script: run.clone(),
            }
        } else if let Some(uses) = &step.uses {
            let parts: Vec<&str> = uses.split('@').collect();
            let (name, version) = if parts.len() == 2 {
                (parts[0].to_string(), parts[1].to_string())
            } else {
                (uses.clone(), "latest".to_string())
            };

            let inputs: IndexMap<String, String> = step
                .action_inputs
                .iter()
                .map(|(k, v)| {
                    let value = match v {
                        serde_yaml::Value::String(s) => s.clone(),
                        other => serde_yaml::to_string(other).unwrap_or_default(),
                    };
                    (k.clone(), value)
                })
                .collect();

            StepCommand::Action {
                name,
                version,
                inputs,
            }
        } else {
            StepCommand::Run {
                shell: Shell::platform_default(),
                script: String::new(),
            }
        };

        let env: IndexMap<String, EnvValue> = step
            .env
            .iter()
            .map(|(k, v)| {
                let value = if crate::variable::has_refs(v) {
                    EnvValue::Expression(v.clone())
                } else {
                    EnvValue::Literal(v.clone())
                };
                (k.clone(), value)
            })
            .collect();

        StepIR {
            id: make_step_id(&step_id),
            name: step.name.clone(),
            command,
            env,
            working_directory: step.working_directory.clone(),
            timeout: step.timeout.unwrap_or(defaults::STEP_TIMEOUT),
            continue_on_error: step.continue_on_error,
        }
    }

    /// Convert a raw service to IR.
    fn convert_service(&self, service: &RawService) -> ServiceDef {
        ServiceDef {
            name: service.name.clone(),
            image: service.image.clone(),
            ports: service.ports.clone(),
            env: service.env.clone(),
            command: service.command.clone(),
            health_check: service.health_check.as_ref().map(|h| self.convert_health_check(h)),
        }
    }

    /// Convert health check configuration.
    fn convert_health_check(&self, raw: &RawHealthCheck) -> HealthCheck {
        let method = if let Some(cmd) = &raw.cmd {
            HealthCheckMethod::Cmd(cmd.clone())
        } else if let Some(http) = &raw.http {
            HealthCheckMethod::Http(http.clone())
        } else if let Some(tcp) = raw.tcp {
            HealthCheckMethod::Tcp(tcp)
        } else {
            HealthCheckMethod::Cmd(vec!["true".to_string()])
        };

        HealthCheck {
            method,
            interval: raw.interval.unwrap_or(std::time::Duration::from_secs(10)),
            timeout: raw.timeout.unwrap_or(std::time::Duration::from_secs(5)),
            retries: raw.retries.unwrap_or(3),
        }
    }

    /// Convert retry policy.
    fn convert_retry_policy(&self, raw: &RawRetryPolicy) -> RetryPolicy {
        RetryPolicy {
            max_attempts: raw.max_attempts,
            backoff: raw.backoff.unwrap_or(std::time::Duration::from_secs(10)),
        }
    }

    /// Convert cache configuration.
    fn convert_cache_config(&self, raw: &RawCacheConfig) -> CacheConfig {
        CacheConfig {
            key: raw.key.clone(),
            paths: raw.paths.clone(),
            restore_keys: raw.restore_keys.clone(),
        }
    }
}

/// A resolved workflow with its definition.
struct ResolvedWorkflow {
    invocation: RawWorkflowInvocation,
    definition: RawWorkflowDef,
    workflow_ref: WorkflowRef,
}

/// Create a JobId from a string identifier.
/// This creates a deterministic UUID based on the string for consistent IDs.
fn make_job_id(s: &str) -> JobId {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    
    let mut hasher = DefaultHasher::new();
    s.hash(&mut hasher);
    let hash = hasher.finish();
    
    // Create a UUID-like value from the hash (using v4 format with custom bits)
    let bytes = [
        (hash >> 56) as u8,
        (hash >> 48) as u8,
        (hash >> 40) as u8,
        (hash >> 32) as u8,
        (hash >> 24) as u8,
        (hash >> 16) as u8,
        (hash >> 8) as u8,
        hash as u8,
        (hash >> 56) as u8 ^ 0x40, // Version 4
        (hash >> 48) as u8 | 0x80, // Variant
        (hash >> 40) as u8,
        (hash >> 32) as u8,
        (hash >> 24) as u8,
        (hash >> 16) as u8,
        (hash >> 8) as u8,
        hash as u8,
    ];
    
    JobId::from_uuid(uuid::Uuid::from_bytes(bytes))
}

/// Create a StepId from a string identifier.
fn make_step_id(s: &str) -> StepId {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    
    let mut hasher = DefaultHasher::new();
    s.hash(&mut hasher);
    let hash = hasher.finish();
    
    let bytes = [
        (hash >> 56) as u8,
        (hash >> 48) as u8,
        (hash >> 40) as u8,
        (hash >> 32) as u8,
        (hash >> 24) as u8,
        (hash >> 16) as u8,
        (hash >> 8) as u8,
        hash as u8,
        (hash >> 56) as u8 ^ 0x40,
        (hash >> 48) as u8 | 0x80,
        (hash >> 40) as u8,
        (hash >> 32) as u8,
        (hash >> 24) as u8,
        (hash >> 16) as u8,
        (hash >> 8) as u8,
        hash as u8,
    ];
    
    StepId::from_uuid(uuid::Uuid::from_bytes(bytes))
}

/// Check if a string is a valid identifier.
fn is_valid_id(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }

    let mut chars = s.chars();
    let first = chars.next().unwrap();
    if !first.is_ascii_alphabetic() && first != '_' {
        return false;
    }

    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::MockWorkflowProvider;

    fn mock_workflow() -> RawWorkflowDef {
        RawWorkflowDef {
            name: "Test Workflow".to_string(),
            description: None,
            version: Some("1.0.0".to_string()),
            inputs: IndexMap::new(),
            outputs: IndexMap::new(),
            jobs: vec![RawJob {
                name: "Test Job".to_string(),
                id: "test".to_string(),
                runs_on: None,
                steps: vec![RawStep {
                    name: "Test Step".to_string(),
                    id: Some("step1".to_string()),
                    run: Some("echo hello".to_string()),
                    shell: None,
                    uses: None,
                    action_inputs: IndexMap::new(),
                    env: IndexMap::new(),
                    working_directory: None,
                    timeout: None,
                    continue_on_error: false,
                }],
                services: vec![],
                depends_on: vec![],
                condition: None,
                timeout: None,
                retry: None,
            }],
        }
    }

    #[tokio::test]
    async fn test_parse_simple_pipeline() {
        let yaml = r#"
name: Test Pipeline
triggers:
  manual: {}
vars:
  VERSION: "1.0"
workflows:
  - name: Build
    id: build
    workflow: global/test
    version: v1.0
"#;

        let mut provider = MockWorkflowProvider::new();
        provider.add_workflow(
            crate::ir::WorkflowScope::Global,
            "test",
            mock_workflow(),
        );

        let mut parser = PipelineParser::new(&provider);
        let result = parser.parse(yaml).await;

        assert!(result.is_ok(), "parse error: {:?}", result.err());
        let ir = result.unwrap();
        assert_eq!(ir.name, "Test Pipeline");
        assert_eq!(ir.variables.get("VERSION"), Some(&"1.0".to_string()));
        assert_eq!(ir.jobs.len(), 1);
    }

    #[tokio::test]
    async fn test_parse_with_dependencies() {
        let yaml = r#"
name: Test Pipeline
triggers:
  manual: {}
workflows:
  - name: Build
    id: build
    workflow: global/test
  - name: Deploy
    id: deploy
    workflow: global/test
    depends-on: [build]
"#;

        let mut provider = MockWorkflowProvider::new();
        provider.add_workflow(
            crate::ir::WorkflowScope::Global,
            "test",
            mock_workflow(),
        );

        let mut parser = PipelineParser::new(&provider);
        let result = parser.parse(yaml).await;

        assert!(result.is_ok());
        let ir = result.unwrap();
        assert_eq!(ir.jobs.len(), 2);
    }

    #[tokio::test]
    async fn test_parse_cyclic_dependency() {
        let yaml = r#"
name: Test Pipeline
triggers:
  manual: {}
workflows:
  - name: A
    id: a
    workflow: global/test
    depends-on: [c]
  - name: B
    id: b
    workflow: global/test
    depends-on: [a]
  - name: C
    id: c
    workflow: global/test
    depends-on: [b]
"#;

        let mut provider = MockWorkflowProvider::new();
        provider.add_workflow(
            crate::ir::WorkflowScope::Global,
            "test",
            mock_workflow(),
        );

        let mut parser = PipelineParser::new(&provider);
        let result = parser.parse(yaml).await;

        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.code == ErrorCode::E5001));
    }

    #[tokio::test]
    async fn test_parse_missing_workflow() {
        let yaml = r#"
name: Test Pipeline
triggers:
  manual: {}
workflows:
  - name: Build
    id: build
    workflow: global/nonexistent
"#;

        let provider = MockWorkflowProvider::new();
        let mut parser = PipelineParser::new(&provider);
        let result = parser.parse(yaml).await;

        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.code == ErrorCode::E3001));
    }

    #[test]
    fn test_is_valid_id() {
        assert!(is_valid_id("build"));
        assert!(is_valid_id("build_test"));
        assert!(is_valid_id("build-test"));
        assert!(is_valid_id("_private"));
        assert!(is_valid_id("Build123"));
        assert!(!is_valid_id(""));
        assert!(!is_valid_id("123start"));
        assert!(!is_valid_id("has space"));
        assert!(!is_valid_id("has.dot"));
    }
}
