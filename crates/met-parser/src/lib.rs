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

pub mod dag;
pub mod error;
pub mod ir;
pub mod parser;
pub mod schema;
pub mod variable;
pub mod workflow;

pub use dag::{build_dag, DagNode, ValidatedDag};
pub use error::{ErrorCode, ParseDiagnostics, ParseError, Result, Severity, SourceLocation};
pub use ir::{
    CacheConfig, EnvValue, HealthCheck, HealthCheckMethod, JobIR, PipelineIR, PoolSelector,
    RetryPolicy, ScheduleTrigger, SecretRef, ServiceDef, Shell, StepCommand, StepIR, TagTrigger,
    TagValue, Trigger, WebhookEvent, WebhookTrigger, WorkflowRef, WorkflowScope,
};
pub use parser::{ParserConfig, PipelineParser};
pub use schema::{
    RawCacheConfig, RawJob, RawPipeline, RawPoolSelector, RawRetryPolicy, RawSecretRef, RawService,
    RawStep, RawTriggers, RawWorkflowDef, RawWorkflowInvocation,
};
pub use variable::{extract_vars, has_refs, interpolate, validate_refs, VariableContext};
pub use workflow::{MockWorkflowProvider, WorkflowFetchError, WorkflowProvider, WorkflowResolver};
