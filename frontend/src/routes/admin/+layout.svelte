<script lang="ts">
	import { goto } from '$app/navigation';
	import { auth } from '$stores';
	import { AlertTriangle, Loader2, Shield } from 'lucide-svelte';

	let { children } = $props();

	const isAuthorized = $derived(auth.user?.role === 'admin');
	const isLoading = $derived(auth.isLoading);

	$effect(() => {
		if (!isLoading && !auth.isAuthenticated) {
			goto('/auth/login?redirect=/admin');
		}
	});
</script>

{#if isLoading}
	<div class="flex min-h-[50vh] items-center justify-center">
		<div class="flex flex-col items-center gap-4">
			<Loader2 class="h-8 w-8 animate-spin text-primary-500" />
			<p class="text-sm text-[var(--text-secondary)]">Loading...</p>
		</div>
	</div>
{:else if !auth.isAuthenticated}
	<div class="flex min-h-[50vh] items-center justify-center">
		<div class="flex flex-col items-center gap-4">
			<Loader2 class="h-8 w-8 animate-spin text-primary-500" />
			<p class="text-sm text-[var(--text-secondary)]">Redirecting to login...</p>
		</div>
	</div>
{:else if !isAuthorized}
	<div class="flex min-h-[50vh] items-center justify-center">
		<div class="mx-auto max-w-md rounded-lg border border-red-200 bg-red-50 p-6 dark:border-red-900 dark:bg-red-950/50">
			<div class="flex items-center gap-3">
				<AlertTriangle class="h-6 w-6 text-red-600 dark:text-red-400" />
				<h2 class="text-lg font-semibold text-red-800 dark:text-red-200">Access Denied</h2>
			</div>
			<p class="mt-3 text-sm text-red-700 dark:text-red-300">
				You don't have permission to access the admin area.
				Only users with admin privileges can view this section.
			</p>
			<div class="mt-4 flex gap-3">
				<a
					href="/dashboard"
					class="inline-flex items-center gap-2 rounded-lg bg-red-600 px-4 py-2 text-sm font-medium text-white hover:bg-red-700 dark:bg-red-700 dark:hover:bg-red-600"
				>
					Go to Dashboard
				</a>
			</div>
		</div>
	</div>
{:else}
	<div class="mb-6">
		<div class="flex items-center gap-3">
			<div class="flex h-10 w-10 items-center justify-center rounded-lg bg-primary-100 dark:bg-primary-900/30">
				<Shield class="h-5 w-5 text-primary-600 dark:text-primary-400" />
			</div>
			<div>
				<h1 class="text-xl font-semibold text-[var(--text-primary)]">Administration</h1>
				<p class="text-sm text-[var(--text-secondary)]">Manage users, groups, and system settings</p>
			</div>
		</div>
	</div>

	{@render children()}
{/if}
