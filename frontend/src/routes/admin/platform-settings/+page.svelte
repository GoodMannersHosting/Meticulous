<script lang="ts">
	import { onMount } from 'svelte';
	import { Button, Alert } from '$components/ui';
	import { apiMethods } from '$api/client';

	const SYNC_INTERVAL_OPTIONS = [
		{ value: 0, label: 'Disabled (no global default)' },
		{ value: 15, label: 'Every 15 minutes' },
		{ value: 30, label: 'Every 30 minutes' },
		{ value: 60, label: 'Every hour' },
		{ value: 360, label: 'Every 6 hours' },
		{ value: 720, label: 'Every 12 hours' },
		{ value: 1440, label: 'Daily' }
	];

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

	// Global workflow sync default
	let globalSyncInterval = $state(0);
	let syncSettingsLoading = $state(false);
	let syncSettingsSaving = $state(false);
	let syncSettingsError = $state<string | null>(null);
	let syncSettingsSuccess = $state<string | null>(null);

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
		// Load global sync settings
		syncSettingsLoading = true;
		try {
			const res = await apiMethods.wfCatalog.getSyncSettings();
			globalSyncInterval = res.default_sync_interval_minutes ?? 0;
		} catch {
			// not critical
		} finally {
			syncSettingsLoading = false;
		}
	});

	async function saveGlobalSyncSettings() {
		syncSettingsSaving = true;
		syncSettingsError = null;
		syncSettingsSuccess = null;
		try {
			const res = await apiMethods.wfCatalog.putSyncSettings({
				default_sync_interval_minutes: globalSyncInterval === 0 ? null : globalSyncInterval
			});
			globalSyncInterval = res.default_sync_interval_minutes ?? 0;
			syncSettingsSuccess = 'Global sync interval saved.';
		} catch (e) {
			syncSettingsError = e instanceof Error ? e.message : 'Failed to save';
		} finally {
			syncSettingsSaving = false;
		}
	}

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
		<!-- Global workflow sync interval -->
		<div class="max-w-xl rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] p-6">
			<h3 class="mb-1 text-sm font-medium text-[var(--text-primary)]">Workflow auto-sync</h3>
			<p class="mb-4 text-xs text-[var(--text-secondary)]">
				Default interval for automatically re-importing workflow catalog entries from source. Individual
				workflows can override this on their detail page.
			</p>

			{#if syncSettingsError}
				<Alert variant="error">{syncSettingsError}</Alert>
			{/if}
			{#if syncSettingsSuccess}
				<Alert variant="success">{syncSettingsSuccess}</Alert>
			{/if}

			<div class="flex items-center gap-3">
				{#if syncSettingsLoading}
					<p class="text-sm text-[var(--text-secondary)]">Loading…</p>
				{:else}
					<select
						bind:value={globalSyncInterval}
						class="block rounded-lg border border-[var(--border-primary)] bg-[var(--bg-tertiary)] px-3 py-2 text-sm text-[var(--text-primary)] focus:outline-none focus:ring-2 focus:ring-primary-500"
					>
						{#each SYNC_INTERVAL_OPTIONS as opt (opt.value)}
							<option value={opt.value}>{opt.label}</option>
						{/each}
					</select>
					<Button variant="outline" size="sm" onclick={saveGlobalSyncSettings} loading={syncSettingsSaving}>
						Save
					</Button>
				{/if}
			</div>
		</div>

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
