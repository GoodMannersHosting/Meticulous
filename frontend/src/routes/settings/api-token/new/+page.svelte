<script lang="ts">
	import { Button, Card, Input, Badge, Alert, CopyButton } from '$components/ui';
	import { api, apiMethods } from '$lib/api';
	import type { Pipeline, Project } from '$api/types';
	import { ArrowLeft } from 'lucide-svelte';

	type PipelinePickRow = Pipeline & { project_name: string };

	interface ApiToken {
		id: string;
		name: string;
		description?: string;
		prefix: string;
		scopes: string[];
		project_ids?: string[];
		pipeline_ids?: string[];
		created_at: string;
		last_used_at?: string;
		expires_at?: string;
		deactivated_at?: string;
		revoked_at?: string;
	}

	let newToken = $state({
		name: '',
		description: '',
		scopes: ['read'] as string[],
		expiresIn: '90',
		scopeProjects: false,
		selectedProjectIds: [] as string[],
		selectedPipelineIds: [] as string[]
	});

	let scopeProjectsCatalog = $state<Project[]>([]);
	let scopeProjectsLoading = $state(false);
	let pipelineCatalogRows = $state<PipelinePickRow[]>([]);
	let pipelinesLoading = $state(false);
	let pipelineSearch = $state('');

	let error = $state<string | null>(null);
	let creating = $state(false);
	let createdPlain = $state<string | null>(null);

	const scopeOptions = [
		{ value: 'read', label: 'Read', description: 'Read access to resources' },
		{
			value: 'write',
			label: 'Operator',
			description: 'Operator access to resources (run pipelines, manage runs)'
		},
		{ value: 'admin', label: 'Admin', description: 'Full administrative access' }
	];

	const expirationOptions = [
		{ value: '30', label: '30 days' },
		{ value: '90', label: '90 days' },
		{ value: '365', label: '1 year' },
		{ value: 'never', label: 'Never' }
	];

	const backHref = '/settings?tab=security';

	$effect(() => {
		scopeProjectsLoading = true;
		pipelinesLoading = true;
		pipelineCatalogRows = [];
		void (async () => {
			try {
				const res = await apiMethods.projects.list({ per_page: 200 });
				scopeProjectsCatalog = res.data ?? [];
				const rows: PipelinePickRow[] = [];
				await Promise.all(
					(scopeProjectsCatalog ?? []).map(async (proj) => {
						try {
							const pr = await apiMethods.pipelines.list({
								project_id: proj.id,
								per_page: 200
							});
							for (const pl of pr.data ?? []) {
								rows.push({ ...pl, project_name: proj.name });
							}
						} catch {
							/* omit */
						}
					})
				);
				pipelineCatalogRows = rows.sort((a, b) =>
					a.name.localeCompare(b.name, undefined, { sensitivity: 'base' })
				);
			} catch {
				scopeProjectsCatalog = [];
				pipelineCatalogRows = [];
			} finally {
				scopeProjectsLoading = false;
				pipelinesLoading = false;
			}
		})();
	});

	const scopedPipelineRows = $derived.by(() => {
		if (newToken.scopeProjects && newToken.selectedProjectIds.length > 0) {
			const allowed = new Set(newToken.selectedProjectIds);
			return pipelineCatalogRows.filter((r) => allowed.has(r.project_id));
		}
		return pipelineCatalogRows;
	});

	const pipelineSearchNorm = $derived(pipelineSearch.trim().toLowerCase());

	const filteredPipelineRows = $derived.by(() => {
		const q = pipelineSearchNorm;
		if (!q) return scopedPipelineRows;
		return scopedPipelineRows.filter(
			(r) =>
				r.name.toLowerCase().includes(q) ||
				r.slug.toLowerCase().includes(q) ||
				r.project_name.toLowerCase().includes(q)
		);
	});

	function toggleProjectForScope(projectId: string) {
		const set = new Set(newToken.selectedProjectIds);
		if (set.has(projectId)) set.delete(projectId);
		else set.add(projectId);
		newToken.selectedProjectIds = [...set];
		const allowed = new Set(newToken.selectedProjectIds);
		newToken.selectedPipelineIds = newToken.selectedPipelineIds.filter((id) => {
			const row = pipelineCatalogRows.find((p) => p.id === id);
			return row ? allowed.has(row.project_id) : false;
		});
	}

	function togglePipelineForScope(pipelineId: string) {
		const set = new Set(newToken.selectedPipelineIds);
		if (set.has(pipelineId)) set.delete(pipelineId);
		else set.add(pipelineId);
		newToken.selectedPipelineIds = [...set];
	}

	function toggleScope(scope: string) {
		if (newToken.scopes.includes(scope)) {
			newToken.scopes = newToken.scopes.filter((s) => s !== scope);
		} else {
			newToken.scopes = [...newToken.scopes, scope];
		}
	}

	async function createToken() {
		if (!newToken.name.trim()) return;
		if (newToken.scopeProjects && newToken.selectedProjectIds.length === 0) {
			error = 'Select at least one project, or turn off “Limit to specific projects”.';
			return;
		}

		creating = true;
		error = null;
		try {
			const expiresInDays = newToken.expiresIn === 'never' ? null : parseInt(newToken.expiresIn, 10);
			const description = newToken.description.trim() || undefined;

			const project_ids =
				newToken.scopeProjects && newToken.selectedProjectIds.length > 0
					? newToken.selectedProjectIds
					: undefined;
			const pipeline_ids =
				newToken.selectedPipelineIds.length > 0 ? newToken.selectedPipelineIds : undefined;

			const response = await api.post<{ token: ApiToken; plain_token: string }>('/api/v1/tokens', {
				name: newToken.name.trim(),
				description,
				scopes: newToken.scopes,
				expires_in_days: expiresInDays,
				...(project_ids ? { project_ids } : {}),
				...(pipeline_ids ? { pipeline_ids } : {})
			});

			createdPlain = response.plain_token;
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to create token';
			console.error('Failed to create token:', e);
		} finally {
			creating = false;
		}
	}
</script>

<svelte:head>
	<title>Create API token | Meticulous</title>
</svelte:head>

<div class="mx-auto max-w-5xl px-4 pb-10 pt-2 sm:px-6">
	<div class="mb-6 flex flex-wrap items-center gap-3">
		<Button variant="ghost" size="sm" href={backHref} class="shrink-0 gap-2">
			<ArrowLeft class="h-4 w-4" />
			Back
		</Button>
		<div class="min-w-0">
			<h1 class="text-xl font-bold text-[var(--text-primary)] sm:text-2xl">Create API token</h1>
			<p class="mt-1 text-sm text-[var(--text-secondary)]">
				Configure scopes, optional project and pipeline limits, and expiration.
			</p>
		</div>
	</div>

	{#if createdPlain}
		<Card class="max-w-2xl">
			<h2 class="text-lg font-medium text-[var(--text-primary)]">Token created</h2>
			<p class="mt-1 text-sm text-[var(--text-secondary)]">
				Copy the secret now — you will not see it again.
			</p>
			<div class="mt-4 space-y-4">
				<Alert variant="warning" title="Save this token securely">
					This is the only time you can copy the full secret. Store it in a password manager or secret store.
				</Alert>
				<div class="flex flex-col gap-2 rounded-lg bg-[var(--bg-tertiary)] p-3 sm:flex-row sm:items-center">
					<code class="min-w-0 flex-1 break-all font-mono text-sm">{createdPlain}</code>
					<CopyButton text={createdPlain} class="shrink-0 self-end sm:self-center" />
				</div>
				<div class="flex flex-wrap gap-3">
					<Button variant="primary" href={backHref}>Done</Button>
				</div>
			</div>
		</Card>
	{:else}
		{#if error}
			<Alert variant="error" title="Error" class="mb-6">
				{error}
			</Alert>
		{/if}

		<form
			onsubmit={(e) => {
				e.preventDefault();
				createToken();
			}}
			class="grid gap-6 lg:grid-cols-2 lg:gap-8"
		>
			<div class="space-y-6">
				<Card>
					<h2 class="text-base font-medium text-[var(--text-primary)]">Details</h2>
					<p class="mt-1 text-sm text-[var(--text-secondary)]">Name and optional description.</p>
					<div class="mt-4 space-y-4">
						<div>
							<label for="token-name" class="block text-sm font-medium text-[var(--text-primary)]">
								Token name
							</label>
							<Input
								id="token-name"
								placeholder="e.g., CI/CD Token"
								bind:value={newToken.name}
								class="mt-1"
								required
								autocomplete="off"
							/>
						</div>
						<div>
							<label for="token-description" class="block text-sm font-medium text-[var(--text-primary)]">
								Description
								<span class="font-normal text-[var(--text-tertiary)]">(optional)</span>
							</label>
							<Input
								id="token-description"
								placeholder="e.g., Used by GitHub Actions to deploy staging"
								bind:value={newToken.description}
								class="mt-1"
							/>
						</div>
					</div>
				</Card>

				<Card>
					<h2 class="text-base font-medium text-[var(--text-primary)]">Scopes</h2>
					<p class="mt-1 text-sm text-[var(--text-secondary)]">What this token is allowed to do.</p>
					<div class="mt-4 space-y-2">
						{#each scopeOptions as option (option.value)}
							<label
								class="flex items-start gap-3 rounded-lg border border-[var(--border-primary)] p-3 sm:items-center"
							>
								<input
									type="checkbox"
									checked={newToken.scopes.includes(option.value)}
									onchange={() => toggleScope(option.value)}
									class="mt-1 h-4 w-4 shrink-0 rounded border-secondary-300 sm:mt-0"
								/>
								<div class="min-w-0">
									<p class="font-medium text-[var(--text-primary)]">{option.label}</p>
									<p class="text-sm text-[var(--text-secondary)]">{option.description}</p>
								</div>
							</label>
						{/each}
					</div>
				</Card>

				<Card>
					<h2 class="text-base font-medium text-[var(--text-primary)]">Expiration</h2>
					<p class="mt-1 text-sm text-[var(--text-secondary)]">When the token stops working unless rotated.</p>
					<label for="expiration" class="mt-4 block text-sm font-medium text-[var(--text-primary)]">
						Expires after
					</label>
					<select
						id="expiration"
						bind:value={newToken.expiresIn}
						class="
							mt-1 w-full max-w-md rounded-lg border border-[var(--border-primary)]
							bg-[var(--bg-secondary)] px-3 py-2 text-sm
							focus:outline-none focus:ring-2 focus:ring-primary-500
						"
					>
						{#each expirationOptions as option (option.value)}
							<option value={option.value}>{option.label}</option>
						{/each}
					</select>
				</Card>
			</div>

			<div class="space-y-6 lg:min-h-0">
				<Card class="flex min-h-0 flex-col lg:max-h-[min(70vh,42rem)]">
					<h2 class="text-base font-medium text-[var(--text-primary)]">Access limits</h2>
					<p class="mt-1 text-sm text-[var(--text-secondary)]">
						Optional: restrict which projects and pipelines this token can use.
					</p>

					<div class="mt-4 flex min-h-0 flex-1 flex-col gap-4">
						<div class="rounded-lg border border-[var(--border-primary)] p-3">
							<label class="flex items-start gap-3">
								<input
									type="checkbox"
									class="mt-1 h-4 w-4 shrink-0 rounded border-secondary-300"
									bind:checked={newToken.scopeProjects}
								/>
								<span>
									<span class="block text-sm font-medium text-[var(--text-primary)]">
										Limit to specific projects
									</span>
									<span class="block text-sm text-[var(--text-secondary)]">
										Leave off for all projects you can access. Turn on to pick one or more.
									</span>
								</span>
							</label>
							{#if newToken.scopeProjects}
								<div class="mt-3 max-h-36 space-y-2 overflow-y-auto rounded-md border border-[var(--border-secondary)] p-2">
									{#if scopeProjectsLoading}
										<p class="text-sm text-[var(--text-tertiary)]">Loading projects…</p>
									{:else if scopeProjectsCatalog.length === 0}
										<p class="text-sm text-[var(--text-tertiary)]">No projects available.</p>
									{:else}
										{#each scopeProjectsCatalog as p (p.id)}
											<label class="flex items-center gap-2 text-sm">
												<input
													type="checkbox"
													class="h-4 w-4 rounded border-secondary-300"
													checked={newToken.selectedProjectIds.includes(p.id)}
													onchange={() => toggleProjectForScope(p.id)}
												/>
												<span class="text-[var(--text-primary)]">{p.name}</span>
											</label>
										{/each}
									{/if}
								</div>
							{/if}
						</div>

						<div class="flex min-h-0 flex-1 flex-col">
							<span class="text-sm font-medium text-[var(--text-primary)]">
								Pipelines
								<span class="font-normal text-[var(--text-tertiary)]">(optional)</span>
							</span>
							<p class="mt-1 text-xs text-[var(--text-secondary)]">
								Search by pipeline or project name. Leave empty to allow all pipelines allowed by the
								project rules above.
							</p>
							{#if pipelinesLoading}
								<p class="mt-2 text-sm text-[var(--text-tertiary)]">Loading pipelines…</p>
							{:else if scopedPipelineRows.length === 0}
								<p class="mt-2 text-sm text-[var(--text-tertiary)]">No pipelines available.</p>
							{:else}
								<Input
									type="search"
									bind:value={pipelineSearch}
									placeholder="Search pipelines…"
									class="mt-2"
									autocomplete="off"
								/>
								<div
									class="mt-2 min-h-[8rem] flex-1 space-y-1 overflow-y-auto rounded-md border border-[var(--border-secondary)] p-2 lg:min-h-[12rem]"
								>
									{#if filteredPipelineRows.length === 0}
										<p class="px-1 py-2 text-sm text-[var(--text-tertiary)]">No matching pipelines.</p>
									{:else}
										{#each filteredPipelineRows as pl (pl.id)}
											<label
												class="flex cursor-pointer items-start gap-2 rounded px-1 py-1.5 text-sm hover:bg-[var(--bg-hover)]"
											>
												<input
													type="checkbox"
													class="mt-0.5 h-4 w-4 shrink-0 rounded border-secondary-300"
													checked={newToken.selectedPipelineIds.includes(pl.id)}
													onchange={() => togglePipelineForScope(pl.id)}
												/>
												<span class="min-w-0 flex-1">
													<span class="font-medium text-[var(--text-primary)]">{pl.name}</span>
													<span class="block text-xs text-[var(--text-tertiary)]">{pl.project_name}</span>
												</span>
											</label>
										{/each}
									{/if}
								</div>
								{#if newToken.selectedPipelineIds.length > 0}
									<p class="mt-2 text-xs text-[var(--text-tertiary)]">
										{newToken.selectedPipelineIds.length} pipeline{newToken.selectedPipelineIds.length === 1
											? ''
											: 's'} selected
									</p>
								{/if}
							{/if}
						</div>
					</div>
				</Card>
			</div>

			<div class="flex flex-col gap-3 border-t border-[var(--border-primary)] pt-6 lg:col-span-2 sm:flex-row sm:items-center sm:justify-between">
				<Button variant="outline" href={backHref} type="button" class="order-2 w-full sm:order-1 sm:w-auto">
					Cancel
				</Button>
				<Button
					variant="primary"
					type="submit"
					disabled={!newToken.name || newToken.scopes.length === 0 || creating}
					class="order-1 w-full sm:order-2 sm:min-w-[10rem]"
				>
					{creating ? 'Creating…' : 'Create token'}
				</Button>
			</div>
		</form>
	{/if}
</div>
