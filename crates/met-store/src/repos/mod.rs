//! Repository modules for database operations.
//!
//! Each repository provides CRUD operations for a specific entity type,
//! with compile-time checked SQL queries.

mod agent_heartbeats;
mod agent_join_registration;
mod agents;
mod api_tokens;
mod auth_providers;
mod cache_entries;
mod dashboard;
mod definition_snapshots;
mod groups;
mod job_assignments;
mod join_tokens;
mod jobs;
mod organizations;
mod pipeline_run_workflow_outputs;
mod pipelines;
mod projects;
mod roles;
mod runs;
mod users;
mod workflows;
mod audit_log;
mod builtin_secrets;
mod run_logs;
mod run_binary_executions;
mod run_network_connections;
mod log_cache;
mod meticulous_apps;
mod triggers;

pub use agent_heartbeats::{AgentHeartbeatRepo, HeartbeatStats};
pub use agent_join_registration::{
    register_agent_with_join_token, reenroll_agent_with_exhausted_join_token,
};
pub use agents::AgentRepo;
pub use api_tokens::ApiTokenRepo;
pub use audit_log::{AuditLogRepo, AuditLogRow, AuditLogFilter, CreateAuditLog};
pub use auth_providers::AuthProviderRepo;
pub use builtin_secrets::{BuiltinSecretCipherRow, BuiltinSecretMetaRow, BuiltinSecretsRepo, StoredSecretKind};
pub use cache_entries::CacheEntryRepo;
pub use dashboard::{
    org_dashboard_stats, org_recent_runs, DashboardRecentRunRow, DashboardStats,
};
pub use definition_snapshots::DefinitionSnapshotRepo;
pub use groups::GroupRepo;
pub use job_assignments::JobAssignmentRepo;
pub use jobs::{JobDagNode, JobRepo};
pub use join_tokens::JoinTokenRepo;
pub use meticulous_apps::MeticulousAppRepo;
pub use organizations::OrganizationRepo;
pub use triggers::{get_trigger_for_webhook_dispatch, TriggerRepo};
pub use pipeline_run_workflow_outputs::PipelineRunWorkflowOutputsRepo;
pub use pipelines::PipelineRepo;
pub use projects::ProjectRepo;
pub use roles::RoleRepo;
pub use runs::{
    JobQueueItemRow, JobRunPipelineContext, JobRunRepo, RunRepo, RunWithJobs, RunWithPipelineName,
    StepRunRepo,
};
pub use users::UserRepo;
pub use workflows::{
    CreateGlobalCatalogGit, CreateWorkflow, ReusableWorkflow, WorkflowRepo, WorkflowScope,
    WorkflowSource, WorkflowSubmissionStatus, WorkflowTrustState, WorkflowVersionListMode,
};
pub use run_logs::{LogEntry, RunLogRepo};
pub use run_binary_executions::{
    RunBinaryExecutionAgg, RunBinaryExecutionRepo, RunBinaryFootprintRow,
};
pub use run_network_connections::{RunNetworkConnectionRepo, RunNetworkConnectionRow};
pub use log_cache::{
    project_run_for_job_run, LazyCacheLine, LogArchiveRow, LogCacheEntry, LogCacheRepo,
};
