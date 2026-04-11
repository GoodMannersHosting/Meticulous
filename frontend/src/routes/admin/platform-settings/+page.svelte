<script lang="ts">
	import { onMount } from 'svelte';
	import { Button, Alert } from '$components/ui';
	import { apiMethods } from '$api/client';

	let loading = $state(true);
	let saving = $state(false);
	let error = $state<string | null>(null);
	let success = $state<string | null>(null);
	let allowUnauth = $state(false);

	onMount(async () => {
		try {
			const settings = await apiMethods.platformSettings.get();
			allowUnauth = settings.allow_unauthenticated_access;
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load settings';
		} finally {
			loading = false;
		}
	});

	async function save() {
		saving = true;
		error = null;
		success = null;
		try {
			const updated = await apiMethods.platformSettings.update({
				allow_unauthenticated_access: allowUnauth
			});
			allowUnauth = updated.allow_unauthenticated_access;
			success = 'Settings saved';
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to save';
		} finally {
			saving = false;
		}
	}
</script>

<div class="space-y-6">
	<h2 class="text-xl font-semibold text-[var(--text-primary)]">Platform Settings</h2>

	{#if error}
		<Alert variant="error">{error}</Alert>
	{/if}
	{#if success}
		<Alert variant="success">{success}</Alert>
	{/if}

	{#if loading}
		<p class="text-sm text-[var(--text-secondary)]">Loading...</p>
	{:else}
		<div
			class="max-w-xl rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] p-6"
		>
			<h3 class="mb-4 text-sm font-medium text-[var(--text-primary)]">Public Access</h3>

			<label class="flex items-start gap-3">
				<input
					type="checkbox"
					bind:checked={allowUnauth}
					class="mt-0.5 h-4 w-4 rounded border-zinc-600 bg-zinc-800 text-blue-500 focus:ring-blue-500"
				/>
				<div>
					<span class="text-sm font-medium text-[var(--text-primary)]"
						>Allow unauthenticated access</span
					>
					<p class="mt-0.5 text-xs text-[var(--text-secondary)]">
						When enabled, resources with <strong>public</strong> visibility are accessible without
						authentication (metadata only: names, status, run outcomes).
					</p>
				</div>
			</label>

			<div class="mt-6 flex justify-end">
				<Button variant="primary" onclick={save} {loading} disabled={saving}>Save</Button>
			</div>
		</div>
	{/if}
</div>
