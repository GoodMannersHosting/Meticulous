<script lang="ts">
	import { auth } from '$stores';
	import { Button, Input } from '$components/ui';
	import { Loader2, LogIn, Globe, Github } from 'lucide-svelte';
	import { onMount } from 'svelte';
	import { getPublicApiBase } from '$lib/public-api-base';

	interface AuthProvider {
		id: string;
		name: string;
		provider_type: 'oidc' | 'github';
	}

	interface AuthProvidersResponse {
		password_enabled: boolean;
		show_bootstrap_credentials_hint: boolean;
		providers: AuthProvider[];
	}

	let loading = $state(false);
	let error = $state<string | null>(null);
	let providersLoading = $state(true);
	let passwordEnabled = $state(true);
	let showBootstrapCredentialsHint = $state(false);
	let providers = $state<AuthProvider[]>([]);

	let username = $state('');
	let password = $state('');

	onMount(async () => {
		try {
			const response = await fetch(`${getPublicApiBase()}/auth/providers`);
			if (response.ok) {
				const data: AuthProvidersResponse = await response.json();
				passwordEnabled = data.password_enabled;
				showBootstrapCredentialsHint = data.show_bootstrap_credentials_hint ?? false;
				providers = data.providers;
			}
		} catch (e) {
			console.error('Failed to load auth providers:', e);
		} finally {
			providersLoading = false;
		}
	});

	async function handlePasswordLogin(event: Event) {
		event.preventDefault();
		loading = true;
		error = null;

		try {
			await auth.loginWithPassword(username, password);
		} catch (e) {
			error = e instanceof Error ? e.message : 'Login failed';
			loading = false;
		}
	}

	function loginWithProvider(provider: AuthProvider) {
		const redirectUri = encodeURIComponent(window.location.origin + '/oauth/callback');
		const loginUrl = `${getPublicApiBase()}/auth/oauth/${provider.id}/login?redirect_uri=${redirectUri}`;
		window.location.href = loginUrl;
	}
</script>

<svelte:head>
	<title>Sign in | Meticulous</title>
</svelte:head>

<div class="flex min-h-screen items-center justify-center bg-[var(--bg-primary)] px-4">
	<div class="w-full max-w-sm">
		<div class="mb-8 text-center">
			<div class="mx-auto mb-4 flex h-12 w-12 items-center justify-center rounded-xl bg-gradient-to-br from-primary-500 to-primary-700">
				<svg class="h-7 w-7 text-white" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
					<path d="M5 12L10 17L20 7" />
				</svg>
			</div>

			<h1 class="text-2xl font-bold text-[var(--text-primary)]">
				Welcome to Meticulous
			</h1>

			<p class="mt-2 text-[var(--text-secondary)]">
				Sign in to your account
			</p>
		</div>

		<div class="rounded-xl border border-[var(--border-primary)] bg-[var(--bg-secondary)] p-6 shadow-sm">
			{#if error}
				<div class="mb-4 rounded-lg bg-error-50 p-3 text-sm text-error-700 dark:bg-error-900/20 dark:text-error-500">
					{error}
				</div>
			{/if}

			{#if providersLoading}
				<div class="flex items-center justify-center py-8">
					<Loader2 class="h-6 w-6 animate-spin text-[var(--text-tertiary)]" />
				</div>
			{:else}
				<!-- SSO Providers -->
				{#if providers.length > 0}
					<div class="space-y-3">
						{#each providers as provider (provider.id)}
							<button
								type="button"
								onclick={() => loginWithProvider(provider)}
								class="flex w-full items-center justify-center gap-3 rounded-lg border border-[var(--border-primary)] bg-[var(--bg-primary)] px-4 py-3 text-sm font-medium text-[var(--text-primary)] transition-colors hover:bg-[var(--bg-hover)]"
							>
								{#if provider.provider_type === 'github'}
									<Github class="h-5 w-5" />
								{:else}
									<Globe class="h-5 w-5" />
								{/if}
								Continue with {provider.name}
							</button>
						{/each}
					</div>

					{#if passwordEnabled}
						<div class="relative my-6">
							<div class="absolute inset-0 flex items-center">
								<div class="w-full border-t border-[var(--border-primary)]"></div>
							</div>
							<div class="relative flex justify-center text-xs uppercase">
								<span class="bg-[var(--bg-secondary)] px-2 text-[var(--text-tertiary)]">Or continue with password</span>
							</div>
						</div>
					{/if}
				{/if}

				<!-- Password Login -->
				{#if passwordEnabled}
					<form onsubmit={handlePasswordLogin} class="space-y-4">
						<div>
							<label for="username" class="block text-sm font-medium text-[var(--text-primary)] mb-1">
								Username
							</label>
							<Input
								id="username"
								type="text"
								bind:value={username}
								placeholder="Enter your username"
								required
								disabled={loading}
								autocomplete="username"
							/>
						</div>

						<div>
							<label for="password" class="block text-sm font-medium text-[var(--text-primary)] mb-1">
								Password
							</label>
							<Input
								id="password"
								type="password"
								bind:value={password}
								placeholder="Enter your password"
								required
								disabled={loading}
								autocomplete="current-password"
							/>
						</div>

						<Button
							variant="primary"
							type="submit"
							class="w-full"
							disabled={loading || !username || !password}
						>
							{#if loading}
								<Loader2 class="h-5 w-5 animate-spin" />
								Signing in...
							{:else}
								<LogIn class="h-5 w-5" />
								Sign in
							{/if}
						</Button>
					</form>

					{#if showBootstrapCredentialsHint}
						<div class="mt-4 text-center text-xs text-[var(--text-tertiary)]">
							Default credentials: <code class="rounded bg-[var(--bg-hover)] px-1 py-0.5">admin</code> / <code class="rounded bg-[var(--bg-hover)] px-1 py-0.5">adminadmin</code>
						</div>
					{/if}
				{:else if providers.length === 0}
					<div class="py-8 text-center text-sm text-[var(--text-secondary)]">
						No authentication methods configured. Contact your administrator.
					</div>
				{/if}
			{/if}
		</div>
	</div>
</div>
