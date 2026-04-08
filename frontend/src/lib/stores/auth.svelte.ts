import { browser } from '$app/environment';
import { goto } from '$app/navigation';
import { apiMethods } from '$api/client';
import type { User } from '$api/types';

export type AuthState = 'loading' | 'authenticated' | 'unauthenticated';

function safeInternalRedirect(raw: string | null): string | null {
	if (!raw || !raw.startsWith('/') || raw.startsWith('//')) {
		return null;
	}
	return raw;
}

/** After login / OAuth: honor `?redirect=` on the current page when safe. */
function gotoAfterSuccessfulAuth(user: User): void {
	if (user.password_must_change) {
		goto('/change-password');
		return;
	}
	if (browser) {
		const next = safeInternalRedirect(
			new URLSearchParams(window.location.search).get('redirect'),
		);
		if (next) {
			goto(next);
			return;
		}
	}
	goto('/dashboard');
}

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

			const user = await apiMethods.auth.me();
			this.#user = user;
			this.#state = 'authenticated';

			gotoAfterSuccessfulAuth(user);
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

			const { tokens } = await apiMethods.auth.callback(provider, code, state);

			if (browser) {
				localStorage.setItem('auth_token', tokens.access_token);
				if (tokens.refresh_token) {
					localStorage.setItem('refresh_token', tokens.refresh_token);
				}
			}

			const me = await apiMethods.auth.me();
			this.#user = me;
			this.#state = 'authenticated';

			gotoAfterSuccessfulAuth(me);
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

			gotoAfterSuccessfulAuth(user);
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

			goto('/login');
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
