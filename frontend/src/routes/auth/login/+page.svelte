<script lang="ts">
	import { auth } from '$stores';
	import { Button, Input } from '$components/ui';
	import { Loader2, LogIn } from 'lucide-svelte';

	let loading = $state(false);
	let error = $state<string | null>(null);

	let username = $state('');
	let password = $state('');

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

			<div class="mt-4 text-center text-xs text-[var(--text-tertiary)]">
				Default credentials: <code class="rounded bg-[var(--bg-hover)] px-1 py-0.5">admin</code> / <code class="rounded bg-[var(--bg-hover)] px-1 py-0.5">adminadmin</code>
			</div>
		</div>
	</div>
</div>
