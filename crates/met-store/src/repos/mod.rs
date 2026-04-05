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
mod groups;
mod job_assignments;
mod join_tokens;
mod organizations;
mod pipelines;
mod projects;
mod roles;
mod runs;
mod users;
mod workflows;
mod audit_log;
mod builtin_secrets;
mod run_logs;
mod log_cache;

pub use agent_heartbeats::{AgentHeartbeatRepo, HeartbeatStats};
pub use agent_join_registration::register_agent_with_join_token;
pub use agents::AgentRepo;
pub use api_tokens::ApiTokenRepo;
pub use audit_log::{AuditLogRepo, AuditLogRow, AuditLogFilter, CreateAuditLog};
pub use auth_providers::AuthProviderRepo;
pub use builtin_secrets::{BuiltinSecretCipherRow, BuiltinSecretMetaRow, BuiltinSecretsRepo, StoredSecretKind};
pub use cache_entries::CacheEntryRepo;
pub use groups::GroupRepo;
pub use job_assignments::JobAssignmentRepo;
pub use join_tokens::JoinTokenRepo;
pub use organizations::OrganizationRepo;
pub use pipelines::PipelineRepo;
pub use projects::ProjectRepo;
pub use roles::RoleRepo;
pub use runs::{JobQueueItemRow, JobRunPipelineContext, JobRunRepo, RunRepo, RunWithJobs, StepRunRepo};
pub use users::UserRepo;
pub use workflows::{CreateWorkflow, ReusableWorkflow, WorkflowRepo, WorkflowScope};
pub use run_logs::{LogEntry, RunLogRepo};
pub use log_cache::{
    project_run_for_job_run, LazyCacheLine, LogArchiveRow, LogCacheEntry, LogCacheRepo,
};
