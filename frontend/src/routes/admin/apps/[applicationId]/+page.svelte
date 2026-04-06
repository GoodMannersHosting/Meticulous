<script lang="ts">
	import { page } from '$app/stores';
	import {
		Loader2,
		KeyRound,
		Trash2,
		Plus,
		Copy
	} from 'lucide-svelte';
	import {
		apiMethods,
		type MeticulousAppInstallationRow,
		type MeticulousAppSummary
	} from '$lib/api';
	import type { Project } from '$lib/api/types';

	const applicationId = $derived($page.params.applicationId);

	let app = $state<MeticulousAppSummary | null>(null);
	let installations = $state<MeticulousAppInstallationRow[]>([]);
	let projects = $state<Project[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let newKeyPem = $state<string | null>(null);
	let newKeyId = $state<string | null>(null);
	let rotating = $state(false);
	let installProjectId = $state('');
	let permCreate = $state(true);
	let permRevoke = $state(false);
	let installing = $state(false);
	let actionError = $state<string | null>(null);

	async function reload() {
		if (!applicationId) return;
		loading = true;
		error = null;
		try {
			[app, installations] = await Promise.all([
				apiMethods.admin.meticulousApps.get(applicationId),
				apiMethods.admin.meticulousApps.listInstallations(applicationId)
			]);
			const projRes = await apiMethods.projects.list({ limit: 500 });
			projects = projRes.data ?? [];
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load';
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		void reload();
	});

	async function addKey() {
		if (!applicationId) return;
		rotating = true;
		actionError = null;
		try {
			const r = await apiMethods.admin.meticulousApps.addKey(applicationId);
			newKeyId = r.key_id;
			newKeyPem = r.private_key_pem;
		} catch (e) {
			actionError = e instanceof Error ? e.message : 'Key rotation failed';
		} finally {
			rotating = false;
		}
	}

	function permissionsSelection(): string[] {
		const p: string[] = [];
		if (permCreate) p.push('join_tokens:create');
		if (permRevoke) p.push('join_tokens:revoke');
		return p.length ? p : ['join_tokens:create'];
	}

	async function addInstallation() {
		if (!applicationId || !installProjectId) return;
		installing = true;
		actionError = null;
		try {
			await apiMethods.admin.meticulousApps.createInstallation(applicationId, {
				project_id: installProjectId,
				permissions: permissionsSelection()
			});
			installProjectId = '';
			await reload();
		} catch (e) {
			actionError = e instanceof Error ? e.message : 'Failed to create installation';
		} finally {
			installing = false;
		}
	}

	async function revokeInstallation(inst: MeticulousAppInstallationRow) {
		if (!applicationId || inst.revoked_at) return;
		if (!confirm(`Revoke installation ${inst.id}?`)) return;
		actionError = null;
		try {
			await apiMethods.admin.meticulousApps.revokeInstallation(applicationId, inst.id);
			await reload();
		} catch (e) {
			actionError = e instanceof Error ? e.message : 'Revoke failed';
		}
	}

	function copy(text: string) {
		void navigator.clipboard.writeText(text);
	}

	/** Prefer display name over raw `project_id` (UUID); fallback keeps support visible if the project list is incomplete. */
	function installationProjectLabel(projectId: string): string {
		const p = projects.find((x) => x.id === projectId);
		if (p) return p.name;
		return projectId;
	}
</script>

<div class="space-y-8">
	{#if loading}
		<div class="flex justify-center py-12">
			<Loader2 class="h-8 w-8 animate-spin text-primary-500" />
		</div>
	{:else if error || !app}
		<p class="text-sm text-red-600 dark:text-red-400">{error ?? 'Not found'}</p>
	{:else}
		<div>
			<h2 class="text-lg font-semibold text-[var(--text-primary)]">{app.name}</h2>
			{#if app.description}
				<p class="mt-1 text-sm text-[var(--text-secondary)]">{app.description}</p>
			{/if}
			<div class="mt-3 flex flex-wrap items-center gap-2 text-sm">
				<span class="text-[var(--text-secondary)]">Application id</span>
				<code class="rounded-md bg-[var(--bg-primary)] px-2 py-0.5 text-xs">{app.application_id}</code>
				<button
					type="button"
					class="inline-flex items-center gap-1 rounded text-xs text-primary-600 hover:underline dark:text-primary-400"
					onclick={() => copy(app!.application_id)}
					title="Copy"
				>
					<Copy class="h-3 w-3" />
				</button>
			</div>
		</div>

		{#if actionError}
			<p class="text-sm text-red-600 dark:text-red-400">{actionError}</p>
		{/if}

		{#if newKeyPem}
			<div class="rounded-lg border border-amber-200 bg-amber-50 p-4 dark:border-amber-900/60 dark:bg-amber-950/30">
				<p class="text-sm font-medium text-amber-900 dark:text-amber-200">New private key</p>
				<p class="mt-1 text-xs text-amber-800 dark:text-amber-300/90">
					Store securely. Key id:
					<code class="rounded bg-black/5 px-1">{newKeyId}</code>
				</p>
				<pre
					class="mt-3 max-h-40 overflow-auto rounded-md bg-[var(--bg-primary)] p-3 text-xs">{newKeyPem}</pre>
				<button
					type="button"
					class="mt-2 text-sm text-primary-600 hover:underline dark:text-primary-400"
					onclick={() => {
						newKeyPem = null;
						newKeyId = null;
					}}
				>
					Dismiss
				</button>
			</div>
		{/if}

		<section class="space-y-3">
			<div class="flex items-center justify-between gap-2">
				<h3 class="font-medium text-[var(--text-primary)]">Signing keys</h3>
				<button
					type="button"
					class="inline-flex items-center gap-2 rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-1.5 text-sm hover:bg-[var(--bg-hover)] disabled:opacity-50"
					disabled={rotating}
					onclick={() => void addKey()}
				>
					<KeyRound class="h-4 w-4" />
					{rotating ? 'Generating…' : 'Generate new key'}
				</button>
			</div>
			<p class="text-xs text-[var(--text-secondary)]">
				Add a key before revoking an old one if you need a rotation window. Revoke unused keys from the API if needed.
			</p>
		</section>

		<section class="space-y-4">
			<h3 class="font-medium text-[var(--text-primary)]">Installations</h3>
			<p class="text-sm text-[var(--text-secondary)]">
				Each installation binds this app to one project and defines what the integration may do (for example, create join tokens for that project).
				For the Kubernetes operator, also enable <code class="text-xs">join_tokens:revoke</code> so agents can revoke tokens on delete.
			</p>

			<div class="flex flex-wrap items-end gap-3 rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] p-4">
				<label class="text-sm">
					<span class="text-[var(--text-secondary)]">Project</span>
					<select
						class="mt-1 block min-w-[14rem] rounded-lg border border-[var(--border-primary)] bg-[var(--bg-primary)] px-3 py-2 text-sm"
						bind:value={installProjectId}
					>
						<option value="">Select project…</option>
						{#each projects as p (p.id)}
							<option value={p.id}>{p.name} ({p.slug})</option>
						{/each}
					</select>
				</label>
				<fieldset class="text-sm">
					<legend class="text-[var(--text-secondary)]">Permissions</legend>
					<label class="mt-1 flex items-center gap-2">
						<input type="checkbox" bind:checked={permCreate} />
						<span>join_tokens:create</span>
					</label>
					<label class="flex items-center gap-2">
						<input type="checkbox" bind:checked={permRevoke} />
						<span>join_tokens:revoke</span>
					</label>
				</fieldset>
				<button
					type="button"
					class="inline-flex items-center gap-2 rounded-lg bg-primary-600 px-3 py-2 text-sm font-medium text-white disabled:opacity-50"
					disabled={installing || !installProjectId}
					onclick={() => void addInstallation()}
				>
					<Plus class="h-4 w-4" />
					Install
				</button>
			</div>

			{#if installations.length === 0}
				<p class="text-sm text-[var(--text-tertiary)]">No installations yet.</p>
			{:else}
				<ul class="divide-y divide-[var(--border-primary)] rounded-lg border border-[var(--border-primary)]">
					{#each installations as inst (inst.id)}
						<li class="flex flex-wrap items-center justify-between gap-2 px-4 py-3">
							<div class="min-w-0">
								<p class="text-sm font-medium text-[var(--text-primary)]">
									Installation <code class="text-xs">{inst.id}</code>
								</p>
								<p class="text-xs text-[var(--text-tertiary)]">
									<span class="text-[var(--text-secondary)]">{installationProjectLabel(inst.project_id)}</span>
									· {inst.permissions.join(', ') || '—'}
								</p>
								{#if inst.revoked_at}
									<p class="mt-1 text-xs text-amber-700 dark:text-amber-400">Revoked {inst.revoked_at}</p>
								{/if}
							</div>
							{#if !inst.revoked_at}
								<button
									type="button"
									class="inline-flex items-center gap-1 rounded text-sm text-red-600 hover:underline dark:text-red-400"
									onclick={() => void revokeInstallation(inst)}
								>
									<Trash2 class="h-4 w-4" />
									Revoke
								</button>
							{/if}
						</li>
					{/each}
				</ul>
			{/if}
		</section>
	{/if}
</div>
