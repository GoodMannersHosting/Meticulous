import { browser } from '$app/environment';
import { goto } from '$app/navigation';
import { PUBLIC_API_URL } from '$env/static/public';
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
		this.baseUrl = browser ? (PUBLIC_API_URL ?? '') : '';
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
		const url = new URL(endpoint, this.baseUrl);

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
				goto('/auth/login');
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

		if (response.status === 204) {
			return undefined as T;
		}

		return response.json();
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
		stats: () => api.get<import('./types').DashboardStats>('/api/v1/dashboard/stats'),
		recentRuns: (limit = 10) =>
			api.get<import('./types').RecentRun[]>('/api/v1/dashboard/recent-runs', { params: { limit } })
	},

	// Projects
	projects: {
		list: (params?: import('./types').ListProjectsParams) =>
			api.get<import('./types').PaginatedResponse<import('./types').Project>>('/api/v1/projects', { params }),
		get: (id: string) => api.get<import('./types').Project>(`/api/v1/projects/${id}`),
		getBySlug: (slug: string) => api.get<import('./types').Project>(`/api/v1/projects/by-slug/${slug}`),
		create: (data: import('./types').CreateProjectInput) =>
			api.post<import('./types').Project>('/api/v1/projects', data),
		update: (id: string, data: Partial<import('./types').Project>) =>
			api.patch<import('./types').Project>(`/api/v1/projects/${id}`, data),
		delete: (id: string) => api.delete<void>(`/api/v1/projects/${id}`)
	},

	// Platform stored secrets (encrypted at rest; values never returned)
	storedSecrets: {
		list: (projectId: string, params?: { pipeline_id?: string }) =>
			api.get<StoredSecret[]>(`/api/v1/projects/${projectId}/stored-secrets`, {
				params
			}),
		create: (
			projectId: string,
			body: {
				path: string;
				kind: string;
				value: string;
				description?: string;
				pipeline_id?: string;
			}
		) => api.post<StoredSecret>(`/api/v1/projects/${projectId}/stored-secrets`, body),
		rotate: (id: string, value: string) =>
			api.post<StoredSecret>(`/api/v1/stored-secrets/${id}/rotate`, { value }),
		delete: (id: string) => api.delete<{ message: string }>(`/api/v1/stored-secrets/${id}`)
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
		update: (id: string, data: Partial<import('./types').Pipeline>) =>
			api.put<import('./types').Pipeline>(`/api/v1/pipelines/${id}`, data),
		delete: (id: string) => api.delete<void>(`/api/v1/pipelines/${id}`),
		trigger: (id: string, data?: import('./types').TriggerRunInput) =>
			api.post<import('./types').TriggerRunResponse>(`/api/v1/pipelines/${id}/trigger`, data ?? {})
	},

	// Runs
	runs: {
		list: (params: import('./types').ListRunsParams) =>
			api.get<import('./types').PaginatedResponse<import('./types').Run>>('/api/v1/runs', { params }),
		get: (id: string) => api.get<import('./types').Run>(`/api/v1/runs/${id}`),
		cancel: (id: string) => api.post<{ run_id: string; status: string; message: string }>(`/api/v1/runs/${id}/cancel`),
		retry: (id: string) => api.post<{ original_run_id: string; new_run_id: string; run_number: number }>(`/api/v1/runs/${id}/retry`),
		jobs: (runId: string) => api.get<import('./types').JobRun[]>(`/api/v1/runs/${runId}/jobs`),
		logs: (runId: string, jobRunId: string) =>
			api.get<{ lines: import('./types').LogLinePayload[] }>(`/api/v1/runs/${runId}/jobs/${jobRunId}/logs`)
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
		}
	}
};

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
