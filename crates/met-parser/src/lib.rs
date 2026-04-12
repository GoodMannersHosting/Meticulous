//! Pipeline definition parsing for Meticulous CI/CD.
//!
//! This crate handles YAML pipeline definition parsing, schema validation,
//! workflow resolution, and DAG construction.
//!
//! # Architecture
//!
//! The parser follows a 6-stage pipeline:
//!
//! 1. **Deserialize** - Parse YAML into raw AST structs
//! 2. **Schema Validate** - Check required fields, types, unknown keys
//! 3. **Workflow Resolution** - Fetch and inline reusable workflow references
//! 4. **Variable Resolution** - Validate `${...}` token references
//! 5. **DAG Construction** - Build dependency graph, detect cycles
//! 6. **Emit IR** - Produce fully-resolved Pipeline IR
//!
//! # Usage
//!
//! ```ignore
//! use met_parser::{PipelineParser, MockWorkflowProvider};
//!
//! let provider = MockWorkflowProvider::new();
//! let parser = PipelineParser::new(&provider);
//!
//! let yaml = r#"
//! name: My Pipeline
//! triggers:
//!   manual: {}
//! workflows:
//!   - name: Build
//!     id: build
//!     workflow: global/docker-build
//! "#;
//!
//! let result = parser.parse(yaml).await;
//! ```

pub mod affinity;
pub mod dag;
pub mod error;
pub mod hash_files;
pub mod ir;
pub mod parser;
pub mod providers;
pub mod schema;
pub mod semver;
pub mod span;
pub mod variable;
pub mod workflow;

pub use affinity::validate_share_workspace_affinity;
pub use dag::{DagNode, ValidatedDag, build_dag};
pub use error::{ErrorCode, ParseDiagnostics, ParseError, Result, Severity, SourceLocation};
pub use hash_files::{HashFilesOptions, hash_files, hash_files_with_glob};
pub use ir::{
    CacheConfig, EnvValue, HealthCheck, HealthCheckMethod, JobIR, PipelineIR, PoolSelector,
    RetryPolicy, ScheduleTrigger, SecretRef, ServiceDef, Shell, StepCommand, StepIR, TagTrigger,
    TagValue, Trigger, WebhookEvent, WebhookTrigger, WorkflowRef, WorkflowScope,
};
pub use parser::{ParserConfig, PipelineParser, secret_refs_from_raw_secrets};
pub use providers::GitWorkflowProvider;
#[cfg(feature = "database")]
pub use providers::{CompositeWorkflowProvider, DatabaseWorkflowProvider};
pub use schema::{
    RawAgentAffinity, RawAzureSecretRef, RawCacheConfig, RawEnvironment, RawGcpSecretRef, RawJob,
    RawKubernetesSecretRef, RawPipeline, RawPoolSelector, RawRetryPolicy, RawSecretRef, RawService,
    RawStep, RawStoredSecretRef, RawTriggers, RawWorkflowDef, RawWorkflowInvocation,
    RawWorkspaceTransfer,
};
pub use semver::{VersionConstraint, parse_version_constraint, resolve_version};
pub use span::{SpanTracker, SpannedYamlParser};
pub use variable::{
    VariableContext, extract_vars, has_refs, interpolate, validate_refs,
    validate_refs_in_run_script,
};
pub use workflow::{MockWorkflowProvider, WorkflowFetchError, WorkflowProvider, WorkflowResolver};
