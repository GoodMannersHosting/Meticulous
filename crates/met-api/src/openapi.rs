//! OpenAPI specification and Swagger UI configuration.

use utoipa::OpenApi;

use crate::error::{ErrorBody, ErrorResponse};
use crate::extractors::pagination::PaginationMeta;
use crate::routes::{
    admin_workflows::{AdminWorkflowDeleteResponse, AdminWorkflowOpResponse},
    agents::{AgentActionResponse, AgentResponse},
    artifacts::{ArtifactResponse, AttestationResponse, SbomResponse},
    auth::{
        AdminResetPasswordRequest, AdminResetPasswordResponse, AuthProvidersResponse,
        ChangePasswordRequest, ChangePasswordResponse, LoginRequest, LoginResponse, LogoutResponse,
        MeResponse, PublicAuthProvider, SetupRequest, SetupResponse, SetupStatusResponse,
        UserResponse,
    },
    dashboard::{DashboardStatsResponse, RecentRunResponse},
    debug::{CreateDebugSessionRequest, DebugSecretResponse, DebugSessionResponse},
    health::{CheckStatus, HealthResponse, ReadyChecks, ReadyResponse},
    orgs::{CreateOrgRequest, OrgResponse, UpdateOrgRequest},
    pipelines::{
        CreatePipelineRequest, ImportPipelineGitRequest, PipelineResponse, SyncPipelineGitRequest,
        TriggerPipelineRequest, TriggerPipelineResponse, UpdatePipelineRequest,
        ValidatePipelineRequest, ValidatePipelineResponse,
    },
    projects::{CreateProjectRequest, ProjectResponse, UpdateProjectRequest},
    runs::{
        CancelRunResponse, DagNodeResponse, ExecutedBinarySummary, FootprintBinaryRow,
        FootprintDirectoryEntry, FootprintDirectoryGroup, FootprintNetworkRow,
        JobAssignmentResponse, JobRunResponse, JobRunSnapshotsResponse, LogsQuery, LogsResponse,
        RetryRunResponse, RunDagResponse, RunFootprintResponse, RunResponse, StepRunResponse,
    },
    secrets::{CreateSecretRequest, SecretResponse, UpdateSecretRequest},
    stored_secrets::{CreateStoredSecretRequest, RotateStoredSecretRequest, StoredSecretResponse},
    tokens::{CreateTokenRequest, CreateTokenResponseBody, TokenResponse},
    triggers::{CreateTriggerRequest, TriggerPublicResponse, UpdateTriggerRequest},
    variables::{CreateVariableRequest, UpdateVariableRequest, VariableResponse},
    webhooks::{
        CreateWebhookTargetRequest, PatchProjectWebhookRequest, ProjectWebhookRegistrationResponse,
        RotateInboundSecretResponse, SetupScmWebhookRequest, SetupScmWebhookResponse,
        SetupScmWebhookTargetInput, UpdateWebhookTargetRequest, WebhookResponse,
        WebhookTargetResponse,
    },
    workflows::{
        CreateWorkflowRequest, ProjectWorkflowsAvailableResponse, WorkflowResponse,
        WorkflowVersionsResponse,
    },
    workflows_catalog::{
        CatalogCommitPreview, CatalogRefItem, CatalogUpstreamRefSearchRequest,
        CatalogUpstreamRefSearchResponse, CatalogVersionsPage, ImportCatalogWorkflowGitRequest,
    },
    workspace_config::{WorkspaceStoredSecretListItem, WorkspaceVariableListItem},
};
use crate::workflow_diagnostics::WorkflowDiagnosticItem;

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Meticulous API",
        description = "REST API for the Meticulous CI/CD platform",
        version = "1.0.0",
        contact(name = "Meticulous Team"),
        license(name = "UNLICENSED"),
    ),
    servers(
        (url = "/", description = "Current server"),
    ),
    tags(
        (name = "health", description = "Health and readiness checks"),
        (name = "auth", description = "Authentication and session management"),
        (name = "organizations", description = "Organization management"),
        (name = "projects", description = "Project management"),
        (name = "pipelines", description = "Pipeline management"),
        (name = "runs", description = "Pipeline run management"),
        (name = "dashboard", description = "Org dashboard overview"),
        (name = "agents", description = "Agent management"),
        (name = "tokens", description = "API token management"),
        (name = "secrets", description = "Secret management"),
        (name = "stored_secrets", description = "Platform-stored encrypted secrets (metadata only on read)"),
        (name = "variables", description = "Variable management"),
        (name = "workflows", description = "Reusable workflow management"),
        (name = "webhooks", description = "Webhook management"),
        (name = "triggers", description = "Pipeline trigger configuration"),
        (name = "artifacts", description = "Build artifact management"),
        (name = "debug", description = "Debug session management"),
    ),
    paths(
        // Health
        crate::routes::health::health_handler,
        crate::routes::health::ready_handler,
        // Auth
        crate::routes::auth::list_auth_providers,
        crate::routes::auth::login,
        crate::routes::auth::me,
        crate::routes::auth::logout,
        crate::routes::auth::setup_status,
        crate::routes::auth::setup,
        crate::routes::auth::change_password,
        crate::routes::auth::admin_reset_password,
        // Organizations
        crate::routes::orgs::list_orgs,
        crate::routes::orgs::create_org,
        crate::routes::orgs::get_org,
        crate::routes::orgs::update_org,
        crate::routes::orgs::delete_org,
        // Projects
        crate::routes::projects::list_projects,
        crate::routes::projects::create_project,
        crate::routes::projects::get_project,
        crate::routes::projects::update_project,
        crate::routes::projects::delete_project,
        // Pipelines
        crate::routes::pipelines::list_pipelines,
        crate::routes::pipelines::create_pipeline,
        crate::routes::pipelines::get_pipeline,
        crate::routes::pipelines::update_pipeline,
        crate::routes::pipelines::delete_pipeline,
        crate::routes::pipelines::trigger_pipeline,
        crate::routes::pipelines::validate_pipeline,
        crate::routes::pipelines::pipeline_workflow_diagnostics,
        crate::routes::pipelines::import_pipeline_git,
        crate::routes::pipelines::sync_pipeline_from_git,
        crate::routes::triggers::list_triggers,
        crate::routes::triggers::create_trigger,
        crate::routes::triggers::update_trigger,
        crate::routes::triggers::delete_trigger,
        // Dashboard
        crate::routes::dashboard::dashboard_stats,
        crate::routes::dashboard::dashboard_recent_runs,
        // Runs
        crate::routes::runs::list_runs,
        crate::routes::runs::get_run,
        crate::routes::runs::cancel_run,
        crate::routes::runs::retry_run,
        crate::routes::runs::get_run_jobs,
        crate::routes::runs::get_job_run_snapshots,
        crate::routes::runs::get_job_steps,
        crate::routes::runs::get_job_assignments,
        crate::routes::runs::get_job_logs,
        crate::routes::runs::get_run_dag,
        crate::routes::runs::get_run_footprint,
        // Agents
        crate::routes::agents::list_agents,
        crate::routes::agents::get_agent,
        crate::routes::agents::delete_agent,
        crate::routes::agents::drain_agent,
        crate::routes::agents::resume_agent,
        crate::routes::agents::revoke_agent,
        // Tokens
        crate::routes::tokens::list_tokens,
        crate::routes::tokens::create_token,
        crate::routes::tokens::revoke_token,
        // Secrets
        crate::routes::secrets::list_secrets,
        crate::routes::secrets::create_secret,
        crate::routes::secrets::update_secret,
        crate::routes::secrets::delete_secret,
        // Stored secrets (builtin_secrets)
        crate::routes::stored_secrets::list_stored_secrets,
        crate::routes::stored_secrets::list_stored_secret_versions,
        crate::routes::stored_secrets::create_stored_secret,
        crate::routes::stored_secrets::rotate_stored_secret,
        crate::routes::stored_secrets::delete_stored_secret,
        crate::routes::stored_secrets::activate_stored_secret_version,
        crate::routes::stored_secrets::purge_stored_secret_version,
        // Variables
        crate::routes::variables::list_variables,
        crate::routes::variables::create_variable,
        crate::routes::variables::update_variable,
        crate::routes::variables::delete_variable,
        crate::routes::workspace_config::list_workspace_variables,
        crate::routes::workspace_config::list_workspace_stored_secrets,
        // Workflows
        crate::routes::workflows::list_global_workflows,
        crate::routes::workflows::list_project_workflows_available,
        crate::routes::workflows::list_project_workflows,
        crate::routes::workflows::create_project_workflow,
        crate::routes::workflows::get_workflow,
        crate::routes::workflows::list_versions,
        crate::routes::workflows_catalog::list_catalog_workflows,
        crate::routes::workflows_catalog::import_catalog_workflow_git_organization,
        crate::routes::workflows_catalog::catalog_upstream_ref_search_organization,
        crate::routes::workflows_catalog::import_catalog_workflow_git,
        crate::routes::workflows_catalog::catalog_upstream_ref_search_project,
        crate::routes::workflows_catalog::list_catalog_versions,
        crate::routes::admin_workflows::approve_catalog_workflow,
        crate::routes::admin_workflows::reject_catalog_workflow,
        crate::routes::admin_workflows::trust_catalog_workflow,
        crate::routes::admin_workflows::untrust_catalog_workflow,
        crate::routes::admin_workflows::soft_delete_catalog_workflow,
        // Webhooks
        crate::routes::webhooks::handle_webhook,
        crate::routes::webhooks::handle_github_webhook,
        crate::routes::webhooks::handle_gitlab_webhook,
        crate::routes::webhooks::handle_bitbucket_webhook,
        crate::routes::webhooks::setup_scm_webhook,
        crate::routes::webhooks::list_project_webhooks,
        crate::routes::webhooks::patch_project_webhook,
        crate::routes::webhooks::rotate_project_webhook_inbound_secret,
        crate::routes::webhooks::clear_project_webhook_inbound_secret,
        crate::routes::webhooks::list_webhook_targets,
        crate::routes::webhooks::create_webhook_target,
        crate::routes::webhooks::update_webhook_target,
        crate::routes::webhooks::delete_webhook_target,
        // Artifacts
        crate::routes::artifacts::list_run_artifacts,
        crate::routes::artifacts::get_artifact,
        crate::routes::artifacts::get_run_sbom,
        crate::routes::artifacts::get_run_attestation,
        // Debug
        crate::routes::debug::create_debug_session,
        crate::routes::debug::get_debug_secret,
    ),
    components(
        schemas(
            // Error types
            ErrorResponse, ErrorBody,
            // Pagination
            PaginationMeta,
            // Health
            HealthResponse, ReadyResponse, ReadyChecks, CheckStatus,
            // Auth
            PublicAuthProvider, AuthProvidersResponse,
            LoginRequest, LoginResponse,
            UserResponse, MeResponse,
            LogoutResponse,
            SetupStatusResponse, SetupRequest, SetupResponse,
            ChangePasswordRequest, ChangePasswordResponse,
            AdminResetPasswordRequest, AdminResetPasswordResponse,
            // Organizations
            OrgResponse, CreateOrgRequest, UpdateOrgRequest,
            // Projects
            ProjectResponse, CreateProjectRequest, UpdateProjectRequest,
            // Dashboard
            DashboardStatsResponse, RecentRunResponse,
            // Pipelines
            PipelineResponse, CreatePipelineRequest, UpdatePipelineRequest,
            ImportPipelineGitRequest, SyncPipelineGitRequest,
            TriggerPipelineRequest, TriggerPipelineResponse,
            ValidatePipelineRequest, ValidatePipelineResponse,
            TriggerPublicResponse, CreateTriggerRequest, UpdateTriggerRequest,
            // Runs
            RunResponse, CancelRunResponse, RetryRunResponse,
            JobRunResponse, JobRunSnapshotsResponse, JobAssignmentResponse, StepRunResponse,
            RunFootprintResponse, FootprintBinaryRow, FootprintNetworkRow,
            FootprintDirectoryGroup, FootprintDirectoryEntry,
            LogsQuery, LogsResponse,
            DagNodeResponse, ExecutedBinarySummary, RunDagResponse,
            // Agents
            AgentResponse, AgentActionResponse,
            // Tokens
            TokenResponse, CreateTokenRequest, CreateTokenResponseBody,
            // Secrets
            SecretResponse, CreateSecretRequest, UpdateSecretRequest,
            StoredSecretResponse, CreateStoredSecretRequest, RotateStoredSecretRequest,
            // Variables
            VariableResponse, CreateVariableRequest, UpdateVariableRequest,
            WorkspaceVariableListItem, WorkspaceStoredSecretListItem,
            // Webhooks
            WebhookResponse, WebhookTargetResponse,
            SetupScmWebhookRequest, SetupScmWebhookTargetInput, SetupScmWebhookResponse,
            ProjectWebhookRegistrationResponse,
            PatchProjectWebhookRequest, RotateInboundSecretResponse,
            CreateWebhookTargetRequest, UpdateWebhookTargetRequest,
            // Workflows
            WorkflowResponse, ProjectWorkflowsAvailableResponse, CreateWorkflowRequest,
            WorkflowVersionsResponse,
            ImportCatalogWorkflowGitRequest, CatalogVersionsPage, CatalogUpstreamRefSearchRequest,
            CatalogUpstreamRefSearchResponse, CatalogRefItem, CatalogCommitPreview,
            WorkflowDiagnosticItem,
            AdminWorkflowOpResponse, AdminWorkflowDeleteResponse,
            // Artifacts
            ArtifactResponse, SbomResponse, AttestationResponse,
            // Debug
            CreateDebugSessionRequest, DebugSessionResponse, DebugSecretResponse,
        )
    )
)]
pub struct ApiDoc;
