//! Pipeline Intermediate Representation (IR).
//!
//! The IR is the fully-resolved, validated representation of a pipeline.
//! All workflow references are inlined, variables are validated, and the DAG
//! is constructed. The engine consumes this IR to execute pipelines.

use indexmap::IndexMap;
use met_core::{JobId, PipelineId, ProjectId, StepId};
use std::time::Duration;

/// Fully-resolved pipeline ready for execution.
#[derive(Debug, Clone)]
pub struct PipelineIR {
    /// Pipeline ID.
    pub id: PipelineId,
    /// Human-readable name.
    pub name: String,
    /// Source file path (if parsed from file).
    pub source_file: Option<String>,
    /// Project this pipeline belongs to.
    pub project_id: Option<ProjectId>,
    /// Trigger configurations.
    pub triggers: Vec<Trigger>,
    /// Plain-text variables.
    pub variables: IndexMap<String, String>,
    /// Secret references (not the actual values).
    pub secret_refs: IndexMap<String, SecretRef>,
    /// Resolved jobs (from expanded workflows).
    pub jobs: Vec<JobIR>,
    /// Default pool selector (can be overridden per-job).
    pub default_pool_selector: Option<PoolSelector>,
    /// When true, `${{ workflows.*.outputs.*}}` may resolve **secret** outputs into plaintext env for dependent jobs.
    pub expose_workflow_secret_outputs: bool,
    /// When true, shared-workspace affinity groups may contain concurrent jobs (S3 snapshot isolation).
    pub allow_parallel_shared_workspace_jobs: bool,
}

impl PipelineIR {
    /// Get a job by ID.
    pub fn get_job(&self, id: &JobId) -> Option<&JobIR> {
        self.jobs.iter().find(|j| &j.id == id)
    }

    /// Get jobs with no dependencies (entry points).
    pub fn entry_jobs(&self) -> impl Iterator<Item = &JobIR> {
        self.jobs.iter().filter(|j| j.depends_on.is_empty())
    }

    /// Get jobs that depend on the given job.
    pub fn dependents(&self, id: &JobId) -> impl Iterator<Item = &JobIR> {
        self.jobs.iter().filter(|j| j.depends_on.contains(id))
    }
}

/// Trigger configuration.
#[derive(Debug, Clone)]
pub enum Trigger {
    /// Manual trigger (always available).
    Manual,
    /// Webhook trigger for SCM events.
    Webhook(WebhookTrigger),
    /// Tag/release trigger.
    Tag(TagTrigger),
    /// Scheduled trigger.
    Schedule(ScheduleTrigger),
}

/// Webhook trigger for SCM events.
#[derive(Debug, Clone)]
pub struct WebhookTrigger {
    /// Events that trigger the pipeline.
    pub events: Vec<WebhookEvent>,
    /// Branch patterns to match.
    pub branches: Vec<String>,
    /// Path patterns that must change to trigger.
    pub paths: Vec<String>,
    /// Path patterns to exclude.
    pub paths_ignore: Vec<String>,
    /// When set (non-empty in YAML), repo sync manages a `triggers` row with this key.
    pub sync_key: Option<String>,
}

/// Webhook event types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebhookEvent {
    Push,
    PullRequest,
    PullRequestReview,
    PullRequestComment,
    Release,
}

/// Tag/release trigger.
#[derive(Debug, Clone)]
pub struct TagTrigger {
    /// Tag patterns to match.
    pub patterns: Vec<String>,
}

/// Scheduled trigger.
#[derive(Debug, Clone)]
pub struct ScheduleTrigger {
    /// Cron expression (UTC).
    pub cron: String,
    /// Timezone override.
    pub timezone: Option<String>,
}

/// Secret reference (resolved but not decrypted).
#[derive(Debug, Clone)]
pub enum SecretRef {
    /// AWS Secrets Manager.
    Aws { arn: String, key: Option<String> },
    /// HashiCorp Vault.
    Vault {
        path: String,
        key: String,
        mount: Option<String>,
    },
    /// Built-in secret store (legacy alias for platform-stored secrets).
    Builtin { name: String },
    /// Platform-stored encrypted secret (`builtin_secrets` table).
    Stored { name: String },
}

/// Agent pool selector.
#[derive(Debug, Clone, Default)]
pub struct PoolSelector {
    /// Tag requirements (all must match).
    pub required_tags: IndexMap<String, TagValue>,
    /// Pool name (alternative to tags).
    pub pool_name: Option<String>,
}

/// Tag value requirement.
#[derive(Debug, Clone)]
pub enum TagValue {
    /// Tag must be present with this boolean value.
    Bool(bool),
    /// Tag must equal this string.
    String(String),
    /// Tag must be present (any value).
    Present,
}

/// Explicit workspace snapshot transfer (ADR-014 `workspace:` on a workflow invocation).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkspaceTransferIR {
    /// Pipeline `workflows[].id` of the invocation whose workspace snapshot to restore.
    pub restore_from_invocation_id: Option<String>,
    /// Non-empty: pack only these paths (relative to workspace root) when uploading a snapshot.
    pub snapshot_include_paths: Vec<String>,
    /// Resolved producer [`JobId`] when `restore_from_invocation_id` is set (filled by the parser).
    pub restore_from_job_id: Option<JobId>,
}

/// Resolved job ready for scheduling.
#[derive(Debug, Clone)]
pub struct JobIR {
    /// Unique job ID.
    pub id: JobId,
    /// Human-readable name.
    pub name: String,
    /// Jobs that must complete before this one.
    pub depends_on: Vec<JobId>,
    /// Pool selector for agent matching.
    pub pool_selector: PoolSelector,
    /// Steps to execute in order.
    pub steps: Vec<StepIR>,
    /// Sidecar services.
    pub services: Vec<ServiceDef>,
    /// Maximum execution time.
    pub timeout: Duration,
    /// Retry policy.
    pub retry_policy: Option<RetryPolicy>,
    /// Cache configuration.
    pub cache_config: Option<CacheConfig>,
    /// Conditional execution (CEL expression).
    pub condition: Option<String>,
    /// Reference to the source workflow for traceability.
    pub source_workflow: Option<WorkflowRef>,
    /// Job-level environment variables.
    pub env: IndexMap<String, EnvValue>,
    /// Effective same-agent affinity group (pipeline default or invocation override).
    pub affinity_group: Option<String>,
    /// When true, this job participates in workspace snapshot restore/upload for the run (pipeline
    /// `agent-affinity.share-workspace`). Legacy shared-disk mode also uses [`Self::affinity_group`]
    /// (or an internal default partition when it is unset).
    pub share_workspace: bool,
    /// Pipeline `workflows[].id` when this job was expanded from a reusable workflow.
    pub workflow_invocation_id: Option<String>,
    /// Pipeline `workflows[].name` when this job was expanded from a reusable workflow.
    pub workflow_invocation_name: Option<String>,
    /// Optional explicit workspace restore / subset snapshot (ADR-014).
    pub workspace_transfer: Option<WorkspaceTransferIR>,
}

impl JobIR {
    /// Get a step by ID.
    pub fn get_step(&self, id: &StepId) -> Option<&StepIR> {
        self.steps.iter().find(|s| &s.id == id)
    }
}

/// Resolved step ready for execution.
#[derive(Debug, Clone)]
pub struct StepIR {
    /// Unique step ID.
    pub id: StepId,
    /// Human-readable name.
    pub name: String,
    /// Command to execute.
    pub command: StepCommand,
    /// Environment variables.
    pub env: IndexMap<String, EnvValue>,
    /// Working directory (relative to workspace).
    pub working_directory: Option<String>,
    /// Maximum execution time.
    pub timeout: Duration,
    /// Whether to continue if this step fails.
    pub continue_on_error: bool,
}

/// Step command type.
#[derive(Debug, Clone)]
pub enum StepCommand {
    /// Shell command execution.
    Run {
        /// Shell to use.
        shell: Shell,
        /// Script content.
        script: String,
    },
    /// Built-in action execution.
    Action {
        /// Action name.
        name: String,
        /// Action version.
        version: String,
        /// Action inputs.
        inputs: IndexMap<String, String>,
    },
}

/// Shell type for running commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Shell {
    #[default]
    Bash,
    Sh,
    Powershell,
    Pwsh,
    Cmd,
    Python,
}

/// Returned when [`FromStr`](std::str::FromStr) parsing does not recognize the shell name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseShellError;

impl std::str::FromStr for Shell {
    type Err = ParseShellError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "bash" => Ok(Shell::Bash),
            "sh" => Ok(Shell::Sh),
            "powershell" => Ok(Shell::Powershell),
            "pwsh" => Ok(Shell::Pwsh),
            "cmd" => Ok(Shell::Cmd),
            "python" => Ok(Shell::Python),
            _ => Err(ParseShellError),
        }
    }
}

impl Shell {
    /// Get the default shell for the current platform.
    pub fn platform_default() -> Self {
        #[cfg(windows)]
        {
            Shell::Powershell
        }
        #[cfg(not(windows))]
        {
            Shell::Bash
        }
    }
}

/// Environment variable value.
#[derive(Debug, Clone)]
pub enum EnvValue {
    /// Literal string value.
    Literal(String),
    /// Reference to a secret.
    SecretRef(String),
    /// Expression to evaluate at runtime.
    Expression(String),
}

/// Service (sidecar container) definition.
#[derive(Debug, Clone)]
pub struct ServiceDef {
    /// Service name (used for networking).
    pub name: String,
    /// Container image.
    pub image: String,
    /// Exposed ports.
    pub ports: Vec<u16>,
    /// Environment variables.
    pub env: IndexMap<String, String>,
    /// Command override.
    pub command: Option<Vec<String>>,
    /// Health check configuration.
    pub health_check: Option<HealthCheck>,
}

/// Health check for services.
#[derive(Debug, Clone)]
pub struct HealthCheck {
    /// Check method.
    pub method: HealthCheckMethod,
    /// Interval between checks.
    pub interval: Duration,
    /// Timeout for each check.
    pub timeout: Duration,
    /// Retries before marking unhealthy.
    pub retries: u32,
}

/// Health check method.
#[derive(Debug, Clone)]
pub enum HealthCheckMethod {
    /// Run a command.
    Cmd(Vec<String>),
    /// HTTP GET request.
    Http(String),
    /// TCP connection check.
    Tcp(u16),
}

/// Retry policy.
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// Maximum number of attempts (including initial).
    pub max_attempts: u32,
    /// Backoff duration between retries.
    pub backoff: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            backoff: Duration::from_secs(10),
        }
    }
}

/// Cache configuration.
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Cache key template.
    pub key: String,
    /// Paths to cache.
    pub paths: Vec<String>,
    /// Fallback keys for partial matches.
    pub restore_keys: Vec<String>,
}

/// Reference to a reusable workflow.
#[derive(Debug, Clone)]
pub struct WorkflowRef {
    /// Workflow scope.
    pub scope: WorkflowScope,
    /// Workflow name.
    pub name: String,
    /// Version used.
    pub version: String,
}

/// Workflow scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowScope {
    /// Global workflow (managed by platform admins).
    Global,
    /// Project workflow (in repo).
    Project,
}

impl std::fmt::Display for WorkflowRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let scope = match self.scope {
            WorkflowScope::Global => "global",
            WorkflowScope::Project => "project",
        };
        write!(f, "{}/{}@{}", scope, self.name, self.version)
    }
}

/// Default timeout values.
pub mod defaults {
    use std::time::Duration;

    /// Default job timeout (1 hour).
    pub const JOB_TIMEOUT: Duration = Duration::from_secs(3600);

    /// Default step timeout (30 minutes).
    pub const STEP_TIMEOUT: Duration = Duration::from_secs(1800);

    /// Maximum workflow nesting depth.
    pub const MAX_WORKFLOW_DEPTH: usize = 5;
}
