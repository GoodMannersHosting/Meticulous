<script lang="ts">
	import { auth } from '$stores';
	import { Loader2, AlertTriangle } from 'lucide-svelte';
	import { Button } from '$components/ui';
	import { onMount } from 'svelte';

	let { data } = $props();

	let error = $state<string | null>(null);

	onMount(async () => {
		if (data.token) {
			try {
				await auth.handleOAuthToken(data.token);
			} catch (e) {
				error = e instanceof Error ? e.message : 'Authentication failed';
			}
		}
	});
</script>

<svelte:head>
	<title>Authenticating... | Meticulous</title>
</svelte:head>

<div class="flex min-h-screen items-center justify-center bg-[var(--bg-primary)] px-4">
	<div class="w-full max-w-sm text-center">
		{#if error}
			<div class="rounded-xl border border-[var(--border-primary)] bg-[var(--bg-secondary)] p-6 shadow-sm">
				<div class="mx-auto mb-4 flex h-12 w-12 items-center justify-center rounded-full bg-error-100 dark:bg-error-900/30">
					<AlertTriangle class="h-6 w-6 text-error-600 dark:text-error-500" />
				</div>

				<h1 class="text-lg font-semibold text-[var(--text-primary)]">
					Authentication Failed
				</h1>

				<p class="mt-2 text-sm text-[var(--text-secondary)]">
					{error}
				</p>

				<div class="mt-6">
					<Button variant="primary" href="/auth/login">
						Try again
					</Button>
				</div>
			</div>
		{:else}
			<div class="flex flex-col items-center">
				<Loader2 class="h-10 w-10 animate-spin text-primary-600" />

				<h1 class="mt-4 text-lg font-semibold text-[var(--text-primary)]">
					Completing sign in...
				</h1>

				<p class="mt-1 text-sm text-[var(--text-secondary)]">
					Please wait while we verify your credentials.
				</p>
			</div>
		{/if}
	</div>
</div>
