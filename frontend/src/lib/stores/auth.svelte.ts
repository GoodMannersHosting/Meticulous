import { browser } from '$app/environment';
import { goto } from '$app/navigation';
import { apiMethods } from '$api/client';
import type { User } from '$api/types';

export type AuthState = 'loading' | 'authenticated' | 'unauthenticated';

class AuthStore {
	#state = $state<AuthState>('loading');
	#user = $state<User | null>(null);
	#error = $state<string | null>(null);

	constructor() {
		if (browser) {
			this.initialize();
		}
	}

	private async initialize(): Promise<void> {
		const token = localStorage.getItem('auth_token');

		if (!token) {
			this.#state = 'unauthenticated';
			return;
		}

		try {
			const user = await apiMethods.auth.me();
			this.#user = user;
			this.#state = 'authenticated';
		} catch {
			// Token is invalid
			localStorage.removeItem('auth_token');
			this.#state = 'unauthenticated';
		}
	}

	get state(): AuthState {
		return this.#state;
	}

	get user(): User | null {
		return this.#user;
	}

	get error(): string | null {
		return this.#error;
	}

	get isAuthenticated(): boolean {
		return this.#state === 'authenticated';
	}

	get isLoading(): boolean {
		return this.#state === 'loading';
	}

	async login(provider: string = 'github'): Promise<void> {
		try {
			this.#error = null;
			const { redirect_url } = await apiMethods.auth.login(provider);
			window.location.href = redirect_url;
		} catch (err) {
			this.#error = err instanceof Error ? err.message : 'Failed to initiate login';
			throw err;
		}
	}

	async loginWithPassword(username: string, password: string): Promise<void> {
		try {
			this.#state = 'loading';
			this.#error = null;

			const response = await apiMethods.auth.loginWithPassword(username, password);

			if (browser) {
				localStorage.setItem('auth_token', response.token);
			}

			this.#user = {
				id: response.user.id,
				org_id: '', // Will be populated from /auth/me
				name: response.user.display_name || response.user.username,
				email: response.user.email,
				role: response.user.is_admin ? 'admin' : 'user',
				created_at: new Date().toISOString()
			};
			this.#state = 'authenticated';

			goto('/dashboard');
		} catch (err) {
			this.#state = 'unauthenticated';
			this.#error = err instanceof Error ? err.message : 'Login failed';
			throw err;
		}
	}

	async handleCallback(provider: string, code: string, state: string): Promise<void> {
		try {
			this.#state = 'loading';
			this.#error = null;

			const { user, tokens } = await apiMethods.auth.callback(provider, code, state);

			if (browser) {
				localStorage.setItem('auth_token', tokens.access_token);
				if (tokens.refresh_token) {
					localStorage.setItem('refresh_token', tokens.refresh_token);
				}
			}

			this.#user = user;
			this.#state = 'authenticated';

			goto('/dashboard');
		} catch (err) {
			this.#state = 'unauthenticated';
			this.#error = err instanceof Error ? err.message : 'Authentication failed';
			throw err;
		}
	}

	async handleOAuthToken(token: string): Promise<void> {
		try {
			this.#state = 'loading';
			this.#error = null;

			if (browser) {
				localStorage.setItem('auth_token', token);
			}

			const user = await apiMethods.auth.me();
			this.#user = user;
			this.#state = 'authenticated';

			goto('/dashboard');
		} catch (err) {
			if (browser) {
				localStorage.removeItem('auth_token');
			}
			this.#state = 'unauthenticated';
			this.#error = err instanceof Error ? err.message : 'Authentication failed';
			throw err;
		}
	}

	async logout(): Promise<void> {
		try {
			await apiMethods.auth.logout();
		} catch {
			// Ignore logout errors
		} finally {
			if (browser) {
				localStorage.removeItem('auth_token');
				localStorage.removeItem('refresh_token');
			}

			this.#user = null;
			this.#state = 'unauthenticated';
			this.#error = null;

			goto('/auth/login');
		}
	}

	setUser(user: User): void {
		this.#user = user;
		this.#state = 'authenticated';
	}

	clearError(): void {
		this.#error = null;
	}
}

export const auth = new AuthStore();
