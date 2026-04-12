<script lang="ts">
	import { onMount } from 'svelte';
	import { Button, Alert } from '$components/ui';
	import { apiMethods } from '$api/client';

	const EXTERNAL_KIND_TOGGLES: { key: string; label: string; description: string }[] = [
		{
			key: 'aws_sm',
			label: 'AWS Secrets Manager',
			description: 'Allow creating and rotating secrets that reference AWS Secrets Manager.'
		},
		{
			key: 'vault',
			label: 'HashiCorp Vault',
			description: 'Allow Vault path references for stored secrets.'
		},
		{
			key: 'gcp_sm',
			label: 'GCP Secret Manager',
			description: 'Allow GCP Secret Manager resource names as references.'
		},
		{
			key: 'azure_kv',
			label: 'Azure Key Vault',
			description: 'Allow Azure Key Vault secret references.'
		},
		{
			key: 'kubernetes',
			label: 'Kubernetes',
			description: 'Allow Kubernetes secret references.'
		}
	];

	let loading = $state(true);
	let saving = $state(false);
	let error = $state<string | null>(null);
	let success = $state<string | null>(null);
	let allowUnauth = $state(false);
	/** Merged policy (defaults true when unset). */
	let extKindEnabled = $state<Record<string, boolean>>({});

	onMount(async () => {
		try {
			const settings = await apiMethods.platformSettings.get();
			allowUnauth = settings.allow_unauthenticated_access;
			const base: Record<string, boolean> = {};
			for (const { key } of EXTERNAL_KIND_TOGGLES) base[key] = true;
			extKindEnabled = { ...base, ...(settings.stored_secret_external_kinds ?? {}) };
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
			const stored_secret_external_kinds: Record<string, boolean> = {};
			for (const { key } of EXTERNAL_KIND_TOGGLES) {
				stored_secret_external_kinds[key] = extKindEnabled[key] !== false;
			}
			const updated = await apiMethods.platformSettings.update({
				allow_unauthenticated_access: allowUnauth,
				stored_secret_external_kinds
			});
			allowUnauth = updated.allow_unauthenticated_access;
			const base: Record<string, boolean> = {};
			for (const { key } of EXTERNAL_KIND_TOGGLES) base[key] = true;
			extKindEnabled = { ...base, ...(updated.stored_secret_external_kinds ?? {}) };
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

			<div class="mt-8 border-t border-[var(--border-secondary)] pt-8">
				<h3 class="mb-2 text-sm font-medium text-[var(--text-primary)]">External stored secret providers</h3>
				<p class="mb-4 text-xs text-[var(--text-secondary)]">
					When disabled, users cannot create or rotate secrets of that kind (existing secrets are unchanged).
					Inline and encrypted kinds (for example key/value) are always available.
				</p>
				<div class="space-y-4">
					{#each EXTERNAL_KIND_TOGGLES as row (row.key)}
						<label class="flex items-start gap-3">
							<input
								type="checkbox"
								checked={extKindEnabled[row.key] !== false}
								onchange={(e) => {
									const el = e.currentTarget;
									extKindEnabled = { ...extKindEnabled, [row.key]: el.checked };
								}}
								class="mt-0.5 h-4 w-4 rounded border-zinc-600 bg-zinc-800 text-blue-500 focus:ring-blue-500"
							/>
							<div>
								<span class="text-sm font-medium text-[var(--text-primary)]">{row.label}</span>
								<p class="mt-0.5 text-xs text-[var(--text-secondary)]">{row.description}</p>
							</div>
						</label>
					{/each}
				</div>
			</div>

			<div class="mt-6 flex justify-end">
				<Button variant="primary" onclick={save} {loading} disabled={saving}>Save</Button>
			</div>
		</div>
	{/if}
</div>
