<script lang="ts">
	import { auth } from '$stores';
	import { Button, Input } from '$components/ui';
	import { Loader2, KeyRound } from 'lucide-svelte';
	import { apiMethods, ApiClientError } from '$api/client';
	import { goto } from '$app/navigation';

	let currentPassword = $state('');
	let newPassword = $state('');
	let confirmPassword = $state('');
	let loading = $state(false);
	let error = $state<string | null>(null);

	$effect(() => {
		if (auth.state === 'loading') return;
		if (auth.state === 'unauthenticated') {
			goto('/login');
		}
	});

	async function handleSubmit(event: Event) {
		event.preventDefault();
		if (newPassword !== confirmPassword) {
			error = 'New passwords do not match';
			return;
		}
		loading = true;
		error = null;
		try {
			await apiMethods.auth.changePassword(currentPassword, newPassword);
			const user = await apiMethods.auth.me();
			auth.setUser(user);
			goto('/dashboard');
		} catch (e) {
			error =
				e instanceof ApiClientError
					? e.message
					: e instanceof Error
						? e.message
						: 'Failed to change password';
		} finally {
			loading = false;
		}
	}
</script>

<svelte:head>
	<title>Change password | Meticulous</title>
</svelte:head>

<div class="flex min-h-screen items-center justify-center bg-[var(--bg-primary)] px-4">
	<div class="w-full max-w-sm">
		<div class="mb-8 text-center">
			<div
				class="mx-auto mb-4 flex h-12 w-12 items-center justify-center rounded-xl bg-gradient-to-br from-primary-500 to-primary-700"
			>
				<KeyRound class="h-7 w-7 text-white" />
			</div>

			<h1 class="text-2xl font-bold text-[var(--text-primary)]">Set a new password</h1>

			<p class="mt-2 text-[var(--text-secondary)]">
				For security, you must change your password before continuing.
			</p>
		</div>

		<div class="rounded-xl border border-[var(--border-primary)] bg-[var(--bg-secondary)] p-6 shadow-sm">
			{#if error}
				<div
					class="mb-4 rounded-lg bg-error-50 p-3 text-sm text-error-700 dark:bg-error-900/20 dark:text-error-500"
				>
					{error}
				</div>
			{/if}

			<form onsubmit={handleSubmit} class="space-y-4">
				<div>
					<label for="current" class="mb-1 block text-sm font-medium text-[var(--text-primary)]">
						Current password
					</label>
					<Input
						id="current"
						type="password"
						bind:value={currentPassword}
						required
						disabled={loading}
						autocomplete="current-password"
					/>
				</div>

				<div>
					<label for="new" class="mb-1 block text-sm font-medium text-[var(--text-primary)]">
						New password
					</label>
					<Input
						id="new"
						type="password"
						bind:value={newPassword}
						required
						disabled={loading}
						autocomplete="new-password"
					/>
				</div>

				<div>
					<label for="confirm" class="mb-1 block text-sm font-medium text-[var(--text-primary)]">
						Confirm new password
					</label>
					<Input
						id="confirm"
						type="password"
						bind:value={confirmPassword}
						required
						disabled={loading}
						autocomplete="new-password"
					/>
				</div>

				<Button variant="primary" type="submit" class="w-full" disabled={loading}>
					{#if loading}
						<Loader2 class="h-5 w-5 animate-spin" />
						Updating…
					{:else}
						<KeyRound class="h-5 w-5" />
						Update password
					{/if}
				</Button>
			</form>

			<p class="mt-6 text-center text-xs text-[var(--text-tertiary)]">
				<button
					type="button"
					class="text-primary-600 hover:underline dark:text-primary-400"
					onclick={() => auth.logout()}
				>
					Sign out
				</button>
			</p>
		</div>
	</div>
</div>
