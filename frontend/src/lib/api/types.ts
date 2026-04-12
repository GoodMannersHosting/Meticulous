// API Response Types

export interface ApiError {
	code: string;
	message: string;
	details?: Record<string, unknown>;
}

export interface ApiResponse<T> {
	data: T;
	meta?: {
		total?: number;
		page?: number;
		per_page?: number;
	};
}

export interface PaginatedResponse<T> {
	data: T[];
	pagination: {
		has_more: boolean;
		next_cursor?: string;
		count?: number;
	};
}

// Auth Types
export interface User {
	id: string;
	org_id: string;
	name: string;
	email: string;
	avatar?: string;
	role: string;
	created_at: string;
	/** When true, the user must change password before using the app (from /auth/me). */
	password_must_change?: boolean;
	/** From /auth/me when backed by users.service_account. */
	service_account?: boolean;
	/** Group memberships from `/auth/me`. */
	groups?: { id: string; name: string; role: string }[];
}

export interface AuthTokens {
	access_token: string;
	refresh_token?: string;
	expires_in: number;
	token_type: string;
}

// Organization Types
export interface Organization {
	id: string;
	name: string;
	slug: string;
	created_at: string;
	/** When false, untrusted global catalog workflows cannot run. */
	allow_untrusted_workflows?: boolean;
}

/** Global catalog workflow row (API `/workflows/catalog` and related). */
export interface CatalogWorkflow {
	id: string;
	scope: string;
	project_id?: string | null;
	name: string;
	version: string;
	definition: Record<string, unknown>;
	description?: string | null;
	deprecated: boolean;
	tags: string[];
	created_at: string;
	updated_at: string;
	source: string;
	scm_repository?: string | null;
	scm_ref?: string | null;
	scm_path?: string | null;
	scm_revision?: string | null;
	submission_status: string;
	trust_state: string;
	submitted_by?: string | null;
	reviewed_by?: string | null;
	reviewed_at?: string | null;
	deleted_at?: string | null;
	catalog_metadata: Record<string, unknown>;
}

export interface WorkflowDiagnosticItem {
	invocation_id: string;
	reference: string;
	scope: string;
	name: string;
	version_requested: string;
	version_resolved?: string | null;
	status: string;
	detail?: string | null;
	blocking: boolean;
	/** Declared output names from the workflow definition, when parseable */
	declared_outputs?: string[] | null;
}

export interface CatalogVersionsPage {
	workflow_name: string;
	versions: CatalogWorkflow[];
	has_more?: boolean;
	next_cursor?: string | null;
}

/** POST /workflows/catalog/upstream-ref-search (org admin or project-scoped variant). */
export interface CatalogRefItem {
	name: string;
	commit_sha: string;
}

export interface CatalogCommitPreview {
	sha: string;
	title: string;
	committed_at?: string | null;
}

export interface CatalogUpstreamRefSearchResponse {
	branches: CatalogRefItem[];
	tags: CatalogRefItem[];
	commits: CatalogCommitPreview[];
}

// Project Types
export type OwnerType = 'user' | 'group';
export type ResourceVisibility = 'public' | 'authenticated' | 'private';

export interface Project {
	id: string;
	org_id: string;
	name: string;
	slug: string;
	description?: string;
	owner_type: OwnerType;
	owner_id: string;
	visibility: ResourceVisibility;
	created_at: string;
	updated_at: string;
}

export interface CreateProjectInput {
	name: string;
	slug: string;
	description?: string;
	owner_type: OwnerType;
	owner_id: string;
	visibility?: ResourceVisibility;
}

/** PATCH /api/v1/projects/{id} */
export interface UpdateProjectInput {
	name?: string;
	slug?: string;
	description?: string | null;
	visibility?: ResourceVisibility;
}

/** Member of a project or pipeline. */
export type MemberRole = 'admin' | 'operator' | 'readonly';

export interface Member {
	id: string;
	principal_type: 'user' | 'group';
	principal_id: string;
	role: MemberRole;
	inherited?: boolean;
	display_name?: string;
	created_at: string;
}

export interface AddMemberInput {
	principal_type: 'user' | 'group';
	principal_id: string;
	role: MemberRole;
}

export interface UpdateMemberRoleInput {
	role: MemberRole;
}

/** Batch save for project/pipeline access controls (applied when the user clicks Save). */
export interface AccessControlSaveBatch {
	removePrincipalIds: string[];
	roleUpdates: { principalId: string; role: MemberRole }[];
	adds: AddMemberInput[];
}

/** GET /api/v1/admin/users/{id}/resource-access */
export interface AdminUserResourceProjectRow {
	project_id: string;
	project_name: string;
	role: MemberRole;
	via: 'direct' | 'group';
	group_name?: string | null;
}

export interface AdminUserResourcePipelineRow {
	pipeline_id: string;
	pipeline_name: string;
	project_id: string;
	project_name: string;
	role: MemberRole;
	inherited: boolean;
	via: 'direct' | 'group';
	group_name?: string | null;
}

export interface AdminUserResourceAccessResponse {
	projects: AdminUserResourceProjectRow[];
	pipelines: AdminUserResourcePipelineRow[];
}

/** GET /api/v1/admin/groups/{id}/resource-access */
export interface AdminGroupResourceProjectRow {
	project_id: string;
	project_name: string;
	role: MemberRole;
}

export interface AdminGroupResourcePipelineRow {
	pipeline_id: string;
	pipeline_name: string;
	project_id: string;
	project_name: string;
	role: MemberRole;
	inherited: boolean;
}

export interface AdminGroupResourceAccessResponse {
	projects: AdminGroupResourceProjectRow[];
	pipelines: AdminGroupResourcePipelineRow[];
}

/** Search result for users and groups. */
export interface PrincipalSearchResult {
	id: string;
	name: string;
	principal_type: 'user' | 'group';
	email?: string;
}

/** Platform-wide settings (super_admin only). */
export interface PlatformSettings {
	allow_unauthenticated_access: boolean;
	/** When `false`, create/rotate for that external kind is rejected (`aws_sm`, `vault`, …). */
	stored_secret_external_kinds?: Record<string, boolean>;
}

/** `GET /api/v1/stored-secret-policy` — readable by any authenticated user. */
export interface StoredSecretPolicy {
	stored_secret_external_kinds: Record<string, boolean>;
}

/** Pipeline environment (ADR-016). */
export type EnvironmentTier = 'development' | 'staging' | 'production' | 'custom';

export interface Environment {
	id: string;
	project_id: string;
	name: string;
	display_name: string;
	description?: string;
	require_approval: boolean;
	required_approvers: number;
	approval_timeout_hours: number;
	allowed_branches?: string[];
	auto_deploy_branch?: string;
	variables: Record<string, string>;
	tier: EnvironmentTier;
	created_at: string;
	updated_at: string;
}

export interface CreateEnvironmentInput {
	name: string;
	display_name: string;
	description?: string;
	tier?: EnvironmentTier;
}

export interface UpdateEnvironmentInput {
	/** URL-safe slug; must remain unique within the project. */
	name?: string;
	display_name?: string;
	description?: string;
	tier?: EnvironmentTier;
	require_approval?: boolean;
	required_approvers?: number;
	approval_timeout_hours?: number;
	allowed_branches?: string[];
	auto_deploy_branch?: string;
	variables?: Record<string, string>;
}

/** GET /api/v1/projects/{id}/workflows/available */
export interface ProjectWorkflowsAvailable {
	global_workflows: CatalogWorkflow[];
	project_workflows: CatalogWorkflow[];
}

/** Matrix view cell for pipeline environment dashboard. */
export interface MatrixResponse {
	workflows: string[];
	environments: MatrixEnvironment[];
	cells: MatrixCell[];
}

export interface MatrixEnvironment {
	id: string | null;
	name: string;
	tier: string;
}

export interface MatrixCell {
	workflow: string;
	environment: string | null;
	run_id: string | null;
	run_number: number | null;
	status: string | null;
	started_at: string | null;
	finished_at: string | null;
	duration_ms: number | null;
	branch: string | null;
	triggered_by: string | null;
}

/** Metadata for a platform-stored secret (no plaintext). */
export interface StoredSecret {
	id: string;
	project_id?: string | null;
	pipeline_id?: string | null;
	environment_id?: string | null;
	path: string;
	kind: string;
	version: number;
	metadata: Record<string, unknown>;
	description?: string | null;
	created_at: string;
	updated_at: string;
	/** Org-wide only: when `false`, not listed in project/pipeline secret UIs or used for `stored:` / checkout (catalog SCM may still use). */
	propagate_to_projects?: boolean;
}

// Pipeline Types
export interface Pipeline {
	id: string;
	project_id: string;
	name: string;
	slug: string;
	description?: string;
	definition: PipelineDefinition | Record<string, unknown>;
	definition_path?: string;
	scm_provider?: string | null;
	scm_repository?: string | null;
	scm_ref?: string | null;
	scm_path?: string | null;
	scm_credentials_secret_path?: string | null;
	scm_revision?: string | null;
	owner_type: OwnerType;
	owner_id: string;
	visibility: ResourceVisibility;
	enabled: boolean;
	created_at: string;
	updated_at: string;
}

/** `GET/POST /api/v1/pipelines/{id}/triggers` — `config` never includes `secret`. */
export interface PipelineTrigger {
	id: string;
	pipeline_id: string;
	kind: string;
	config: Record<string, unknown>;
	/** Effective inbound mode for webhooks: `none` | `hmac` | `query`. */
	inbound_auth?: string | null;
	inbound_query_param?: string | null;
	secret_configured: boolean;
	enabled: boolean;
	description?: string | null;
	created_by_user_id?: string | null;
	created_by_username?: string | null;
	created_at: string;
	updated_at: string;
	/** Only present on create when `generate_webhook_secret` was true. */
	generated_secret?: string | null;
}

export interface CreatePipelineTriggerInput {
	kind: string;
	config: Record<string, unknown>;
	description?: string;
	generate_webhook_secret?: boolean;
}

export interface UpdatePipelineTriggerInput {
	enabled?: boolean;
	description?: string;
	config_patch?: Record<string, unknown>;
}

/** `GET /api/v1/projects/{id}/webhooks` */
export interface ProjectWebhookRegistration {
	id: string;
	provider: string;
	events: string[];
	active: boolean;
	payload_mapping: Record<string, unknown>;
	created_at: string;
	/** Path starting with `/api/v1/webhooks/...` — prepend public API origin. */
	inbound_path: string;
	/** `generic` only: `none` | `hmac` | `query` */
	generic_inbound_auth?: string;
	generic_query_param_name?: string | null;
	/** Whether a signing secret is stored (HMAC / query value material). */
	inbound_secret_configured?: boolean;
	/** Only on PATCH/create when a new verifier was generated. */
	signing_secret?: string | null;
	description?: string | null;
	created_by_user_id?: string | null;
	created_by_username?: string | null;
}

/** `PATCH /api/v1/projects/{id}/webhooks/{registration_id}` */
export interface PatchProjectWebhookInput {
	description?: string;
	target_pipeline_ids?: string[];
	generic_inbound_auth?: 'none' | 'hmac' | 'query';
	generic_query_param_name?: string;
}

/** `POST .../rotate-inbound-secret` */
export interface RotateProjectWebhookSecretResponse {
	signing_secret: string;
}

/** `POST /api/v1/projects/{id}/scm/setup` (SCM + `generic` multi-pipeline webhooks). */
export interface SetupScmWebhookInput {
	provider: string;
	repository_url?: string;
	events?: string[];
	targets: Array<{ pipeline_id: string; filter_config?: Record<string, unknown> }>;
	/** For `provider: generic` — optional [`WebhookConfig`] JSON without `secret`. */
	payload_mapping?: Record<string, unknown>;
	generic_inbound_auth?: 'none' | 'hmac' | 'query';
	/** Required when `generic_inbound_auth` is `query`. */
	generic_query_param_name?: string;
	description?: string;
}

export interface SetupScmWebhookResponse {
	webhook_id: string;
	webhook_url: string;
	provider: string;
	events: string[];
	/** Only for `generic`: HMAC signing key (store securely; shown once). */
	signing_secret?: string;
}

/** Webhook routing target (`webhook_registration_targets`). */
export interface WebhookRegistrationTargetRow {
	id: string;
	pipeline_id: string;
	enabled: boolean;
	filter_config: Record<string, unknown>;
}

export interface PipelineDefinition {
	version?: string;
	jobs: PipelineJob[];
}

export interface PipelineJob {
	id?: string;
	name: string;
	depends_on?: string[];
	agent_tags?: string[];
	timeout_secs?: number;
	retry_count?: number;
	condition?: string;
	steps: PipelineStep[];
}

export interface PipelineStep {
	name: string;
	run?: string;
	uses?: string;
	with?: Record<string, unknown>;
	env?: Record<string, string>;
}

export interface CreatePipelineInput {
	project_id: string;
	name: string;
	slug: string;
	description?: string;
	definition: PipelineDefinition | Record<string, unknown>;
	definition_path?: string;
}

export interface ImportPipelineGitInput {
	name: string;
	slug: string;
	description?: string;
	repository: string;
	git_ref: string;
	scm_path: string;
	credentials_path: string;
}

/** Body for PUT /api/v1/pipelines/{id} — omit fields you do not want to change */
export interface UpdatePipelineInput {
	name?: string;
	description?: string;
	enabled?: boolean;
	definition?: PipelineDefinition | Record<string, unknown>;
	scm_provider?: string | null;
	scm_repository?: string | null;
	scm_ref?: string | null;
	scm_path?: string | null;
	scm_credentials_secret_path?: string | null;
	scm_revision?: string | null;
}

/** Project or pipeline-scoped environment variable (non-secret config). */
export interface ProjectVariable {
	id: string;
	project_id: string;
	pipeline_id?: string | null;
	name: string;
	/** Omitted when `is_sensitive` is true. */
	value?: string | null;
	scope: string;
	is_sensitive: boolean;
	created_at: string;
	updated_at: string;
}

/** `scope_level` for GET /api/v1/workspace/* hub lists. */
export type WorkspaceScopeLevel = 'all' | 'organization' | 'project' | 'pipeline';

/** Row from GET /api/v1/workspace/variables */
export interface WorkspaceVariableListItem extends ProjectVariable {
	project_name: string;
	project_slug: string;
	pipeline_name?: string | null;
}

/** Row from GET /api/v1/workspace/stored-secrets (secret fields flattened in JSON). */
export interface WorkspaceStoredSecretListItem extends StoredSecret {
	project_name?: string | null;
	project_slug?: string | null;
	pipeline_name?: string | null;
	environment_name?: string | null;
}

// Run Types
export type RunStatus = 'pending' | 'queued' | 'running' | 'succeeded' | 'failed' | 'cancelled' | 'timed_out';

export interface Run {
	id: string;
	pipeline_id: string;
	/** Present when this run was created via **Retry** from another run (null for Run Pipeline / new triggers). */
	parent_run_id?: string | null;
	/** Populated on run detail API when `parent_run_id` is set (parent's `run_number`). */
	parent_run_number?: number | null;
	/** Set when listing runs by project (all pipelines). */
	pipeline_name?: string;
	/** Set when listing runs across projects (no `project_id` filter). */
	project_id?: string;
	project_name?: string;
	trigger_id?: string;
	status: RunStatus;
	run_number: number;
	commit_sha?: string;
	branch?: string;
	triggered_by: string;
	/** Observed webhook client IP when the run was created from an HTTP webhook. */
	webhook_remote_addr?: string | null;
	created_at: string;
	started_at?: string;
	finished_at?: string;
	duration_ms?: number;
	/** Prefer for badges when set (API: run is executing but no job is on an agent yet). */
	status_display?: RunStatus | null;
}

/** GET /api/v1/runs/:id/dag — layout + exec telemetry per job. */
export interface RunDagExecutedBinary {
	binary_path: string;
	sha256: string;
	execution_count: number;
}

export interface RunDagNode {
	job_id: string;
	job_name: string;
	status: string;
	depends_on: string[];
	/** When set, steps for this workflow can be loaded for this run. */
	job_run_id?: string | null;
	executed_binaries?: RunDagExecutedBinary[];
}

export interface RunDagResponse {
	run_id: string;
	nodes: RunDagNode[];
}

/** One SBOM artifact linked to a job/step (GET /api/v1/runs/:run_id/sbom). */
export interface SbomArtifactApi {
	artifact_id: string;
	format: string;
	status: string;
	sbom: Record<string, unknown> | null;
	job_name?: string | null;
	step_name?: string | null;
	artifact_name: string;
	artifact_path: string;
	/** Resolved catalog workflow ref JSON when the job used a reusable workflow. */
	source_workflow?: Record<string, unknown> | null;
}

/** GET /api/v1/runs/:run_id/sbom */
export interface SbomApiResponse {
	run_id: string;
	/** `not_generated` when there are no SBOM-like artifacts; otherwise `ok`. */
	status: string;
	artifacts: SbomArtifactApi[];
}

/** GET /api/v1/runs/:id/footprint — execution surface for Blast Radius tab */
export interface FootprintBinaryRow {
	job_name: string;
	step_name?: string | null;
	binary_path: string;
	sha256: string;
	execution_count: number;
}

export interface FootprintNetworkRow {
	job_name?: string | null;
	dst_ip: string;
	dst_port: number;
	protocol: string;
	direction: string;
	connected_at: string;
	binary_path?: string | null;
	binary_sha256?: string | null;
}

export interface FootprintDirectoryEntry {
	binary_path: string;
	sha256: string;
	execution_count: number;
	job_names: string[];
}

export interface FootprintDirectoryGroup {
	directory: string;
	entries: FootprintDirectoryEntry[];
	entries_truncated: boolean;
}

export interface RunFootprintResponse {
	run_id: string;
	executed_binaries: FootprintBinaryRow[];
	network_connections: FootprintNetworkRow[];
	/** Total network rows stored for this run (may exceed `network_connections.length`). */
	network_connections_total_count: number;
	network_connections_truncated: boolean;
	filesystem_by_directory: FootprintDirectoryGroup[];
	filesystem_directories_truncated: boolean;
	filesystem_more_directory_count?: number | null;
}

/** GET /api/v1/runs/:run_id/jobs/:job_run_id/snapshots */
export interface JobRunSnapshotsResponse {
	pipeline_definition?: Record<string, unknown> | null;
	workflow_definition?: Record<string, unknown> | null;
}

export interface TriggerRunInput {
	branch?: string;
	commit_sha?: string;
	variables?: Record<string, string>;
}

export interface TriggerRunResponse {
	run_id: string;
	run_number: number;
	status: string;
}

// Job Types (DAG nodes)
export type JobStatus = 'pending' | 'queued' | 'running' | 'succeeded' | 'failed' | 'cancelled' | 'timed_out' | 'skipped';

export interface Job {
	id: string;
	pipeline_id: string;
	name: string;
	depends_on: string[];
	agent_tags: string[];
	timeout_secs?: number;
	retry_count: number;
	condition?: string;
	config: Record<string, unknown>;
	created_at: string;
}

export interface JobRun {
	id: string;
	run_id: string;
	job_id: string;
	job_name: string;
	agent_id?: string;
	status: JobStatus;
	attempt: number;
	exit_code?: number;
	error_message?: string;
	cache_hit?: boolean;
	log_path?: string;
	started_at?: string;
	finished_at?: string;
	duration_ms?: number;
	/** SHA-256 (hex) of pipeline definition JSON in `definition_snapshots`. */
	pipeline_definition_sha256?: string;
	/** SHA-256 (hex) of reusable workflow definition when this job used one. */
	workflow_definition_sha256?: string;
	/** Resolved reusable workflow: scope, name, version (or other JSON from API). */
	source_workflow?: Record<string, unknown>;
	/** Best-effort explanation when status is pending or queued (from API). */
	scheduling_note?: string;
	/** Agent/host audit JSON when the job entered running (stored for forensics). */
	agent_snapshot?: Record<string, unknown> | null;
	agent_snapshot_captured_at?: string;
	created_at: string;
}

/** One agent dispatch attempt for a job run (retries create multiple rows). */
export interface JobAssignment {
	id: string;
	job_run_id: string;
	agent_id: string;
	status: string;
	attempt: number;
	accepted_at: string;
	started_at?: string;
	completed_at?: string;
	exit_code?: number;
	failure_reason?: string;
}

export interface StepRun {
	id: string;
	job_run_id: string;
	step_id: string;
	step_name: string;
	status: JobStatus;
	exit_code?: number;
	log_path?: string;
	started_at?: string;
	finished_at?: string;
	created_at: string;
}

// Agent Types
export type AgentStatus = 'online' | 'offline' | 'busy' | 'draining';

export interface Agent {
	id: string;
	org_id: string;
	name: string;
	status: AgentStatus;
	pool?: string;
	/** Pools this agent can receive jobs for (enrollment). */
	pool_tags?: string[];
	tags: string[];
	os: string;
	arch: string;
	version: string;
	max_jobs: number;
	running_jobs: number;
	available_capacity: number;
	last_heartbeat_at?: string;
	created_at: string;
	/** Registration-time host / security snapshot from the agent. */
	last_security_bundle?: Record<string, unknown> | null;
}

// Dashboard Stats
export interface DashboardStats {
	active_runs: number;
	completed_runs: number;
	failed_runs: number;
	/** Cancelled runs finished (or updated) in the metrics window. */
	cancelled_runs: number;
	/** Runs created in the org during the selected metrics window (any status). */
	total_runs: number;
	avg_duration_ms: number;
	agents_online: number;
	agents_total: number;
	pipelines_count: number;
	projects_count: number;
	/** Time window key echoed from the API: 1h, 4h, 12h, 1d, 3d, 7d */
	window: string;
}

export interface RecentRun {
	id: string;
	pipeline_name: string;
	run_number: number;
	status: RunStatus;
	triggered_by: string;
	webhook_remote_addr?: string | null;
	duration_ms?: number;
	created_at: string;
}

// WebSocket Message Types
export type WsMessageType =
	| 'run_created'
	| 'run_updated'
	| 'job_started'
	| 'job_completed'
	| 'job_failed'
	| 'step_started'
	| 'step_completed'
	| 'step_failed'
	| 'log_line'
	| 'agent_connected'
	| 'agent_disconnected'
	| 'agent_status_changed'
	| 'ping'
	| 'pong';

export interface WsMessage<T = unknown> {
	type: WsMessageType;
	payload: T;
	timestamp: string;
}

export interface LogLinePayload {
	/** When present (REST logs), stable per job run for list keys and deduplication. */
	sequence?: number;
	run_id: string;
	job_run_id: string;
	step_run_id?: string;
	line: string;
	level: 'stdout' | 'stderr' | 'system';
	timestamp: string;
}

// Query Parameters
export interface ListProjectsParams {
	[key: string]: string | number | boolean | undefined;
	page?: number;
	per_page?: number;
	cursor?: string;
	search?: string;
}

export interface ListPipelinesParams {
	[key: string]: string | number | boolean | undefined;
	project_id: string;
	page?: number;
	per_page?: number;
	cursor?: string;
}

export interface ListRunsParams {
	[key: string]: string | number | boolean | undefined;
	/** Use this or `project_id`, not both. */
	pipeline_id?: string;
	/** All pipelines in the project; mutually exclusive with `pipeline_id`. */
	project_id?: string;
	status?: string;
	page?: number;
	per_page?: number;
	limit?: number;
	/** Offset into the pipeline's run list (API uses `cursor` query param). */
	cursor?: string;
	/** With `pipeline_id`, return the single run with this run number (e.g. previous = current − 1). */
	run_number?: number;
}

export interface ListAgentsParams {
	[key: string]: string | number | boolean | undefined;
	status?: string;
	pool?: string;
	tags?: string;
	page?: number;
	per_page?: number;
}

// Admin Types
export interface AdminUser {
	id: string;
	username: string;
	email: string;
	display_name?: string;
	is_active: boolean;
	is_admin: boolean;
	/** API-only principal; interactive login is blocked server-side. */
	service_account?: boolean;
	password_must_change: boolean;
	/** ISO 8601; absent or null if the user has never logged in interactively. */
	last_login_at?: string | null;
	created_at: string;
	updated_at: string;
}

export interface AdminGroup {
	id: string;
	name: string;
	description?: string;
	member_count: number;
	created_at: string;
	updated_at: string;
}

export type GroupRoleType = 'member' | 'maintainer' | 'owner';

export interface GroupMember {
	user_id: string;
	username: string;
	email: string;
	display_name?: string;
	role: GroupRoleType;
	joined_at: string;
}

export interface RoleInfo {
	name: string;
	description: string;
	permissions: string[];
}

export interface UserRoleAssignment {
	role: string;
	granted_by?: string;
	granted_at: string;
}

export interface AuthProviderResponse {
	id: string;
	name: string;
	provider_type: 'oidc' | 'github';
	client_id: string;
	issuer_url?: string;
	enabled: boolean;
	created_at: string;
	updated_at: string;
}

export interface GroupMappingResponse {
	id: string;
	provider_id: string;
	oidc_group_claim: string;
	meticulous_group_id: string;
	role: string;
	created_at: string;
}
