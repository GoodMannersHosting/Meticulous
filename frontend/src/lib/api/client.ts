import { browser } from '$app/environment';
import { goto } from '$app/navigation';
import { getPublicApiBase } from '$lib/public-api-base';
import type { ApiError, ApiResponse, StoredSecret } from './types';

export class ApiClientError extends Error {
	constructor(
		public readonly code: string,
		message: string,
		public readonly status: number,
		public readonly details?: Record<string, unknown>
	) {
		super(message);
		this.name = 'ApiClientError';
	}

	static fromApiError(error: ApiError, status: number): ApiClientError {
		return new ApiClientError(error.code, error.message, status, error.details);
	}
}

export interface RequestConfig extends RequestInit {
	params?: Record<string, string | number | boolean | undefined>;
	skipAuth?: boolean;
}

type RequestInterceptor = (config: RequestConfig) => RequestConfig | Promise<RequestConfig>;
type ResponseInterceptor = (response: Response) => Response | Promise<Response>;

class ApiClient {
	private baseUrl: string;
	private requestInterceptors: RequestInterceptor[] = [];
	private responseInterceptors: ResponseInterceptor[] = [];

	constructor() {
		this.baseUrl = '';
	}

	setBaseUrl(url: string): void {
		this.baseUrl = url;
	}

	addRequestInterceptor(interceptor: RequestInterceptor): () => void {
		this.requestInterceptors.push(interceptor);
		return () => {
			const index = this.requestInterceptors.indexOf(interceptor);
			if (index > -1) this.requestInterceptors.splice(index, 1);
		};
	}

	addResponseInterceptor(interceptor: ResponseInterceptor): () => void {
		this.responseInterceptors.push(interceptor);
		return () => {
			const index = this.responseInterceptors.indexOf(interceptor);
			if (index > -1) this.responseInterceptors.splice(index, 1);
		};
	}

	private getAuthToken(): string | null {
		if (!browser) return null;
		// Token is stored in cookie, but we can also check localStorage for SPA mode
		return localStorage.getItem('auth_token');
	}

	private buildUrl(endpoint: string, params?: Record<string, string | number | boolean | undefined>): string {
		const base = this.baseUrl.trim() || getPublicApiBase() || 'http://127.0.0.1:8080';
		const url = new URL(endpoint, base);

		if (params) {
			Object.entries(params).forEach(([key, value]) => {
				if (value !== undefined) {
					url.searchParams.set(key, String(value));
				}
			});
		}

		return url.toString();
	}

	private async applyRequestInterceptors(config: RequestConfig): Promise<RequestConfig> {
		let result = config;
		for (const interceptor of this.requestInterceptors) {
			result = await interceptor(result);
		}
		return result;
	}

	private async applyResponseInterceptors(response: Response): Promise<Response> {
		let result = response;
		for (const interceptor of this.responseInterceptors) {
			result = await interceptor(result);
		}
		return result;
	}

	async request<T>(endpoint: string, config: RequestConfig = {}): Promise<T> {
		const { params, skipAuth, ...fetchConfig } = await this.applyRequestInterceptors(config);

		const headers = new Headers(fetchConfig.headers);

		if (!headers.has('Content-Type') && fetchConfig.body) {
			headers.set('Content-Type', 'application/json');
		}

		if (!skipAuth) {
			const token = this.getAuthToken();
			if (token) {
				headers.set('Authorization', `Bearer ${token}`);
			}
		}

		const url = this.buildUrl(endpoint, params);

		let response = await fetch(url, {
			...fetchConfig,
			headers
		});

		response = await this.applyResponseInterceptors(response);

		if (!response.ok) {
			if (response.status === 401 && browser) {
				localStorage.removeItem('auth_token');
				goto('/login');
				throw new ApiClientError('UNAUTHORIZED', 'Session expired', 401);
			}

			let apiError: ApiError;
			try {
				const raw = (await response.json()) as { error?: ApiError } | ApiError;
				if (
					raw &&
					typeof raw === 'object' &&
					'error' in raw &&
					raw.error &&
					typeof raw.error.message === 'string'
				) {
					apiError = raw.error;
				} else {
					apiError = raw as ApiError;
				}
				if (!apiError?.message) {
					apiError = {
						code: apiError?.code || 'UNKNOWN_ERROR',
						message: response.statusText || 'Request failed',
						details: apiError?.details
					};
				}
			} catch {
				apiError = {
					code: 'UNKNOWN_ERROR',
					message: response.statusText || 'An unknown error occurred'
				};
			}

			throw ApiClientError.fromApiError(apiError, response.status);
		}

		if (response.status === 204 || response.status === 205) {
			return undefined as T;
		}

		const text = await response.text();
		if (!text.trim()) {
			return undefined as T;
		}

		return JSON.parse(text) as T;
	}

	async get<T>(endpoint: string, config?: RequestConfig): Promise<T> {
		return this.request<T>(endpoint, { ...config, method: 'GET' });
	}

	async post<T>(endpoint: string, data?: unknown, config?: RequestConfig): Promise<T> {
		return this.request<T>(endpoint, {
			...config,
			method: 'POST',
			body: data ? JSON.stringify(data) : undefined
		});
	}

	async put<T>(endpoint: string, data?: unknown, config?: RequestConfig): Promise<T> {
		return this.request<T>(endpoint, {
			...config,
			method: 'PUT',
			body: data ? JSON.stringify(data) : undefined
		});
	}

	async patch<T>(endpoint: string, data?: unknown, config?: RequestConfig): Promise<T> {
		return this.request<T>(endpoint, {
			...config,
			method: 'PATCH',
			body: data ? JSON.stringify(data) : undefined
		});
	}

	async delete<T>(endpoint: string, config?: RequestConfig): Promise<T> {
		return this.request<T>(endpoint, { ...config, method: 'DELETE' });
	}
}

export const api = new ApiClient();

// Login response from password authentication
export interface PasswordLoginResponse {
	token: string;
	token_type: string;
	expires_in: number;
	user: {
		id: string;
		username: string;
		email: string;
		display_name?: string;
		is_admin: boolean;
		password_must_change: boolean;
	};
	password_must_change: boolean;
}

// Type-safe API methods
export const apiMethods = {
	// Auth
	auth: {
		login: (provider: string) => api.get<{ redirect_url: string }>(`/auth/${provider}/login`),
		loginWithPassword: (username: string, password: string) =>
			api.post<PasswordLoginResponse>('/auth/login', { username, password }, { skipAuth: true }),
		callback: (provider: string, code: string, state: string) =>
			api.post<{ user: import('./types').User; tokens: import('./types').AuthTokens }>(`/auth/${provider}/callback`, { code, state }),
		logout: () => api.post<void>('/auth/logout'),
		me: () => api.get<import('./types').User>('/auth/me'),
		changePassword: (currentPassword: string, newPassword: string) =>
			api.post<{ message: string }>('/auth/change-password', {
				current_password: currentPassword,
				new_password: newPassword
			}),
		setupStatus: () => api.get<{ setup_required: boolean }>('/auth/setup', { skipAuth: true }),
		setup: (data: { username: string; email: string; password: string; org_name?: string }) =>
			api.post<PasswordLoginResponse>('/auth/setup', data, { skipAuth: true })
	},

	// Dashboard
	dashboard: {
		stats: (windowKey?: string) =>
			api.get<import('./types').DashboardStats>('/api/v1/dashboard/stats', {
				params: windowKey ? { window: windowKey } : {}
			}),
		recentRuns: (limit = 10, windowKey?: string) =>
			api.get<import('./types').RecentRun[]>('/api/v1/dashboard/recent-runs', {
				params: {
					limit,
					...(windowKey ? { window: windowKey } : {})
				}
			})
	},

	// Projects
	projects: {
		list: (params?: import('./types').ListProjectsParams) =>
			api.get<import('./types').PaginatedResponse<import('./types').Project>>('/api/v1/projects', { params }),
		get: (id: string) => api.get<import('./types').Project>(`/api/v1/projects/${id}`),
		getBySlug: (slug: string) => api.get<import('./types').Project>(`/api/v1/projects/by-slug/${slug}`),
		create: (data: import('./types').CreateProjectInput) =>
			api.post<import('./types').Project>('/api/v1/projects', data),
		update: (id: string, data: import('./types').UpdateProjectInput) =>
			api.patch<import('./types').Project>(`/api/v1/projects/${id}`, data),
		delete: (id: string) => api.delete<void>(`/api/v1/projects/${id}`)
	},

	/** Project-scoped webhook registrations (SCM + generic fan-out). */
	projectWebhooks: {
		list: (projectId: string) =>
			api.get<import('./types').ProjectWebhookRegistration[]>(`/api/v1/projects/${projectId}/webhooks`),
		setup: (projectId: string, body: import('./types').SetupScmWebhookInput) =>
			api.post<import('./types').SetupScmWebhookResponse>(
				`/api/v1/projects/${projectId}/scm/setup`,
				body
			),
		patch: (projectId: string, registrationId: string, body: import('./types').PatchProjectWebhookInput) =>
			api.patch<import('./types').ProjectWebhookRegistration>(
				`/api/v1/projects/${projectId}/webhooks/${registrationId}`,
				body
			),
		rotateInboundSecret: (projectId: string, registrationId: string) =>
			api.post<import('./types').RotateProjectWebhookSecretResponse>(
				`/api/v1/projects/${projectId}/webhooks/${registrationId}/rotate-inbound-secret`,
				{}
			),
		clearInboundSecret: (projectId: string, registrationId: string) =>
			api.post<import('./types').ProjectWebhookRegistration>(
				`/api/v1/projects/${projectId}/webhooks/${registrationId}/clear-inbound-secret`,
				{}
			),
		listTargets: (projectId: string, registrationId: string) =>
			api.get<import('./types').WebhookRegistrationTargetRow[]>(
				`/api/v1/projects/${projectId}/webhooks/${registrationId}/targets`
			),
		addTarget: (
			projectId: string,
			registrationId: string,
			body: { pipeline_id: string; enabled?: boolean; filter_config?: Record<string, unknown> }
		) =>
			api.post<import('./types').WebhookRegistrationTargetRow>(
				`/api/v1/projects/${projectId}/webhooks/${registrationId}/targets`,
				body
			),
		deleteTarget: (projectId: string, registrationId: string, targetId: string) =>
			api.delete<void>(
				`/api/v1/projects/${projectId}/webhooks/${registrationId}/targets/${targetId}`
			)
	},

	// Platform stored secrets (encrypted at rest; values never returned)
	storedSecrets: {
		list: (projectId: string, params?: { pipeline_id?: string }) =>
			api.get<StoredSecret[]>(`/api/v1/projects/${projectId}/stored-secrets`, {
				params
			}),
		listVersions: (
			projectId: string,
			params: { path: string; pipeline_id?: string; organization_wide?: boolean }
		) =>
			api.get<StoredSecret[]>(`/api/v1/projects/${projectId}/stored-secret-versions`, {
				params: {
					path: params.path,
					...(params.pipeline_id ? { pipeline_id: params.pipeline_id } : {}),
					...(params.organization_wide ? { organization_wide: true } : {})
				}
			}),
		create: (
			projectId: string,
			body: {
				path: string;
				kind: string;
				value: string;
				description?: string;
				pipeline_id?: string;
				/** `"organization"` for org-wide secrets (requires org admin) */
				scope?: string;
				/** Org-wide only; default true. When false, secret is not exposed to pipelines/projects (e.g. workflow catalog import from source code only). */
				propagate_to_projects?: boolean;
			}
		) => api.post<StoredSecret>(`/api/v1/projects/${projectId}/stored-secrets`, body),
		rotate: (id: string, value: string) =>
			api.post<StoredSecret>(`/api/v1/stored-secrets/${id}/rotate`, { value }),
		activateVersion: (id: string) =>
			api.post<{
				message: string;
				invalidated_newer_versions: number;
				activated: StoredSecret;
			}>(`/api/v1/stored-secrets/${id}/activate`, {}),
		delete: (id: string) => api.delete<{ message: string }>(`/api/v1/stored-secrets/${id}`),
		purgeVersionPermanent: (id: string) =>
			api.delete<{ message: string }>(`/api/v1/stored-secrets/${id}/permanent`)
	},

	// Environment variables (project / pipeline scope)
	variables: {
		list: (projectId: string) =>
			api.get<import('./types').PaginatedResponse<import('./types').ProjectVariable>>(
				`/api/v1/projects/${projectId}/variables`
			),
		create: (
			projectId: string,
			body: {
				name: string;
				value: string;
				is_sensitive?: boolean;
				pipeline_id?: string;
				scope?: string;
			}
		) =>
			api.post<import('./types').ProjectVariable>(`/api/v1/projects/${projectId}/variables`, {
				scope: 'project',
				...body
			}),
		update: (id: string, body: { name?: string; value?: string; is_sensitive?: boolean }) =>
			api.patch<import('./types').ProjectVariable>(`/api/v1/variables/${id}`, body),
		delete: (id: string) => api.delete<{ message: string }>(`/api/v1/variables/${id}`)
	},

	/** Cross-project hub: variables and stored secrets with search + cursor pagination */
	workspaceConfig: {
		listVariables: (params?: {
			q?: string;
			project_id?: string;
			pipeline_id?: string;
			scope_level?: import('./types').WorkspaceScopeLevel;
			cursor?: string;
			per_page?: number;
		}) =>
			api.get<import('./types').PaginatedResponse<import('./types').WorkspaceVariableListItem>>(
				'/api/v1/workspace/variables',
				{ params }
			),
		listStoredSecrets: (params?: {
			q?: string;
			project_id?: string;
			pipeline_id?: string;
			scope_level?: import('./types').WorkspaceScopeLevel;
			cursor?: string;
			per_page?: number;
		}) =>
			api.get<import('./types').PaginatedResponse<import('./types').WorkspaceStoredSecretListItem>>(
				'/api/v1/workspace/stored-secrets',
				{ params }
			)
	},

	// Pipelines
	pipelines: {
		list: (params: import('./types').ListPipelinesParams) =>
			api.get<import('./types').PaginatedResponse<import('./types').Pipeline>>('/api/v1/pipelines', { params }),
		get: (id: string) => api.get<import('./types').Pipeline>(`/api/v1/pipelines/${id}`),
		getBySlug: (projectId: string, slug: string) =>
			api.get<import('./types').Pipeline>(`/api/v1/pipelines/by-slug/${projectId}/${slug}`),
		create: (data: import('./types').CreatePipelineInput) =>
			api.post<import('./types').Pipeline>('/api/v1/pipelines', data),
		importGit: (projectId: string, data: import('./types').ImportPipelineGitInput) =>
			api.post<import('./types').Pipeline>(
				`/api/v1/projects/${projectId}/pipelines/import-git`,
				data
			),
		syncFromGit: (id: string, data?: { git_ref?: string }) =>
			api.post<import('./types').Pipeline>(`/api/v1/pipelines/${id}/sync-from-git`, data ?? {}),
		update: (id: string, data: import('./types').UpdatePipelineInput) =>
			api.put<import('./types').Pipeline>(`/api/v1/pipelines/${id}`, data),
		delete: (id: string) => api.delete<void>(`/api/v1/pipelines/${id}`),
		trigger: (id: string, data?: import('./types').TriggerRunInput) =>
			api.post<import('./types').TriggerRunResponse>(`/api/v1/pipelines/${id}/trigger`, data ?? {}),
		workflowDiagnostics: (
			id: string,
			params?: { commit_sha?: string; branch?: string }
		) =>
			api.get<import('./types').WorkflowDiagnosticItem[]>(
				`/api/v1/pipelines/${id}/workflow-diagnostics`,
				{ params }
			)
	},

	triggers: {
		list: (pipelineId: string) =>
			api.get<import('./types').PipelineTrigger[]>(`/api/v1/pipelines/${pipelineId}/triggers`),
		create: (pipelineId: string, body: import('./types').CreatePipelineTriggerInput) =>
			api.post<import('./types').PipelineTrigger>(`/api/v1/pipelines/${pipelineId}/triggers`, body),
		update: (triggerId: string, body: import('./types').UpdatePipelineTriggerInput) =>
			api.patch<import('./types').PipelineTrigger>(`/api/v1/triggers/${triggerId}`, body),
		delete: (triggerId: string) => api.delete<void>(`/api/v1/triggers/${triggerId}`)
	},

	// Org workflow catalog (global reusable workflows)
	wfCatalog: {
		list: (params?: { status?: string; limit?: number; cursor?: string }) =>
			api.get<import('./types').PaginatedResponse<import('./types').CatalogWorkflow>>(
				'/api/v1/workflows/catalog',
				{ params }
			),
		/** Import using organization-scoped GitHub App secrets only (`org:admin`). */
		importGitOrganization: (body: {
			repository: string;
			git_ref: string;
			workflow_path: string;
			credentials_path: string;
		}) =>
			api.post<import('./types').CatalogWorkflow>('/api/v1/workflows/catalog/import-git', body),
		/** List branches/tags (and optional recent commits) for catalog sync. Org: `org:admin`. */
		upstreamRefSearchOrganization: (body: {
			repository: string;
			credentials_path: string;
			q?: string;
			commits_for_ref?: string;
		}) =>
			api.post<import('./types').CatalogUpstreamRefSearchResponse>(
				'/api/v1/workflows/catalog/upstream-ref-search',
				body
			),
		upstreamRefSearchProject: (
			projectId: string,
			body: {
				repository: string;
				credentials_path: string;
				q?: string;
				commits_for_ref?: string;
			}
		) =>
			api.post<import('./types').CatalogUpstreamRefSearchResponse>(
				`/api/v1/projects/${projectId}/workflows/catalog/upstream-ref-search`,
				body
			),
		importGit: (
			projectId: string,
			body: {
				repository: string;
				git_ref: string;
				workflow_path: string;
				credentials_path: string;
			}
		) =>
			api.post<import('./types').CatalogWorkflow>(
				`/api/v1/projects/${projectId}/workflows/catalog/import-git`,
				body
			),
		catalogVersions: (
			workflowId: string,
			params?: { q?: string; limit?: number; per_page?: number; cursor?: string }
		) =>
			api.get<import('./types').CatalogVersionsPage>(
				`/api/v1/workflows/${workflowId}/catalog-versions`,
				{ params }
			),
		get: (id: string) => api.get<import('./types').CatalogWorkflow>(`/api/v1/workflows/${id}`),
		/** Global (execution-gated) + project-scoped workflows for pipeline authoring */
		listAvailableForProject: (projectId: string) =>
			api.get<import('./types').ProjectWorkflowsAvailable>(
				`/api/v1/projects/${projectId}/workflows/available`
			)
	},

	artifacts: {
		sbom: (runId: string) =>
			api.get<import('./types').SbomApiResponse>(`/api/v1/runs/${runId}/sbom`)
	},

	// Runs
	runs: {
		list: (params: import('./types').ListRunsParams) =>
			api.get<import('./types').PaginatedResponse<import('./types').Run>>('/api/v1/runs', { params }),
		get: (id: string) => api.get<import('./types').Run>(`/api/v1/runs/${id}`),
		cancel: (id: string) => api.post<{ run_id: string; status: string; message: string }>(`/api/v1/runs/${id}/cancel`),
		retry: (id: string) => api.post<{ original_run_id: string; new_run_id: string; run_number: number }>(`/api/v1/runs/${id}/retry`),
		jobs: (runId: string) => api.get<import('./types').JobRun[]>(`/api/v1/runs/${runId}/jobs`),
		dag: (runId: string) => api.get<import('./types').RunDagResponse>(`/api/v1/runs/${runId}/dag`),
		footprint: (runId: string) =>
			api.get<import('./types').RunFootprintResponse>(`/api/v1/runs/${runId}/footprint`),
		jobRunSnapshots: (runId: string, jobRunId: string) =>
			api.get<import('./types').JobRunSnapshotsResponse>(
				`/api/v1/runs/${runId}/jobs/${jobRunId}/snapshots`
			),
		assignments: (runId: string, jobRunId: string) =>
			api.get<import('./types').JobAssignment[]>(`/api/v1/runs/${runId}/jobs/${jobRunId}/assignments`),
		logs: (
			runId: string,
			jobRunId: string,
			params?: { offset?: number; limit?: number; stream?: string }
		) =>
			api.get<{
				content?: string;
				lines?: import('./types').LogLinePayload[];
				offset?: number;
				has_more?: boolean;
			}>(`/api/v1/runs/${runId}/jobs/${jobRunId}/logs`, { params }),
		jobSteps: (runId: string, jobRunId: string) =>
			api.get<import('./types').StepRun[]>(`/api/v1/runs/${runId}/jobs/${jobRunId}/steps`)
	},

	// Agents
	agents: {
		list: (params?: import('./types').ListAgentsParams) =>
			api.get<import('./types').PaginatedResponse<import('./types').Agent>>('/api/v1/agents', { params }),
		get: (id: string) => api.get<import('./types').Agent>(`/api/v1/agents/${id}`),
		drain: (id: string) => api.post<{ agent_id: string; status: string; message: string }>(`/api/v1/agents/${id}/drain`),
		resume: (id: string) => api.post<{ agent_id: string; status: string; message: string }>(`/api/v1/agents/${id}/resume`),
		delete: (id: string) => api.delete<{ message: string; agent_id: string }>(`/api/v1/agents/${id}`)
	},

	// Admin
	admin: {
		// User management
		users: {
			list: (params?: { limit?: number }) =>
				api.get<import('./types').PaginatedResponse<import('./types').AdminUser>>('/admin/users', { params }),
			get: (id: string) => api.get<import('./types').AdminUser>(`/admin/users/${id}`),
			update: (id: string, data: { display_name?: string; is_admin?: boolean }) =>
				api.patch<import('./types').AdminUser>(`/admin/users/${id}`, data),
			lock: (id: string) => api.post<import('./types').AdminUser>(`/admin/users/${id}/lock`),
			unlock: (id: string) => api.post<import('./types').AdminUser>(`/admin/users/${id}/unlock`),
			delete: (id: string) => api.post<{ message: string }>(`/admin/users/${id}/delete`),
			resetPassword: (id: string, newPassword: string) =>
				api.post<{ message: string }>(`/admin/users/${id}/reset-password`, { new_password: newPassword })
		},
		// Group management
		groups: {
			list: (params?: { limit?: number }) =>
				api.get<import('./types').PaginatedResponse<import('./types').AdminGroup>>('/admin/groups', { params }),
			get: (id: string) => api.get<import('./types').AdminGroup>(`/admin/groups/${id}`),
			create: (data: { name: string; description?: string }) =>
				api.post<import('./types').AdminGroup>('/admin/groups', data),
			update: (id: string, data: { name?: string; description?: string }) =>
				api.patch<import('./types').AdminGroup>(`/admin/groups/${id}`, data),
			delete: (id: string) => api.delete<{ message: string }>(`/admin/groups/${id}`),
			listMembers: (groupId: string) =>
				api.get<import('./types').GroupMember[]>(`/admin/groups/${groupId}/members`),
			addMember: (groupId: string, userId: string, role?: string) =>
				api.post<import('./types').GroupMember>(`/admin/groups/${groupId}/members`, { user_id: userId, role: role ?? 'member' }),
			updateMember: (groupId: string, userId: string, role: string) =>
				api.patch<import('./types').GroupMember>(`/admin/groups/${groupId}/members/${userId}`, { role }),
			removeMember: (groupId: string, userId: string) =>
				api.delete<{ message: string }>(`/admin/groups/${groupId}/members/${userId}`)
		},
		// Role management
		roles: {
			list: () => api.get<import('./types').RoleInfo[]>('/admin/roles'),
			getUserRoles: (userId: string) => api.get<import('./types').UserRoleAssignment[]>(`/admin/users/${userId}/roles`),
			assign: (userId: string, role: string) => api.post<import('./types').UserRoleAssignment>(`/admin/users/${userId}/roles`, { role }),
			revoke: (userId: string, role: string) => api.delete<{ message: string }>(`/admin/users/${userId}/roles/${role}`)
		},
		// Project admin operations
		projects: {
			scheduleDeletion: (id: string, retentionDays?: number) =>
				api.post<{ message: string; scheduled_deletion_at: string }>(`/admin/projects/${id}/schedule-deletion`, { retention_days: retentionDays ?? 7 }),
			cancelDeletion: (id: string) => api.post<{ message: string }>(`/admin/projects/${id}/cancel-deletion`),
			forceDelete: (id: string) => api.post<{ message: string }>(`/admin/projects/${id}/force-delete`)
		},
		workflows: {
			approve: (workflowId: string) =>
				api.post<{ workflow: import('./types').CatalogWorkflow }>(
					`/admin/workflows/${workflowId}/approve`,
					{}
				),
			reject: (workflowId: string) =>
				api.post<{ workflow: import('./types').CatalogWorkflow }>(
					`/admin/workflows/${workflowId}/reject`,
					{}
				),
			trust: (workflowId: string) =>
				api.post<{ workflow: import('./types').CatalogWorkflow }>(
					`/admin/workflows/${workflowId}/trust`,
					{}
				),
			untrust: (workflowId: string) =>
				api.post<{ workflow: import('./types').CatalogWorkflow }>(
					`/admin/workflows/${workflowId}/untrust`,
					{}
				),
			delete: (workflowId: string) =>
				api.post<{ ok: boolean }>(`/admin/workflows/${workflowId}/delete`, {})
		},
		// Auth provider management
		authProviders: {
			list: () => api.get<import('./types').AuthProviderResponse[]>('/admin/auth-providers'),
			get: (id: string) => api.get<import('./types').AuthProviderResponse>(`/admin/auth-providers/${id}`),
			create: (data: { name: string; provider_type: string; client_id: string; client_secret: string; issuer_url?: string }) =>
				api.post<import('./types').AuthProviderResponse>('/admin/auth-providers', data),
			update: (id: string, data: { name?: string; client_id?: string; client_secret?: string; issuer_url?: string }) =>
				api.patch<import('./types').AuthProviderResponse>(`/admin/auth-providers/${id}`, data),
			enable: (id: string) => api.post<import('./types').AuthProviderResponse>(`/admin/auth-providers/${id}/enable`),
			disable: (id: string) => api.post<import('./types').AuthProviderResponse>(`/admin/auth-providers/${id}/disable`),
			delete: (id: string) => api.delete<{ message: string }>(`/admin/auth-providers/${id}`),
			groupMappings: {
				list: (providerId: string) => api.get<import('./types').GroupMappingResponse[]>(`/admin/auth-providers/${providerId}/group-mappings`),
				create: (providerId: string, data: { oidc_group_claim: string; meticulous_group_id: string; role?: string }) =>
					api.post<import('./types').GroupMappingResponse>(`/admin/auth-providers/${providerId}/group-mappings`, data),
				delete: (providerId: string, mappingId: string) => 
					api.delete<{ message: string }>(`/admin/auth-providers/${providerId}/group-mappings/${mappingId}`)
			}
		},
		ops: {
			jobQueue: (params?: { limit?: number }) =>
				api.get<{ count: number; data: JobQueueEntry[] }>('/admin/ops/job-queue', { params })
		},
		/** Meticulous Apps (machine / integration auth). */
		meticulousApps: {
			list: () => api.get<MeticulousAppSummary[]>('/admin/meticulous-apps'),
			create: (data: { name: string; description?: string }) =>
				api.post<CreateMeticulousAppResponse>('/admin/meticulous-apps', data),
			get: (applicationId: string) =>
				api.get<MeticulousAppSummary>(
					`/admin/meticulous-apps/${encodeURIComponent(applicationId)}`
				),
			addKey: (applicationId: string) =>
				api.post<{ key_id: string; private_key_pem: string }>(
					`/admin/meticulous-apps/${encodeURIComponent(applicationId)}/keys`,
					{}
				),
			revokeKey: (applicationId: string, keyId: string) =>
				api.post<{ message: string }>(
					`/admin/meticulous-apps/${encodeURIComponent(applicationId)}/keys/${encodeURIComponent(keyId)}/revoke`,
					{}
				),
			listInstallations: (applicationId: string) =>
				api.get<MeticulousAppInstallationRow[]>(
					`/admin/meticulous-apps/${encodeURIComponent(applicationId)}/installations`
				),
			createInstallation: (
				applicationId: string,
				body: { project_id: string; permissions: string[] }
			) =>
				api.post<MeticulousAppInstallationRow>(
					`/admin/meticulous-apps/${encodeURIComponent(applicationId)}/installations`,
					body
				),
			revokeInstallation: (applicationId: string, installationId: string) =>
				api.post<{ message: string }>(
					`/admin/meticulous-apps/${encodeURIComponent(applicationId)}/installations/${encodeURIComponent(installationId)}/revoke`,
					{}
				)
		}
	}
};

/** Admin: Meticulous App row (no secrets). */
export interface MeticulousAppSummary {
	id: string;
	application_id: string;
	name: string;
	description?: string | null;
	created_at: string;
}

/** Admin: response when creating an app or rotating a key (private key once). */
export interface CreateMeticulousAppResponse {
	app: MeticulousAppSummary;
	key_id: string;
	private_key_pem: string;
}

/** Admin: app installation row. */
export interface MeticulousAppInstallationRow {
	id: string;
	project_id: string;
	permissions: string[];
	created_at: string;
	revoked_at?: string | null;
}

/** Admin job queue row (`/admin/ops/job-queue`). */
export interface JobQueueEntry {
	job_run_id?: string;
	run_id: string;
	job_id?: string;
	job_name: string;
	job_status: string;
	attempt: number;
	job_run_created_at: string;
	run_number: number;
	run_status: string;
	pipeline_id: string;
	pipeline_name: string;
	project_id: string;
	project_slug: string;
}
