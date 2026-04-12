//! Repository modules for database operations.
//!
//! Each repository provides CRUD operations for a specific entity type,
//! with compile-time checked SQL queries.

mod agent_heartbeats;
mod agent_join_registration;
mod agents;
mod api_tokens;
mod audit_log;
mod auth_providers;
mod builtin_secrets;
mod cache_entries;
mod dashboard;
mod definition_snapshots;
mod environments;
mod groups;
mod job_assignments;
mod jobs;
mod join_tokens;
mod log_cache;
mod meticulous_apps;
mod oidc_signing_keys;
mod org_policy;
mod organizations;
mod pipeline_members;
mod pipeline_run_workflow_outputs;
mod pipelines;
mod platform_health;
mod platform_settings;
mod project_members;
mod projects;
mod roles;
mod run_binary_executions;
mod run_logs;
mod run_network_connections;
mod runs;
mod secret_provider_configs;
mod triggers;
mod users;
mod webhooks;
mod workflows;

pub use agent_heartbeats::{AgentHeartbeatRepo, HeartbeatStats};
pub use agent_join_registration::{
    reenroll_agent_with_exhausted_join_token, register_agent_with_join_token,
};
pub use agents::AgentRepo;
pub use api_tokens::ApiTokenRepo;
pub use audit_log::{AuditLogFilter, AuditLogRepo, AuditLogRow, CreateAuditLog};
pub use auth_providers::AuthProviderRepo;
pub use builtin_secrets::{
    BuiltinSecretCipherRow, BuiltinSecretMetaRow, BuiltinSecretsRepo, StoredSecretKind,
};
pub use cache_entries::CacheEntryRepo;
pub use dashboard::{DashboardRecentRunRow, DashboardStats, org_dashboard_stats, org_recent_runs};
pub use definition_snapshots::DefinitionSnapshotRepo;
pub use environments::{EnvironmentApprovalRow, EnvironmentRepo, EnvironmentRow};
pub use groups::{GroupRepo, UserGroupInfoRow};
pub use job_assignments::JobAssignmentRepo;
pub use jobs::{JobDagNode, JobRepo};
pub use join_tokens::JoinTokenRepo;
pub use log_cache::{
    LazyCacheLine, LogArchiveRow, LogCacheEntry, LogCacheRepo, project_run_for_job_run,
};
pub use meticulous_apps::{MeticulousAppInstallationSummary, MeticulousAppRepo};
pub use oidc_signing_keys::{
    OidcPublicKeyRow, OidcSigningKeyRepo, OidcSigningKeyRow, OidcTokenAuditRow,
    ensure_initial_oidc_signing_key,
};
pub use org_policy::{OrgPolicy, OrgPolicyPatch, OrgPolicyRepo};
pub use organizations::OrganizationRepo;
pub use pipeline_members::{PipelineAccessRepo, PipelineMemberRow, PipelineRole};
pub use pipeline_run_workflow_outputs::PipelineRunWorkflowOutputsRepo;
pub use pipelines::PipelineRepo;
pub use platform_health::{
    OrgArtifactStorageTotals, RelationSizeRow, database_disk_overview, org_artifact_storage_totals,
};
pub use platform_settings::PlatformSettingsRepo;
pub use project_members::{ProjectAccessRepo, ProjectMemberRow, ProjectRole};
pub use projects::{ProjectRepo, ProjectRetentionRow};
pub use roles::RoleRepo;
pub use run_binary_executions::{
    RunBinaryExecutionAgg, RunBinaryExecutionRepo, RunBinaryFootprintRow,
};
pub use run_logs::{LogEntry, RunLogRepo};
pub use run_network_connections::{RunNetworkConnectionRepo, RunNetworkConnectionRow};
pub use runs::{
    JobQueueItemRow, JobRunPipelineContext, JobRunRepo, OidcJobIdentityRow, RunRepo, RunWithJobs,
    RunWithPipelineName, StepRunRepo,
};
pub use secret_provider_configs::{
    SecretProviderConfigMeta, SecretProviderConfigRepo, SecretProviderConfigRow,
};
pub use triggers::{PipelineTriggerListEntry, TriggerRepo, get_trigger_for_webhook_dispatch};
pub use users::UserRepo;
pub use webhooks::{
    CreateWebhookTarget, UpdateWebhookTarget, WebhookDeliveryClaim, WebhookRegistrationContext,
    WebhookRegistrationSummary, WebhookRegistrationTarget, WebhookRepo,
};
pub use workflows::{
    CreateGlobalCatalogGit, CreateWorkflow, ReusableWorkflow, WorkflowRepo, WorkflowScope,
    WorkflowSource, WorkflowSubmissionStatus, WorkflowTrustState, WorkflowVersionListMode,
};
