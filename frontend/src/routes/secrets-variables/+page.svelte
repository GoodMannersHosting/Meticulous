<script lang="ts">
	import { Button, Card, Input, Select, Tabs, Alert, Badge, Dialog } from '$components/ui';
	import { EmptyState, Skeleton } from '$components/data';
	import { apiMethods } from '$api/client';
	import type {
		Pipeline,
		Project,
		WorkspaceScopeLevel,
		WorkspaceStoredSecretListItem,
		WorkspaceVariableListItem
	} from '$api/types';
	import { formatRelativeTime } from '$utils/format';
	import { Braces, KeyRound, Search, ExternalLink, RefreshCw, Plus } from 'lucide-svelte';

	let kind = $state<'variables' | 'secrets'>('variables');
	let scopeLevel = $state<WorkspaceScopeLevel>('all');
	let projectId = $state('');
	let pipelineId = $state('');
	let searchInput = $state('');
	/** Value passed to the API (updated via Search or initial load). */
	let appliedSearch = $state('');

	let projects = $state<Project[]>([]);
	let projectsLoading = $state(true);
	let pipelines = $state<Pipeline[]>([]);
	let pipelinesLoading = $state(false);

	let variableRows = $state<WorkspaceVariableListItem[]>([]);
	let secretRows = $state<WorkspaceStoredSecretListItem[]>([]);
	let listLoading = $state(false);
	let listLoadingMore = $state(false);
	let listError = $state<string | null>(null);
	let nextCursor = $state<string | null>(null);

	let showHubCreateVariable = $state(false);
	let hubVarProjectId = $state('');
	let hubVarPipelineId = $state('');
	let hubVarName = $state('');
	let hubVarValue = $state('');
	let hubVarSensitive = $state(false);
	let hubVarPipelines = $state<Pipeline[]>([]);
	let hubVarPipelinesLoading = $state(false);
	let hubVariableActionLoading = $state(false);
	let hubVarError = $state<string | null>(null);

	let showHubCreateSecret = $state(false);
	let hubSecProjectId = $state('');
	let hubSecPipelineId = $state('');
	let hubSecPath = $state('');
	let hubSecKind = $state('kv');
	let hubSecValue = $state('');
	let hubSecDescription = $state('');
	let hubSecOrgWide = $state(false);
	let hubSecPropagateToProjects = $state(true);
	let hubSecGhAppId = $state('');
	let hubSecGhInstallId = $state('');
	let hubSecGhPrivateKey = $state('');
	let hubSecGhApiBase = $state('');
	let hubSecGhExtraJson = $state('');
	let hubSecPipelines = $state<Pipeline[]>([]);
	let hubSecPipelinesLoading = $state(false);
	let hubSecretActionLoading = $state(false);
	let hubSecretError = $state<string | null>(null);

	const kindTabs = [
		{ id: 'variables' as const, label: 'Variables', icon: Braces },
		{ id: 'secrets' as const, label: 'Stored secrets', icon: KeyRound }
	];

	const scopeOptions: { value: WorkspaceScopeLevel; label: string }[] = [
		{ value: 'all', label: 'All scopes' },
		{ value: 'organization', label: 'Organization (secrets only)' },
		{ value: 'project', label: 'Project-wide' },
		{ value: 'pipeline', label: 'Pipeline-scoped' }
	];

	const projectOptions = $derived([
		{ value: '', label: 'All projects' },
		...projects.map((p) => ({ value: p.id, label: p.name }))
	]);

	const pipelineOptions = $derived([
		{ value: '', label: projectId ? 'All pipelines in project' : 'Select a project first' },
		...pipelines.map((p) => ({ value: p.id, label: p.name }))
	]);

	const projectPickOptions = $derived([
		{ value: '', label: 'Select project…' },
		...projects.map((p) => ({ value: p.id, label: p.name }))
	]);

	const hubVarPipelineOptions = $derived([
		{ value: '', label: 'Project-wide (all pipelines)' },
		...hubVarPipelines.map((p) => ({ value: p.id, label: p.name }))
	]);

	const hubSecPipelineOptions = $derived([
		{ value: '', label: 'Project-wide (all pipelines)' },
		...hubSecPipelines.map((p) => ({ value: p.id, label: p.name }))
	]);

	const kindOptions = [
		{ value: 'kv', label: 'Key / value (kv)' },
		{ value: 'api_key', label: 'API key' },
		{ value: 'ssh_private_key', label: 'SSH private key (PEM)' },
		{ value: 'github_app', label: 'GitHub App' },
		{ value: 'x509_bundle', label: 'X.509 bundle (JSON)' }
	];

	$effect(() => {
		void loadProjects();
	});

	async function loadProjects() {
		projectsLoading = true;
		try {
			const acc: Project[] = [];
			let cursor: string | undefined;
			do {
				const r = await apiMethods.projects.list({ per_page: 100, cursor });
				acc.push(...r.data);
				cursor = r.pagination.has_more ? r.pagination.next_cursor : undefined;
			} while (cursor);
			projects = acc;
		} catch {
			projects = [];
		} finally {
			projectsLoading = false;
		}
	}

	$effect(() => {
		const pid = projectId;
		if (!pid) {
			pipelines = [];
			pipelineId = '';
			return;
		}
		void loadPipelinesForProject(pid);
	});

	async function loadPipelinesForProject(pid: string) {
		pipelinesLoading = true;
		pipelineId = '';
		try {
			pipelines = await fetchAllPipelines(pid);
		} catch {
			pipelines = [];
		} finally {
			pipelinesLoading = false;
		}
	}

	async function fetchAllPipelines(project_id: string): Promise<Pipeline[]> {
		const acc: Pipeline[] = [];
		let cursor: string | undefined;
		do {
			const r = await apiMethods.pipelines.list({ project_id, per_page: 100, cursor });
			acc.push(...r.data);
			cursor = r.pagination.has_more ? r.pagination.next_cursor : undefined;
		} while (cursor);
		return acc;
	}

	$effect(() => {
		if (hubSecOrgWide) hubSecPipelineId = '';
	});

	$effect(() => {
		if (!showHubCreateVariable || !hubVarProjectId) {
			if (!showHubCreateVariable) {
				hubVarPipelines = [];
			}
			return;
		}
		const pid = hubVarProjectId;
		let cancelled = false;
		hubVarPipelinesLoading = true;
		void fetchAllPipelines(pid)
			.then((data) => {
				if (cancelled) return;
				hubVarPipelines = data;
				if (hubVarPipelineId && !data.some((p) => p.id === hubVarPipelineId)) {
					hubVarPipelineId = '';
				}
			})
			.catch(() => {
				if (!cancelled) hubVarPipelines = [];
			})
			.finally(() => {
				if (!cancelled) hubVarPipelinesLoading = false;
			});
		return () => {
			cancelled = true;
		};
	});

	$effect(() => {
		if (!showHubCreateSecret || !hubSecProjectId || hubSecOrgWide) {
			if (!showHubCreateSecret || hubSecOrgWide) {
				hubSecPipelines = [];
			}
			return;
		}
		const pid = hubSecProjectId;
		let cancelled = false;
		hubSecPipelinesLoading = true;
		void fetchAllPipelines(pid)
			.then((data) => {
				if (cancelled) return;
				hubSecPipelines = data;
				if (hubSecPipelineId && !data.some((p) => p.id === hubSecPipelineId)) {
					hubSecPipelineId = '';
				}
			})
			.catch(() => {
				if (!cancelled) hubSecPipelines = [];
			})
			.finally(() => {
				if (!cancelled) hubSecPipelinesLoading = false;
			});
		return () => {
			cancelled = true;
		};
	});

	function hubParams(cursor?: string) {
		return {
			...(appliedSearch.trim() ? { q: appliedSearch.trim() } : {}),
			...(projectId ? { project_id: projectId } : {}),
			...(pipelineId ? { pipeline_id: pipelineId } : {}),
			...(scopeLevel !== 'all' ? { scope_level: scopeLevel } : {}),
			per_page: 40,
			...(cursor ? { cursor } : {})
		};
	}

	async function reloadList() {
		listLoading = true;
		listError = null;
		nextCursor = null;
		try {
			if (kind === 'variables') {
				const r = await apiMethods.workspaceConfig.listVariables(hubParams());
				variableRows = r.data;
				nextCursor = r.pagination.next_cursor ?? null;
			} else {
				const r = await apiMethods.workspaceConfig.listStoredSecrets(hubParams());
				secretRows = r.data;
				nextCursor = r.pagination.next_cursor ?? null;
			}
		} catch (e) {
			listError = e instanceof Error ? e.message : 'Failed to load';
			variableRows = [];
			secretRows = [];
		} finally {
			listLoading = false;
		}
	}

	async function loadMore() {
		if (!nextCursor || listLoadingMore) return;
		listLoadingMore = true;
		listError = null;
		try {
			if (kind === 'variables') {
				const r = await apiMethods.workspaceConfig.listVariables(hubParams(nextCursor));
				variableRows = [...variableRows, ...r.data];
				nextCursor = r.pagination.next_cursor ?? null;
			} else {
				const r = await apiMethods.workspaceConfig.listStoredSecrets(hubParams(nextCursor));
				secretRows = [...secretRows, ...r.data];
				nextCursor = r.pagination.next_cursor ?? null;
			}
		} catch (e) {
			listError = e instanceof Error ? e.message : 'Failed to load more';
		} finally {
			listLoadingMore = false;
		}
	}

	$effect(() => {
		kind;
		scopeLevel;
		projectId;
		pipelineId;
		appliedSearch;
		void reloadList();
	});

	function applySearch() {
		appliedSearch = searchInput;
	}

	function openHubCreateVariable() {
		hubVarError = null;
		hubVarProjectId = projectId || '';
		hubVarPipelineId = projectId && pipelineId ? pipelineId : '';
		hubVarName = '';
		hubVarValue = '';
		hubVarSensitive = false;
		showHubCreateVariable = true;
	}

	async function submitHubCreateVariable() {
		if (!hubVarProjectId.trim()) {
			hubVarError = 'Select a project';
			return;
		}
		if (!hubVarName.trim()) {
			hubVarError = 'Name is required';
			return;
		}
		hubVariableActionLoading = true;
		hubVarError = null;
		try {
			await apiMethods.variables.create(hubVarProjectId, {
				name: hubVarName.trim(),
				value: hubVarValue,
				is_sensitive: hubVarSensitive,
				pipeline_id: hubVarPipelineId || undefined
			});
			showHubCreateVariable = false;
			await reloadList();
		} catch (e) {
			hubVarError = e instanceof Error ? e.message : 'Failed to create variable';
		} finally {
			hubVariableActionLoading = false;
		}
	}

	function openHubCreateSecret() {
		hubSecretError = null;
		hubSecProjectId = projectId || '';
		hubSecPipelineId = projectId && pipelineId ? pipelineId : '';
		hubSecPath = '';
		hubSecKind = 'kv';
		hubSecValue = '';
		hubSecDescription = '';
		hubSecOrgWide = false;
		hubSecPropagateToProjects = true;
		hubSecGhAppId = '';
		hubSecGhInstallId = '';
		hubSecGhPrivateKey = '';
		hubSecGhApiBase = '';
		hubSecGhExtraJson = '';
		showHubCreateSecret = true;
	}

	function storedSecretCarrierProjectId(): string {
		if (hubSecOrgWide) {
			return hubSecProjectId.trim() || (projects[0]?.id ?? '');
		}
		return hubSecProjectId.trim();
	}

	function hubCreateSecretValid(): boolean {
		if (!hubSecPath.trim()) return false;
		if (!hubSecOrgWide && !hubSecProjectId.trim()) return false;
		if (hubSecOrgWide && projects.length === 0) return false;
		if (hubSecKind === 'github_app') {
			return !!(
				hubSecGhAppId.trim() &&
				hubSecGhInstallId.trim() &&
				hubSecGhPrivateKey.trim()
			);
		}
		return !!hubSecValue.trim();
	}

	async function submitHubCreateSecret() {
		const carrierProjectId = storedSecretCarrierProjectId();
		if (!carrierProjectId) {
			hubSecretError = hubSecOrgWide
				? 'Add a project to this organization first (the API still needs one for routing), or create a project-scoped secret instead.'
				: 'Select a project';
			return;
		}
		hubSecretActionLoading = true;
		hubSecretError = null;
		try {
			let value: string;
			if (hubSecKind === 'github_app') {
				if (
					!hubSecGhAppId.trim() ||
					!hubSecGhInstallId.trim() ||
					!hubSecGhPrivateKey.trim()
				) {
					hubSecretError =
						'GitHub App: App ID, Installation ID, and private key are required';
					return;
				}
				const app_id = Number(hubSecGhAppId);
				const installation_id = Number(hubSecGhInstallId);
				if (!Number.isFinite(app_id) || !Number.isFinite(installation_id)) {
					hubSecretError = 'GitHub App: App ID and Installation ID must be numeric';
					return;
				}
				let extraFields: Record<string, unknown> = {};
				if (hubSecGhExtraJson.trim()) {
					try {
						const parsed = JSON.parse(hubSecGhExtraJson) as unknown;
						if (
							typeof parsed !== 'object' ||
							parsed === null ||
							Array.isArray(parsed)
						) {
							hubSecretError = 'GitHub App: Additional fields must be a JSON object';
							return;
						}
						extraFields = parsed as Record<string, unknown>;
					} catch {
						hubSecretError = 'GitHub App: Additional fields are not valid JSON';
						return;
					}
				}
				value = JSON.stringify({
					app_id,
					installation_id,
					private_key_pem: hubSecGhPrivateKey.trim(),
					...(hubSecGhApiBase.trim() ? { github_api_base: hubSecGhApiBase.trim() } : {}),
					...extraFields
				});
			} else {
				value = hubSecValue;
			}

			await apiMethods.storedSecrets.create(carrierProjectId, {
				path: hubSecPath.trim(),
				kind: hubSecKind,
				value,
				description: hubSecDescription.trim() || undefined,
				pipeline_id: hubSecOrgWide ? undefined : hubSecPipelineId || undefined,
				...(hubSecOrgWide
					? { scope: 'organization', propagate_to_projects: hubSecPropagateToProjects }
					: {})
			});
			showHubCreateSecret = false;
			await reloadList();
		} catch (e) {
			hubSecretError = e instanceof Error ? e.message : 'Failed to create secret';
		} finally {
			hubSecretActionLoading = false;
		}
	}

	function secretScopeLabel(s: WorkspaceStoredSecretListItem): string {
		if (s.project_id == null || s.project_id === '') return 'Organization';
		if (s.pipeline_id) return 'Pipeline';
		return 'Project';
	}

	function variableScopeLabel(v: WorkspaceVariableListItem): string {
		if (v.pipeline_id) return 'Pipeline';
		return 'Project';
	}
</script>

<svelte:head>
	<title>Secrets &amp; Variables | Meticulous</title>
</svelte:head>

<div class="space-y-6">
	<div>
		<h1 class="text-2xl font-bold text-[var(--text-primary)]">Secrets &amp; Variables</h1>
		<p class="mt-1 max-w-3xl text-sm text-[var(--text-secondary)]">
			Search, create, and browse environment variables and platform stored secrets across your organization. Filter
			by project, pipeline, and scope; use Add to create in place, or open a project or pipeline for more actions.
		</p>
	</div>

	<Card padding="none" class="p-4">
		<div class="flex flex-col gap-4">
			<Tabs items={kindTabs} bind:value={kind} />
			<div class="grid gap-3 md:grid-cols-2 lg:grid-cols-4">
				<div class="lg:col-span-2">
					<label class="mb-1 block text-xs font-medium text-[var(--text-secondary)]" for="hub-q"
						>Search</label
					>
					<form
						class="flex gap-2"
						onsubmit={(e) => {
							e.preventDefault();
							applySearch();
						}}
					>
						<Input
							id="hub-q"
							type="search"
							bind:value={searchInput}
							placeholder={kind === 'variables' ? 'Variable name…' : 'Secret path or description…'}
							class="flex-1"
						/>
						<Button variant="primary" type="submit">
							<Search class="h-4 w-4" />
							Search
						</Button>
					</form>
				</div>
				<div>
					<label class="mb-1 block text-xs font-medium text-[var(--text-secondary)]" for="hub-scope"
						>Scope level</label
					>
					<Select id="hub-scope" options={scopeOptions} bind:value={scopeLevel} />
				</div>
				<div>
					<label class="mb-1 block text-xs font-medium text-[var(--text-secondary)]" for="hub-proj"
						>Project</label
					>
					<Select
						id="hub-proj"
						options={projectOptions}
						bind:value={projectId}
						disabled={projectsLoading}
					/>
				</div>
			</div>
			<div class="max-w-md">
				<label class="mb-1 block text-xs font-medium text-[var(--text-secondary)]" for="hub-pipe"
					>Pipeline</label
				>
				<Select
					id="hub-pipe"
					options={pipelineOptions}
					bind:value={pipelineId}
					disabled={!projectId || pipelinesLoading}
				/>
			</div>
			<div class="flex flex-wrap gap-2">
				{#if kind === 'variables'}
					<Button variant="primary" size="sm" onclick={openHubCreateVariable} disabled={projects.length === 0}>
						<Plus class="h-4 w-4" />
						Add variable
					</Button>
				{:else}
					<Button variant="primary" size="sm" onclick={openHubCreateSecret} disabled={projects.length === 0}>
						<Plus class="h-4 w-4" />
						Add secret
					</Button>
				{/if}
				<Button variant="outline" size="sm" onclick={reloadList} loading={listLoading}>
					<RefreshCw class="h-4 w-4" />
					Refresh
				</Button>
			</div>
		</div>
	</Card>

	{#if listError}
		<Alert variant="error" title="Could not load" dismissible ondismiss={() => (listError = null)}>
			{listError}
		</Alert>
	{/if}

	{#if listLoading && (kind === 'variables' ? variableRows.length === 0 : secretRows.length === 0)}
		<Card>
			<div class="space-y-3 p-4">
				{#each Array(6) as _, i (i)}
					<Skeleton class="h-10 w-full" />
				{/each}
			</div>
		</Card>
	{:else if kind === 'variables'}
		{#if variableRows.length === 0}
			<Card>
				<EmptyState
					title="No variables match"
					description="Try widening filters or search, or add a variable for a project."
				>
					<Button
						variant="primary"
						onclick={openHubCreateVariable}
						disabled={projects.length === 0}
					>
						<Plus class="h-4 w-4" />
						Add variable
					</Button>
				</EmptyState>
			</Card>
		{:else}
			<div class="overflow-hidden rounded-lg border border-[var(--border-primary)]">
				<table class="w-full text-sm">
					<thead class="bg-[var(--bg-tertiary)]">
						<tr>
							<th class="px-4 py-3 text-left font-medium text-[var(--text-secondary)]">Name</th>
							<th class="px-4 py-3 text-left font-medium text-[var(--text-secondary)]">Scope</th>
							<th class="px-4 py-3 text-left font-medium text-[var(--text-secondary)]">Project</th>
							<th class="px-4 py-3 text-left font-medium text-[var(--text-secondary)]">Pipeline</th>
							<th class="px-4 py-3 text-left font-medium text-[var(--text-secondary)]">Value</th>
							<th class="px-4 py-3 text-left font-medium text-[var(--text-secondary)]">Updated</th>
							<th class="px-4 py-3 text-right font-medium text-[var(--text-secondary)]">Open</th>
						</tr>
					</thead>
					<tbody class="divide-y divide-[var(--border-secondary)]">
						{#each variableRows as v (v.id)}
							<tr class="bg-[var(--bg-secondary)]">
								<td class="px-4 py-3 font-mono text-xs">{v.name}</td>
								<td class="px-4 py-3">{variableScopeLabel(v)}</td>
								<td class="px-4 py-3">
									<div class="font-medium text-[var(--text-primary)]">{v.project_name}</div>
									<div class="text-xs text-[var(--text-secondary)]">{v.project_slug}</div>
								</td>
								<td class="px-4 py-3 text-[var(--text-secondary)]">
									{v.pipeline_name ?? '—'}
								</td>
								<td class="px-4 py-3 text-[var(--text-secondary)]">
									{#if v.is_sensitive}
										<span class="italic">hidden</span>
									{:else}
										{v.value ?? '—'}
									{/if}
								</td>
								<td class="px-4 py-3 text-[var(--text-secondary)]">
									{formatRelativeTime(v.updated_at)}
								</td>
								<td class="px-4 py-3 text-right">
									<div class="flex justify-end gap-1">
										<Button variant="ghost" size="sm" href="/projects/{v.project_id}">
											Project
											<ExternalLink class="h-3 w-3 opacity-70" />
										</Button>
										{#if v.pipeline_id}
											<Button variant="ghost" size="sm" href="/pipelines/{v.pipeline_id}">
												Pipeline
												<ExternalLink class="h-3 w-3 opacity-70" />
											</Button>
										{/if}
									</div>
								</td>
							</tr>
						{/each}
					</tbody>
				</table>
			</div>
		{/if}
	{:else if secretRows.length === 0}
		<Card>
			<EmptyState
				title="No stored secrets match"
				description="Try widening filters, or add a stored secret. Organization-wide secrets require an org admin."
			>
				<Button variant="primary" onclick={openHubCreateSecret} disabled={projects.length === 0}>
					<Plus class="h-4 w-4" />
					Add secret
				</Button>
			</EmptyState>
		</Card>
	{:else}
		<div class="overflow-hidden rounded-lg border border-[var(--border-primary)]">
			<table class="w-full text-sm">
				<thead class="bg-[var(--bg-tertiary)]">
					<tr>
						<th class="px-4 py-3 text-left font-medium text-[var(--text-secondary)]">Path</th>
						<th class="px-4 py-3 text-left font-medium text-[var(--text-secondary)]">Kind</th>
						<th class="px-4 py-3 text-left font-medium text-[var(--text-secondary)]">Scope</th>
						<th class="px-4 py-3 text-left font-medium text-[var(--text-secondary)]">Project</th>
						<th class="px-4 py-3 text-left font-medium text-[var(--text-secondary)]">Pipeline</th>
						<th class="px-4 py-3 text-left font-medium text-[var(--text-secondary)]">Version</th>
						<th class="px-4 py-3 text-left font-medium text-[var(--text-secondary)]">Updated</th>
						<th class="px-4 py-3 text-right font-medium text-[var(--text-secondary)]">Open</th>
					</tr>
				</thead>
				<tbody class="divide-y divide-[var(--border-secondary)]">
					{#each secretRows as s (s.id)}
						<tr class="bg-[var(--bg-secondary)]">
							<td class="px-4 py-3 font-mono text-xs">{s.path}</td>
							<td class="px-4 py-3">{s.kind}</td>
							<td class="px-4 py-3">
								<div class="flex flex-wrap items-center gap-1">
									<Badge variant="secondary">{secretScopeLabel(s)}</Badge>
									{#if (!s.project_id || s.project_id === '') && s.propagate_to_projects === false}
										<Badge variant="outline">Platform only</Badge>
									{/if}
								</div>
							</td>
							<td class="px-4 py-3 text-[var(--text-secondary)]">
								{#if s.project_name}
									<div class="font-medium text-[var(--text-primary)]">{s.project_name}</div>
									<div class="text-xs">{s.project_slug ?? ''}</div>
								{:else}
									—
								{/if}
							</td>
							<td class="px-4 py-3 text-[var(--text-secondary)]">
								{s.pipeline_name ?? '—'}
							</td>
							<td class="px-4 py-3 font-mono">v{s.version}</td>
							<td class="px-4 py-3 text-[var(--text-secondary)]">
								{formatRelativeTime(s.updated_at)}
							</td>
							<td class="px-4 py-3 text-right">
								<div class="flex justify-end gap-1">
									{#if s.project_id}
										<Button variant="ghost" size="sm" href="/projects/{s.project_id}">
											Project
											<ExternalLink class="h-3 w-3 opacity-70" />
										</Button>
									{/if}
									{#if s.pipeline_id}
										<Button variant="ghost" size="sm" href="/pipelines/{s.pipeline_id}">
											Pipeline
											<ExternalLink class="h-3 w-3 opacity-70" />
										</Button>
									{/if}
								</div>
							</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
	{/if}

	{#if nextCursor}
		<div class="flex justify-center">
			<Button variant="outline" onclick={loadMore} loading={listLoadingMore}>
				Load more
			</Button>
		</div>
	{/if}
</div>

<Dialog bind:open={showHubCreateVariable} title="Add environment variable">
	<div class="space-y-4">
		{#if hubVarError}
			<Alert variant="error" title="Error" dismissible ondismiss={() => (hubVarError = null)}>
				{hubVarError}
			</Alert>
		{/if}
		<div>
			<label class="mb-1 block text-sm font-medium text-[var(--text-primary)]" for="hub-v-proj">Project</label>
			<Select
				id="hub-v-proj"
				options={projectPickOptions}
				bind:value={hubVarProjectId}
				disabled={projectsLoading}
			/>
		</div>
		<div>
			<label class="mb-1 block text-sm font-medium text-[var(--text-primary)]" for="hub-v-scope">Scope</label>
			<Select
				id="hub-v-scope"
				options={hubVarPipelineOptions}
				bind:value={hubVarPipelineId}
				disabled={!hubVarProjectId || hubVarPipelinesLoading}
			/>
		</div>
		<div>
			<label class="mb-1 block text-sm font-medium" for="hub-v-name">Name</label>
			<Input id="hub-v-name" bind:value={hubVarName} placeholder="e.g. NODE_VERSION" />
		</div>
		<div>
			<label class="mb-1 block text-sm font-medium" for="hub-v-val">Value</label>
			<Input id="hub-v-val" bind:value={hubVarValue} />
		</div>
		<label class="flex items-center gap-2 text-sm">
			<input type="checkbox" bind:checked={hubVarSensitive} class="rounded border-[var(--border-primary)]" />
			Mask value in API responses (sensitive)
		</label>
		<div class="flex justify-end gap-2 pt-2">
			<Button variant="outline" onclick={() => (showHubCreateVariable = false)}>Cancel</Button>
			<Button
				variant="primary"
				onclick={submitHubCreateVariable}
				loading={hubVariableActionLoading}
				disabled={!hubVarProjectId.trim() || !hubVarName.trim()}
			>
				Save
			</Button>
		</div>
	</div>
</Dialog>

<Dialog bind:open={showHubCreateSecret} title="Add stored secret">
	<div class="max-h-[85vh] space-y-4 overflow-y-auto pr-1">
		{#if hubSecretError}
			<Alert variant="error" title="Error" dismissible ondismiss={() => (hubSecretError = null)}>
				{hubSecretError}
			</Alert>
		{/if}
		<div class="rounded-lg border border-[var(--border-primary)] bg-[var(--bg-tertiary)] p-3">
			<label class="flex cursor-pointer items-start gap-3">
				<input
					type="checkbox"
					class="mt-1 h-4 w-4 rounded border-[var(--border-primary)]"
					bind:checked={hubSecOrgWide}
				/>
				<span>
					<span class="text-sm font-medium text-[var(--text-primary)]">Organization-wide secret</span>
					<span class="mt-0.5 block text-xs text-[var(--text-secondary)]">
						Applies to the whole organization. Requires org admin. You do not need to pick a project below.
					</span>
				</span>
			</label>
			{#if hubSecOrgWide}
				<label class="mt-3 flex cursor-pointer items-start gap-3 border-t border-[var(--border-secondary)] pt-3">
					<input
						type="checkbox"
						class="mt-1 h-4 w-4 rounded border-[var(--border-primary)]"
						bind:checked={hubSecPropagateToProjects}
					/>
					<span>
						<span class="text-sm font-medium text-[var(--text-primary)]"
							>Expose to all projects and pipelines</span>
						<span class="mt-0.5 block text-xs text-[var(--text-secondary)]">
							When off, the secret is for platform features (e.g. importing the workflow catalog from source code)
							only, not
							<code class="font-mono">stored:</code> or project secret lists.
						</span>
					</span>
				</label>
			{/if}
		</div>
		{#if !hubSecOrgWide}
			<div>
				<label class="mb-1 block text-sm font-medium text-[var(--text-primary)]" for="hub-s-proj">Project</label>
				<p class="mb-2 text-xs text-[var(--text-secondary)]">
					Secret is stored under this project (and optional pipeline below).
				</p>
				<Select
					id="hub-s-proj"
					options={projectPickOptions}
					bind:value={hubSecProjectId}
					disabled={projectsLoading}
				/>
			</div>
		{:else}
			<p class="text-xs text-[var(--text-secondary)]">
				{#if projects.length > 0}
					No project selection required. The secret is not tied to a single project.
				{:else}
					Add at least one project to this organization before creating stored secrets.
				{/if}
			</p>
		{/if}
		<div>
			<label class="mb-1 block text-sm font-medium text-[var(--text-primary)]" for="hub-s-path">Logical name</label>
			<Input id="hub-s-path" bind:value={hubSecPath} placeholder="e.g. MY_API_TOKEN" />
		</div>
		<div>
			<label class="mb-1 block text-sm font-medium text-[var(--text-primary)]" for="hub-s-kind">Kind</label>
			<Select id="hub-s-kind" options={kindOptions} bind:value={hubSecKind} />
		</div>
		{#if hubSecKind === 'github_app'}
			<div class="space-y-3 rounded-lg border border-[var(--border-primary)] bg-[var(--bg-tertiary)] p-3">
				<p class="text-xs text-[var(--text-secondary)]">
					Create a GitHub App, install it on your org or repo, then paste credentials here. Values are encrypted.
				</p>
				<div class="grid gap-3 sm:grid-cols-2">
					<div>
						<label class="mb-1 block text-xs font-medium" for="hub-gh-app">App ID</label>
						<Input id="hub-gh-app" bind:value={hubSecGhAppId} placeholder="123456" />
					</div>
					<div>
						<label class="mb-1 block text-xs font-medium" for="hub-gh-inst">Installation ID</label>
						<Input id="hub-gh-inst" bind:value={hubSecGhInstallId} placeholder="78901234" />
					</div>
				</div>
				<div>
					<label class="mb-1 block text-xs font-medium" for="hub-gh-pem">Private key (PEM)</label>
					<textarea
						id="hub-gh-pem"
						bind:value={hubSecGhPrivateKey}
						rows="6"
						class="w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 font-mono text-xs text-[var(--text-primary)] focus:outline-none focus:ring-2 focus:ring-primary-500"
						placeholder="-----BEGIN RSA PRIVATE KEY----- ..."
					></textarea>
				</div>
				<div>
					<label class="mb-1 block text-xs font-medium" for="hub-gh-api">GitHub API base (optional)</label>
					<Input
						id="hub-gh-api"
						bind:value={hubSecGhApiBase}
						placeholder="https://api.github.com (default)"
					/>
				</div>
				<div>
					<label class="mb-1 block text-xs font-medium" for="hub-gh-extra">Additional fields (optional JSON)</label>
					<textarea
						id="hub-gh-extra"
						bind:value={hubSecGhExtraJson}
						rows="3"
						class="w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 font-mono text-xs text-[var(--text-primary)] focus:outline-none focus:ring-2 focus:ring-primary-500"
						placeholder={`{\n  "client_id": "..."\n}`}
					></textarea>
				</div>
			</div>
		{:else}
			<div>
				<label class="mb-1 block text-sm font-medium text-[var(--text-primary)]" for="hub-s-val"
					>Value (one-time)</label
				>
				<textarea
					id="hub-s-val"
					bind:value={hubSecValue}
					rows="4"
					class="w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm text-[var(--text-primary)] focus:outline-none focus:ring-2 focus:ring-primary-500"
					placeholder="Secret value or PEM / JSON payload"
				></textarea>
			</div>
		{/if}
		<div>
			<label class="mb-1 block text-sm font-medium text-[var(--text-primary)]" for="hub-s-desc"
				>Description (optional)</label
			>
			<Input id="hub-s-desc" bind:value={hubSecDescription} />
		</div>
		{#if !hubSecOrgWide}
			<div>
				<label class="mb-1 block text-sm font-medium text-[var(--text-primary)]" for="hub-s-scope">Scope</label>
				<Select
					id="hub-s-scope"
					options={hubSecPipelineOptions}
					bind:value={hubSecPipelineId}
					disabled={!hubSecProjectId || hubSecPipelinesLoading}
				/>
			</div>
		{/if}
		<div class="flex justify-end gap-2 pt-2">
			<Button variant="outline" onclick={() => (showHubCreateSecret = false)}>Cancel</Button>
			<Button
				variant="primary"
				onclick={submitHubCreateSecret}
				loading={hubSecretActionLoading}
				disabled={!hubCreateSecretValid()}
			>
				Save
			</Button>
		</div>
	</div>
</Dialog>
