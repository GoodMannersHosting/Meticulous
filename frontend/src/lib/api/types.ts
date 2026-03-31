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
}

// Project Types
export type OwnerType = 'user' | 'group';

export interface Project {
	id: string;
	org_id: string;
	name: string;
	slug: string;
	description?: string;
	owner_type: OwnerType;
	owner_id: string;
	created_at: string;
	updated_at: string;
}

export interface CreateProjectInput {
	name: string;
	slug: string;
	description?: string;
	owner_type: OwnerType;
	owner_id: string;
}

/** Metadata for a platform-stored secret (no plaintext). */
export interface StoredSecret {
	id: string;
	project_id?: string | null;
	pipeline_id?: string | null;
	path: string;
	kind: string;
	version: number;
	metadata: Record<string, unknown>;
	description?: string | null;
	created_at: string;
	updated_at: string;
}

// Pipeline Types
export interface Pipeline {
	id: string;
	project_id: string;
	name: string;
	slug: string;
	description?: string;
	definition: PipelineDefinition;
	definition_path?: string;
	enabled: boolean;
	created_at: string;
	updated_at: string;
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

// Run Types
export type RunStatus = 'pending' | 'queued' | 'running' | 'succeeded' | 'failed' | 'cancelled' | 'timed_out';

export interface Run {
	id: string;
	pipeline_id: string;
	trigger_id?: string;
	status: RunStatus;
	run_number: number;
	commit_sha?: string;
	branch?: string;
	triggered_by: string;
	created_at: string;
	started_at?: string;
	finished_at?: string;
	duration_ms?: number;
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
	agent_id?: string;
	status: JobStatus;
	attempt: number;
	log_path?: string;
	started_at?: string;
	finished_at?: string;
	created_at: string;
}

export interface StepRun {
	id: string;
	job_run_id: string;
	step_id: string;
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
	completed_today: number;
	failed_today: number;
	avg_duration_ms: number;
	agents_online: number;
	agents_total: number;
	pipelines_count: number;
	projects_count: number;
}

export interface RecentRun {
	id: string;
	pipeline_name: string;
	run_number: number;
	status: RunStatus;
	triggered_by: string;
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
	search?: string;
}

export interface ListPipelinesParams {
	[key: string]: string | number | boolean | undefined;
	project_id: string;
	page?: number;
	per_page?: number;
}

export interface ListRunsParams {
	[key: string]: string | number | boolean | undefined;
	pipeline_id: string;
	status?: string;
	page?: number;
	per_page?: number;
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
	password_must_change: boolean;
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
