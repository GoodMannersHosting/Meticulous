<script lang="ts">
	import { AppWindow, ChevronRight, Plus, Loader2 } from 'lucide-svelte';
	import {
		apiMethods,
		type CreateMeticulousAppResponse,
		type MeticulousAppSummary
	} from '$lib/api';

	let apps = $state<MeticulousAppSummary[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let showCreate = $state(false);
	let creating = $state(false);
	let newName = $state('');
	let newDescription = $state('');
	let createSecret = $state<CreateMeticulousAppResponse | null>(null);

	async function loadApps() {
		loading = true;
		error = null;
		try {
			apps = await apiMethods.admin.meticulousApps.list();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load apps';
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		void loadApps();
	});

	async function createApp() {
		const name = newName.trim();
		if (!name) return;
		creating = true;
		error = null;
		try {
			const d = newDescription.trim();
			createSecret = await apiMethods.admin.meticulousApps.create({
				name,
				description: d.length ? d : undefined
			});
			showCreate = false;
			newName = '';
			newDescription = '';
			await loadApps();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Create failed';
		} finally {
			creating = false;
		}
	}
</script>

<div class="space-y-6">
	<div class="flex flex-wrap items-center justify-between gap-3">
		<div>
			<h2 class="text-lg font-semibold text-[var(--text-primary)]">Meticulous Apps</h2>
			<p class="text-sm text-[var(--text-secondary)]">
				Integrations that authenticate with short-lived JWTs (for example, the Kubernetes operator).
			</p>
		</div>
		<button
			type="button"
			class="inline-flex items-center gap-2 rounded-lg bg-primary-600 px-4 py-2 text-sm font-medium text-white hover:bg-primary-700"
			onclick={() => {
				showCreate = true;
				createSecret = null;
			}}
		>
			<Plus class="h-4 w-4" />
			New app
		</button>
	</div>

	{#if error}
		<p class="rounded-lg border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-800 dark:border-red-900 dark:bg-red-950/40 dark:text-red-200">
			{error}
		</p>
	{/if}

	{#if createSecret}
		<div class="rounded-lg border border-amber-200 bg-amber-50 p-4 dark:border-amber-900/60 dark:bg-amber-950/30">
			<p class="text-sm font-medium text-amber-900 dark:text-amber-200">Save this private key now</p>
			<p class="mt-1 text-xs text-amber-800 dark:text-amber-300/90">
				It will not be shown again. Key id:
				<code class="rounded bg-black/5 px-1 dark:bg-white/10">{createSecret.key_id}</code>
			</p>
			<pre
				class="mt-3 max-h-48 overflow-auto rounded-md bg-[var(--bg-primary)] p-3 text-xs text-[var(--text-primary)]">{createSecret.private_key_pem}</pre>
			<button
				type="button"
				class="mt-3 text-sm text-primary-600 hover:underline dark:text-primary-400"
				onclick={() => {
					createSecret = null;
				}}
			>
				Dismiss
			</button>
		</div>
	{/if}

	{#if loading}
		<div class="flex justify-center py-12">
			<Loader2 class="h-8 w-8 animate-spin text-primary-500" />
		</div>
	{:else if apps.length === 0}
		<p class="text-sm text-[var(--text-secondary)]">No apps yet. Create one to obtain an application id and signing key.</p>
	{:else}
		<ul class="divide-y divide-[var(--border-primary)] rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)]">
			{#each apps as app (app.application_id)}
				<li>
					<a
						href="/admin/apps/{encodeURIComponent(app.application_id)}"
						class="flex items-center gap-4 px-4 py-3 transition-colors hover:bg-[var(--bg-hover)]"
					>
						<div class="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-[var(--bg-primary)]">
							<AppWindow class="h-5 w-5 text-[var(--text-secondary)]" />
						</div>
						<div class="min-w-0 flex-1">
							<p class="font-medium text-[var(--text-primary)]">{app.name}</p>
							<p class="truncate text-xs text-[var(--text-tertiary)]">{app.application_id}</p>
						</div>
						<ChevronRight class="h-5 w-5 text-[var(--text-tertiary)]" />
					</a>
				</li>
			{/each}
		</ul>
	{/if}
</div>

{#if showCreate}
	<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
	<div
		class="fixed inset-0 z-40 flex items-center justify-center bg-black/40 p-4"
		role="dialog"
		aria-modal="true"
		aria-labelledby="create-app-title"
		onkeydown={(e) => e.key === 'Escape' && (showCreate = false)}
	>
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<div
			class="w-full max-w-md rounded-xl border border-[var(--border-primary)] bg-[var(--bg-secondary)] p-6 shadow-lg"
			onclick={(e) => e.stopPropagation()}
		>
			<h3 id="create-app-title" class="text-lg font-semibold text-[var(--text-primary)]">Create Meticulous App</h3>
			<div class="mt-4 space-y-3">
				<label class="block text-sm">
					<span class="text-[var(--text-secondary)]">Name</span>
					<input
						class="mt-1 w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-primary)] px-3 py-2 text-sm"
						bind:value={newName}
						placeholder="e.g. Production cluster operator"
					/>
				</label>
				<label class="block text-sm">
					<span class="text-[var(--text-secondary)]">Description (optional)</span>
					<textarea
						class="mt-1 w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-primary)] px-3 py-2 text-sm"
						rows="2"
						bind:value={newDescription}
					></textarea>
				</label>
			</div>
			<div class="mt-6 flex justify-end gap-2">
				<button
					type="button"
					class="rounded-lg px-4 py-2 text-sm text-[var(--text-secondary)] hover:bg-[var(--bg-hover)]"
					onclick={() => (showCreate = false)}
				>
					Cancel
				</button>
				<button
					type="button"
					class="inline-flex items-center gap-2 rounded-lg bg-primary-600 px-4 py-2 text-sm font-medium text-white disabled:opacity-50"
					disabled={creating || !newName.trim()}
					onclick={() => void createApp()}
				>
					{#if creating}
						<Loader2 class="h-4 w-4 animate-spin" />
					{/if}
					Create
				</button>
			</div>
		</div>
	</div>
{/if}
