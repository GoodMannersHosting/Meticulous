<script lang="ts">
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { Button, Card, Badge, Tabs, Dialog, Input, Alert, Select } from '$components/ui';
	import { DataTable, EmptyState, Skeleton } from '$components/data';
	import { apiMethods } from '$api/client';
	import type { Project, Pipeline, ProjectVariable, StoredSecret } from '$api/types';
	import { formatRelativeTime } from '$utils/format';
	import {
		ArrowLeft,
		Plus,
		GitBranch,
		Play,
		Settings,
		Trash2,
		Edit,
		KeyRound,
		RefreshCw,
		Braces,
		History
	} from 'lucide-svelte';
	import type { Column } from '$components/data/DataTable.svelte';

	let project = $state<Project | null>(null);
	let pipelines = $state<Pipeline[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let activeTab = $state('pipelines');

	let secrets = $state<StoredSecret[]>([]);
	let secretsLoading = $state(false);
	let secretsError = $state<string | null>(null);
	let showCreateSecret = $state(false);
	let createPath = $state('');
	let createKind = $state('kv');
	let createValue = $state('');
	let createDescription = $state('');
	let createPipelineId = $state('');
	let ghAppId = $state('');
	let ghInstallationId = $state('');
	let ghPrivateKey = $state('');
	let ghApiBase = $state('');
	let secretActionLoading = $state(false);
	let rotateTarget = $state<StoredSecret | null>(null);
	let rotateValue = $state('');
	let showRotateSecretDialog = $state(false);
	let deleteTarget = $state<StoredSecret | null>(null);
	let showDeleteSecretDialog = $state(false);
	let showSecretVersionsDialog = $state(false);
	let versionsContext = $state<StoredSecret | null>(null);
	let secretVersionRows = $state<StoredSecret[]>([]);
	let versionsLoading = $state(false);
	let versionsError = $state<string | null>(null);
	let purgeVersionTarget = $state<StoredSecret | null>(null);
	let showPurgeVersionDialog = $state(false);

	let variables = $state<ProjectVariable[]>([]);
	let variablesLoading = $state(false);
	let variablesError = $state<string | null>(null);
	let showCreateVariable = $state(false);
	let cvName = $state('');
	let cvValue = $state('');
	let cvSensitive = $state(false);
	let cvPipelineId = $state('');
	let variableActionLoading = $state(false);
	let editVariableTarget = $state<ProjectVariable | null>(null);
	let evName = $state('');
	let evValue = $state('');
	let evSensitive = $state(false);
	let showEditVariableDialog = $state(false);
	let deleteVariableTarget = $state<ProjectVariable | null>(null);
	let showDeleteVariableDialog = $state(false);
	let ghExtraJson = $state('');

	const kindOptions = [
		{ value: 'kv', label: 'Key / value (kv)' },
		{ value: 'api_key', label: 'API key' },
		{ value: 'ssh_private_key', label: 'SSH private key (PEM)' },
		{ value: 'github_app', label: 'GitHub App' },
		{ value: 'x509_bundle', label: 'X.509 bundle (JSON)' }
	];

	const tabs = [
		{ id: 'pipelines', label: 'Pipelines', icon: GitBranch },
		{ id: 'variables', label: 'Variables', icon: Braces },
		{ id: 'secrets', label: 'Secrets', icon: KeyRound },
		{ id: 'runs', label: 'Recent Runs', icon: Play },
		{ id: 'settings', label: 'Settings', icon: Settings }
	];

	$effect(() => {
		loadProject();
	});

	async function loadProject() {
		loading = true;
		error = null;
		try {
			const projectId = $page.params.id!;
			project = await apiMethods.projects.get(projectId);
			const pipelinesResponse = await apiMethods.pipelines.list({ project_id: projectId });
			pipelines = pipelinesResponse.data;
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load project';
		} finally {
			loading = false;
		}
	}

	const pipelineColumns: Column<Pipeline>[] = [
		{
			key: 'name',
			label: 'Pipeline',
			sortable: true,
			render: (_, row) => `
				<div>
					<div class="font-medium text-[var(--text-primary)]">${row.name}</div>
					<div class="text-sm text-[var(--text-secondary)]">${row.slug}</div>
				</div>
			`
		},
		{
			key: 'enabled',
			label: 'Status',
			render: (value) =>
				value
					? '<span class="inline-flex items-center gap-1.5 text-sm text-success-600 dark:text-success-400"><span class="h-2 w-2 rounded-full bg-success-500"></span>Active</span>'
					: '<span class="inline-flex items-center gap-1.5 text-sm text-secondary-500"><span class="h-2 w-2 rounded-full bg-secondary-400"></span>Disabled</span>'
		},
		{
			key: 'updated_at',
			label: 'Last Updated',
			sortable: true,
			render: (value) => formatRelativeTime(value as string)
		}
	];

	function handlePipelineClick(pipeline: Pipeline) {
		goto(`/pipelines/${pipeline.id}`);
	}

	function pipelineLabel(id: string | null | undefined): string {
		if (!id) return '—';
		const p = pipelines.find((x) => x.id === id);
		return p ? p.name : id.slice(0, 8);
	}

	async function loadSecrets() {
		if (!project) return;
		secretsLoading = true;
		secretsError = null;
		try {
			secrets = await apiMethods.storedSecrets.list(project.id);
		} catch (e) {
			secretsError = e instanceof Error ? e.message : 'Failed to load secrets';
			secrets = [];
		} finally {
			secretsLoading = false;
		}
	}

	$effect(() => {
		const pid = project?.id;
		if (activeTab !== 'secrets' || !pid || loading) return;
		void loadSecrets();
	});

	async function loadVariables() {
		if (!project) return;
		variablesLoading = true;
		variablesError = null;
		try {
			const res = await apiMethods.variables.list(project.id);
			variables = res.data;
		} catch (e) {
			variablesError = e instanceof Error ? e.message : 'Failed to load variables';
			variables = [];
		} finally {
			variablesLoading = false;
		}
	}

	$effect(() => {
		const pid = project?.id;
		if (activeTab !== 'variables' || !pid || loading) return;
		void loadVariables();
	});

	function openCreateSecret() {
		createPath = '';
		createKind = 'kv';
		createValue = '';
		createDescription = '';
		createPipelineId = '';
		ghAppId = '';
		ghInstallationId = '';
		ghPrivateKey = '';
		ghApiBase = '';
		ghExtraJson = '';
		showCreateSecret = true;
	}

	async function submitCreateSecret() {
		if (!project) return;
		secretActionLoading = true;
		secretsError = null;
		try {
			let value: string;
			if (createKind === 'github_app') {
				if (!ghAppId.trim() || !ghInstallationId.trim() || !ghPrivateKey.trim()) {
					secretsError = 'GitHub App: App ID, Installation ID, and private key are required';
					return;
				}
				const app_id = Number(ghAppId);
				const installation_id = Number(ghInstallationId);
				if (!Number.isFinite(app_id) || !Number.isFinite(installation_id)) {
					secretsError = 'GitHub App: App ID and Installation ID must be numeric';
					return;
				}
				let extraFields: Record<string, unknown> = {};
				if (ghExtraJson.trim()) {
					try {
						const parsed = JSON.parse(ghExtraJson) as unknown;
						if (
							typeof parsed !== 'object' ||
							parsed === null ||
							Array.isArray(parsed)
						) {
							secretsError = 'GitHub App: Additional fields must be a JSON object';
							return;
						}
						extraFields = parsed as Record<string, unknown>;
					} catch {
						secretsError = 'GitHub App: Additional fields are not valid JSON';
						return;
					}
				}
				value = JSON.stringify({
					app_id,
					installation_id,
					private_key_pem: ghPrivateKey.trim(),
					...(ghApiBase.trim() ? { github_api_base: ghApiBase.trim() } : {}),
					...extraFields
				});
			} else {
				value = createValue;
			}

			await apiMethods.storedSecrets.create(project.id, {
				path: createPath.trim(),
				kind: createKind,
				value,
				description: createDescription.trim() || undefined,
				pipeline_id: createPipelineId || undefined
			});
			showCreateSecret = false;
			await loadSecrets();
		} catch (e) {
			secretsError = e instanceof Error ? e.message : 'Failed to create secret';
		} finally {
			secretActionLoading = false;
		}
	}

	function createSecretValid(): boolean {
		if (!createPath.trim()) return false;
		if (createKind === 'github_app') {
			return !!(ghAppId.trim() && ghInstallationId.trim() && ghPrivateKey.trim());
		}
		return !!createValue.trim();
	}

	async function submitRotateSecret() {
		if (!rotateTarget) return;
		secretActionLoading = true;
		secretsError = null;
		try {
			await apiMethods.storedSecrets.rotate(rotateTarget.id, rotateValue);
			showRotateSecretDialog = false;
			rotateTarget = null;
			rotateValue = '';
			await loadSecrets();
		} catch (e) {
			secretsError = e instanceof Error ? e.message : 'Failed to rotate secret';
		} finally {
			secretActionLoading = false;
		}
	}

	async function submitDeleteSecret() {
		if (!deleteTarget) return;
		secretActionLoading = true;
		secretsError = null;
		try {
			await apiMethods.storedSecrets.delete(deleteTarget.id);
			showDeleteSecretDialog = false;
			deleteTarget = null;
			await loadSecrets();
		} catch (e) {
			secretsError = e instanceof Error ? e.message : 'Failed to delete secret';
		} finally {
			secretActionLoading = false;
		}
	}

	function openSecretVersions(s: StoredSecret) {
		versionsContext = s;
		versionsError = null;
		secretVersionRows = [];
		showSecretVersionsDialog = true;
		void refreshSecretVersions();
	}

	async function refreshSecretVersions() {
		const ctx = versionsContext;
		const proj = project;
		if (!ctx || !proj) return;
		versionsLoading = true;
		versionsError = null;
		try {
			secretVersionRows = await apiMethods.storedSecrets.listVersions(proj.id, {
				path: ctx.path,
				...(ctx.pipeline_id ? { pipeline_id: ctx.pipeline_id } : {})
			});
		} catch (e) {
			versionsError = e instanceof Error ? e.message : 'Failed to load versions';
			secretVersionRows = [];
		} finally {
			versionsLoading = false;
		}
	}

	async function submitActivateSecretVersion(row: StoredSecret) {
		secretActionLoading = true;
		versionsError = null;
		secretsError = null;
		try {
			await apiMethods.storedSecrets.activateVersion(row.id);
			await loadSecrets();
			await refreshSecretVersions();
		} catch (e) {
			versionsError = e instanceof Error ? e.message : 'Failed to roll back';
		} finally {
			secretActionLoading = false;
		}
	}

	async function submitPurgeSecretVersion() {
		if (!purgeVersionTarget) return;
		secretActionLoading = true;
		versionsError = null;
		secretsError = null;
		try {
			await apiMethods.storedSecrets.purgeVersionPermanent(purgeVersionTarget.id);
			showPurgeVersionDialog = false;
			purgeVersionTarget = null;
			await loadSecrets();
			await refreshSecretVersions();
		} catch (e) {
			versionsError = e instanceof Error ? e.message : 'Failed to purge version';
		} finally {
			secretActionLoading = false;
		}
	}

	const pipelineScopeOptions = $derived([
		{ value: '', label: 'Project-wide (all pipelines)' },
		...pipelines.map((p) => ({ value: p.id, label: p.name }))
	]);

	function openCreateVariable() {
		cvName = '';
		cvValue = '';
		cvSensitive = false;
		cvPipelineId = '';
		showCreateVariable = true;
	}

	async function submitCreateVariable() {
		if (!project) return;
		variableActionLoading = true;
		variablesError = null;
		try {
			await apiMethods.variables.create(project.id, {
				name: cvName.trim(),
				value: cvValue,
				is_sensitive: cvSensitive,
				pipeline_id: cvPipelineId || undefined
			});
			showCreateVariable = false;
			await loadVariables();
		} catch (e) {
			variablesError = e instanceof Error ? e.message : 'Failed to create variable';
		} finally {
			variableActionLoading = false;
		}
	}

	function openEditVariable(v: ProjectVariable) {
		editVariableTarget = v;
		evName = v.name;
		evValue = v.value ?? '';
		evSensitive = v.is_sensitive;
		showEditVariableDialog = true;
	}

	async function submitEditVariable() {
		if (!editVariableTarget) return;
		variableActionLoading = true;
		variablesError = null;
		try {
			await apiMethods.variables.update(editVariableTarget.id, {
				name: evName.trim(),
				...(evValue !== '' ? { value: evValue } : {}),
				is_sensitive: evSensitive
			});
			showEditVariableDialog = false;
			editVariableTarget = null;
			await loadVariables();
		} catch (e) {
			variablesError = e instanceof Error ? e.message : 'Failed to update variable';
		} finally {
			variableActionLoading = false;
		}
	}

	async function submitDeleteVariable() {
		if (!deleteVariableTarget) return;
		variableActionLoading = true;
		variablesError = null;
		try {
			await apiMethods.variables.delete(deleteVariableTarget.id);
			showDeleteVariableDialog = false;
			deleteVariableTarget = null;
			await loadVariables();
		} catch (e) {
			variablesError = e instanceof Error ? e.message : 'Failed to delete variable';
		} finally {
			variableActionLoading = false;
		}
	}

	function variableScopeLabel(v: ProjectVariable): string {
		if (!v.pipeline_id) return 'Project';
		return pipelineLabel(v.pipeline_id);
	}
</script>

<svelte:head>
	<title>{project?.name ?? 'Project'} | Meticulous</title>
</svelte:head>

<div class="space-y-6">
	<div class="flex items-center gap-4">
		<Button variant="ghost" size="sm" href="/projects">
			<ArrowLeft class="h-4 w-4" />
		</Button>

		{#if loading}
			<div class="space-y-2">
				<Skeleton class="h-7 w-48" />
				<Skeleton class="h-4 w-32" />
			</div>
		{:else if project}
			<div class="flex-1">
				<div class="flex items-center gap-3">
					<h1 class="text-2xl font-bold text-[var(--text-primary)]">{project.name}</h1>
				</div>
				{#if project.description}
					<p class="mt-1 text-[var(--text-secondary)]">{project.description}</p>
				{/if}
			</div>

			<div class="flex items-center gap-2">
				<Button variant="outline" size="sm">
					<Edit class="h-4 w-4" />
					Edit
				</Button>
				<Button variant="primary" href="/pipelines/new?project={project.id}">
					<Plus class="h-4 w-4" />
					New Pipeline
				</Button>
			</div>
		{/if}
	</div>

	{#if error}
		<Alert variant="error" title="Error">
			{error}
		</Alert>
	{/if}

	{#if !loading && project}
		<Tabs items={tabs} bind:value={activeTab} />

		{#if activeTab === 'pipelines'}
			{#if pipelines.length === 0}
				<Card>
					<EmptyState
						title="No pipelines yet"
						description="Create your first pipeline to start automating your builds."
					>
						<Button variant="primary" href="/pipelines/new?project={project.id}">
							<Plus class="h-4 w-4" />
							Create Pipeline
						</Button>
					</EmptyState>
				</Card>
			{:else}
				<DataTable
					columns={pipelineColumns}
					data={pipelines}
					rowKey="id"
					onRowClick={handlePipelineClick}
				/>
			{/if}
		{:else if activeTab === 'variables'}
			<div class="flex flex-wrap items-center justify-between gap-3">
				<p class="text-sm text-[var(--text-secondary)]">
					Non-secret configuration merged into runs: <strong>project</strong> variables apply to all pipelines;
					<strong>pipeline</strong> rows override for that pipeline. Pipeline YAML <code
						class="rounded bg-[var(--bg-tertiary)] px-1 font-mono text-xs">variables:</code
					>
					and trigger payloads override these for the same name.
				</p>
				<div class="flex gap-2">
					<Button variant="outline" size="sm" onclick={loadVariables} loading={variablesLoading}>
						<RefreshCw class="h-4 w-4" />
						Refresh
					</Button>
					<Button variant="primary" size="sm" onclick={openCreateVariable}>
						<Plus class="h-4 w-4" />
						Add variable
					</Button>
				</div>
			</div>
			{#if variablesError}
				<Alert variant="error" title="Variables" dismissible ondismiss={() => (variablesError = null)}>
					{variablesError}
				</Alert>
			{/if}
			{#if variablesLoading && variables.length === 0}
				<Card>
					<div class="space-y-3 p-4">
						{#each Array(4) as _, i (i)}
							<Skeleton class="h-10 w-full" />
						{/each}
					</div>
				</Card>
			{:else if variables.length === 0}
				<Card>
					<EmptyState title="No variables" description="Add project or pipeline-scoped values for use in pipelines.">
						<Button variant="primary" onclick={openCreateVariable}>
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
								<th class="px-4 py-3 text-left font-medium text-[var(--text-secondary)]">Value</th>
								<th class="px-4 py-3 text-left font-medium text-[var(--text-secondary)]">Sensitive</th>
								<th class="px-4 py-3 text-right font-medium text-[var(--text-secondary)]">Actions</th>
							</tr>
						</thead>
						<tbody class="divide-y divide-[var(--border-secondary)]">
							{#each variables as v (v.id)}
								<tr class="bg-[var(--bg-secondary)]">
									<td class="px-4 py-3 font-mono text-sm">{v.name}</td>
									<td class="px-4 py-3">{variableScopeLabel(v)}</td>
									<td class="px-4 py-3 text-[var(--text-secondary)]">
										{#if v.is_sensitive}
											<span class="italic">hidden</span>
										{:else}
											{v.value ?? '—'}
										{/if}
									</td>
									<td class="px-4 py-3">{v.is_sensitive ? 'Yes' : 'No'}</td>
									<td class="px-4 py-3 text-right">
										<div class="flex justify-end gap-2">
											<Button variant="ghost" size="sm" onclick={() => openEditVariable(v)}>
												Edit
											</Button>
											<Button
												variant="ghost"
												size="sm"
												onclick={() => {
													deleteVariableTarget = v;
													showDeleteVariableDialog = true;
												}}
											>
												<Trash2 class="h-4 w-4" />
											</Button>
										</div>
									</td>
								</tr>
							{/each}
						</tbody>
					</table>
				</div>
			{/if}
		{:else if activeTab === 'secrets'}
			<div class="flex flex-wrap items-center justify-between gap-3">
				<p class="text-sm text-[var(--text-secondary)]">
					Values are encrypted and never shown again after save. Reference them in pipeline YAML with{' '}
					<code class="rounded bg-[var(--bg-tertiary)] px-1 font-mono text-xs"
						>stored: &#123; name: MY_TOKEN &#125;</code
					>
					(use the same logical name you entered here).
				</p>
				<div class="flex gap-2">
					<Button variant="outline" size="sm" onclick={loadSecrets} loading={secretsLoading}>
						<RefreshCw class="h-4 w-4" />
						Refresh
					</Button>
					<Button variant="primary" size="sm" onclick={openCreateSecret}>
						<Plus class="h-4 w-4" />
						Add secret
					</Button>
				</div>
			</div>

			{#if secretsError}
				<Alert variant="error" title="Secrets" dismissible ondismiss={() => (secretsError = null)}>
					{secretsError}
				</Alert>
			{/if}

			{#if secretsLoading && secrets.length === 0}
				<Card>
					<div class="space-y-3 p-4">
						{#each Array(4) as _, i (i)}
							<Skeleton class="h-10 w-full" />
						{/each}
					</div>
				</Card>
			{:else if secrets.length === 0}
				<Card>
					<EmptyState
						title="No stored secrets"
						description="Create a secret to inject into jobs via the pipeline secrets block."
					>
						<Button variant="primary" onclick={openCreateSecret}>
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
								<th class="px-4 py-3 text-left font-medium text-[var(--text-secondary)]">Name</th>
								<th class="px-4 py-3 text-left font-medium text-[var(--text-secondary)]">Kind</th>
								<th class="px-4 py-3 text-left font-medium text-[var(--text-secondary)]">Scope</th>
								<th class="px-4 py-3 text-left font-medium text-[var(--text-secondary)]">Version</th>
								<th class="px-4 py-3 text-left font-medium text-[var(--text-secondary)]">Updated</th>
								<th class="px-4 py-3 text-right font-medium text-[var(--text-secondary)]">Actions</th>
							</tr>
						</thead>
						<tbody class="divide-y divide-[var(--border-secondary)]">
							{#each secrets as s (s.id)}
								<tr class="bg-[var(--bg-secondary)]">
									<td class="px-4 py-3 font-mono text-sm">{s.path}</td>
									<td class="px-4 py-3">{s.kind}</td>
									<td class="px-4 py-3">
										{s.pipeline_id ? pipelineLabel(s.pipeline_id) : 'Project'}
									</td>
									<td class="px-4 py-3 font-mono">
										<button
											type="button"
											class="text-primary-600 hover:underline dark:text-primary-400"
											onclick={() => openSecretVersions(s)}
										>
											v{s.version}
										</button>
									</td>
									<td class="px-4 py-3 text-[var(--text-secondary)]">
										{formatRelativeTime(s.updated_at)}
									</td>
									<td class="px-4 py-3 text-right">
										<div class="flex justify-end gap-2">
											<Button
												variant="ghost"
												size="sm"
												title="Versions, roll back, purge"
												onclick={() => openSecretVersions(s)}
											>
												<History class="h-4 w-4" />
											</Button>
											<Button
												variant="ghost"
												size="sm"
												onclick={() => {
													rotateTarget = s;
													rotateValue = '';
													showRotateSecretDialog = true;
												}}
											>
												Rotate
											</Button>
											<Button
												variant="ghost"
												size="sm"
												onclick={() => {
													deleteTarget = s;
													showDeleteSecretDialog = true;
												}}
											>
												<Trash2 class="h-4 w-4" />
											</Button>
										</div>
									</td>
								</tr>
							{/each}
						</tbody>
					</table>
				</div>
			{/if}
		{:else if activeTab === 'runs'}
			<Card>
				<EmptyState
					title="No recent runs"
					description="Runs will appear here when you trigger a pipeline."
				/>
			</Card>
		{:else if activeTab === 'settings'}
			<Card>
				<div class="space-y-6">
					<div>
						<h3 class="text-lg font-medium text-[var(--text-primary)]">Project Settings</h3>
						<p class="mt-1 text-sm text-[var(--text-secondary)]">
							Manage project configuration and access.
						</p>
					</div>

					<div class="border-t border-[var(--border-primary)] pt-6">
						<h4 class="text-sm font-medium text-[var(--text-primary)]">Danger Zone</h4>
						<div class="mt-4 rounded-lg border border-error-200 p-4 dark:border-error-800">
							<div class="flex items-center justify-between">
								<div>
									<p class="font-medium text-error-700 dark:text-error-400">Delete Project</p>
									<p class="mt-1 text-sm text-[var(--text-secondary)]">
										Once deleted, all pipelines and runs will be permanently removed.
									</p>
								</div>
								<Button variant="destructive" size="sm">
									<Trash2 class="h-4 w-4" />
									Delete
								</Button>
							</div>
						</div>
					</div>
				</div>
			</Card>
		{/if}
	{/if}
</div>

<Dialog bind:open={showCreateSecret} title="Add stored secret">
	<div class="space-y-4">
		<div>
			<label class="mb-1 block text-sm font-medium text-[var(--text-primary)]" for="sec-path"
				>Logical name</label
			>
			<Input id="sec-path" bind:value={createPath} placeholder="e.g. MY_API_TOKEN" />
		</div>
		<div>
			<label class="mb-1 block text-sm font-medium text-[var(--text-primary)]" for="sec-kind">Kind</label>
			<Select id="sec-kind" options={kindOptions} bind:value={createKind} />
		</div>
		{#if createKind === 'github_app'}
			<div class="rounded-lg border border-[var(--border-primary)] bg-[var(--bg-tertiary)] p-3 space-y-3">
				<p class="text-xs text-[var(--text-secondary)]">
					Create a GitHub App, install it on your org or repo, then paste credentials here. Values are encrypted; the
					private key never leaves the control plane except to mint short-lived tokens for jobs.
				</p>
				<div class="grid gap-3 sm:grid-cols-2">
					<div>
						<label class="mb-1 block text-xs font-medium" for="gh-app-id">App ID</label>
						<Input id="gh-app-id" bind:value={ghAppId} placeholder="123456" />
					</div>
					<div>
						<label class="mb-1 block text-xs font-medium" for="gh-install">Installation ID</label>
						<Input id="gh-install" bind:value={ghInstallationId} placeholder="78901234" />
					</div>
				</div>
				<div>
					<label class="mb-1 block text-xs font-medium" for="gh-pem">Private key (PEM)</label>
					<textarea
						id="gh-pem"
						bind:value={ghPrivateKey}
						rows="6"
						class="w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 font-mono text-xs text-[var(--text-primary)] focus:outline-none focus:ring-2 focus:ring-primary-500"
						placeholder="-----BEGIN RSA PRIVATE KEY----- ..."
					></textarea>
				</div>
				<div>
					<label class="mb-1 block text-xs font-medium" for="gh-api-base">GitHub API base (optional)</label>
					<Input
						id="gh-api-base"
						bind:value={ghApiBase}
						placeholder="https://api.github.com (default)"
					/>
				</div>
				<div>
					<label class="mb-1 block text-xs font-medium" for="gh-extra">Additional fields (optional JSON object)</label>
					<textarea
						id="gh-extra"
						bind:value={ghExtraJson}
						rows="3"
						class="w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 font-mono text-xs text-[var(--text-primary)] focus:outline-none focus:ring-2 focus:ring-primary-500"
						placeholder={`{\n  "client_id": "...",\n  "webhook_secret": "..."\n}`}
					></textarea>
				</div>
			</div>
		{:else}
			<div>
				<label class="mb-1 block text-sm font-medium text-[var(--text-primary)]" for="sec-val"
					>Value (one-time)</label
				>
				<textarea
					id="sec-val"
					bind:value={createValue}
					rows="4"
					class="w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm text-[var(--text-primary)] focus:outline-none focus:ring-2 focus:ring-primary-500"
					placeholder="Secret value or PEM / JSON payload"
				></textarea>
			</div>
		{/if}
		<div>
			<label class="mb-1 block text-sm font-medium text-[var(--text-primary)]" for="sec-desc"
				>Description (optional)</label
			>
			<Input id="sec-desc" bind:value={createDescription} />
		</div>
		<div>
			<label class="mb-1 block text-sm font-medium text-[var(--text-primary)]" for="sec-scope">Scope</label>
			<Select id="sec-scope" options={pipelineScopeOptions} bind:value={createPipelineId} />
		</div>
		<div class="flex justify-end gap-2 pt-2">
			<Button variant="outline" onclick={() => (showCreateSecret = false)}>Cancel</Button>
			<Button
				variant="primary"
				onclick={submitCreateSecret}
				loading={secretActionLoading}
				disabled={!createSecretValid()}
			>
				Save
			</Button>
		</div>
	</div>
</Dialog>

<Dialog
	bind:open={showRotateSecretDialog}
	title="Rotate secret"
	onclose={() => {
		rotateTarget = null;
		rotateValue = '';
	}}
>
	{#if rotateTarget}
		<p class="text-sm text-[var(--text-secondary)]">
			New value for <span class="font-mono text-[var(--text-primary)]">{rotateTarget.path}</span> (creates a new
			version).
		</p>
		{#if rotateTarget.kind === 'github_app'}
			<p class="mt-2 text-xs text-[var(--text-secondary)]">
				Use a single JSON object with <code class="font-mono">app_id</code>, <code class="font-mono"
					>installation_id</code
				>, <code class="font-mono">private_key_pem</code>, optional <code class="font-mono"
					>github_api_base</code
				>, and any other fields you need preserved.
			</p>
		{/if}
		<div class="mt-4">
			<textarea
				bind:value={rotateValue}
				rows={rotateTarget.kind === 'github_app' ? 14 : 4}
				class="w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 font-mono text-sm text-[var(--text-primary)] focus:outline-none focus:ring-2 focus:ring-primary-500"
			></textarea>
		</div>
		<div class="mt-6 flex justify-end gap-2">
			<Button
				variant="outline"
				onclick={() => {
					showRotateSecretDialog = false;
					rotateTarget = null;
					rotateValue = '';
				}}
			>
				Cancel
			</Button>
			<Button
				variant="primary"
				onclick={submitRotateSecret}
				loading={secretActionLoading}
				disabled={!rotateValue?.trim()}
			>
				Rotate
			</Button>
		</div>
	{/if}
</Dialog>

<Dialog
	bind:open={showDeleteSecretDialog}
	title="Delete secret?"
	onclose={() => {
		deleteTarget = null;
	}}
>
	{#if deleteTarget}
		<p class="text-sm text-[var(--text-secondary)]">
			Soft-delete <span class="font-mono">{deleteTarget.path}</span>? Runs that still need it may fail validation.
		</p>
		<div class="mt-6 flex justify-end gap-2">
			<Button variant="outline" onclick={() => (showDeleteSecretDialog = false)}>Cancel</Button>
			<Button
				variant="primary"
				class="bg-red-600 hover:bg-red-700"
				onclick={submitDeleteSecret}
				loading={secretActionLoading}
			>
				Delete
			</Button>
		</div>
	{/if}
</Dialog>

<Dialog
	bind:open={showSecretVersionsDialog}
	title="Secret versions"
	onclose={() => {
		versionsContext = null;
		secretVersionRows = [];
	}}
>
	{#if versionsContext}
		<p class="text-sm text-[var(--text-secondary)]">
			<span class="font-mono text-[var(--text-primary)]">{versionsContext.path}</span>
			·
			{versionsContext.pipeline_id ? pipelineLabel(versionsContext.pipeline_id) : 'Project-wide'}
		</p>
		<p class="mt-2 text-xs text-[var(--text-tertiary)]">
			Roll back soft-deletes newer ciphertext rows so jobs resolve this version. Purge permanently removes one row from
			the database (including soft-deleted rows).
		</p>
		{#if versionsError}
			<div class="mt-3 rounded-lg border border-red-200 bg-red-50 p-2 text-sm text-red-700 dark:border-red-900 dark:bg-red-950/50 dark:text-red-400">
				{versionsError}
			</div>
		{/if}
		<div class="mt-3 flex justify-end">
			<Button variant="ghost" size="sm" onclick={refreshSecretVersions} loading={versionsLoading}>
				<RefreshCw class="h-4 w-4" />
				Refresh
			</Button>
		</div>
		{#if versionsLoading && secretVersionRows.length === 0}
			<div class="mt-4 space-y-2">
				{#each Array(3) as _, i (i)}
					<Skeleton class="h-10 w-full" />
				{/each}
			</div>
		{:else if secretVersionRows.length === 0}
			<p class="mt-4 text-sm text-[var(--text-secondary)]">No versions found.</p>
		{:else}
			<div class="mt-4 max-h-80 overflow-auto rounded-lg border border-[var(--border-primary)]">
				<table class="w-full text-sm">
					<thead class="sticky top-0 bg-[var(--bg-tertiary)]">
						<tr>
							<th class="px-3 py-2 text-left font-medium text-[var(--text-secondary)]">Ver</th>
							<th class="px-3 py-2 text-left font-medium text-[var(--text-secondary)]">Updated</th>
							<th class="px-3 py-2 text-left font-medium text-[var(--text-secondary)]">Row</th>
							<th class="px-3 py-2 text-right font-medium text-[var(--text-secondary)]">Actions</th>
						</tr>
					</thead>
					<tbody class="divide-y divide-[var(--border-secondary)]">
						{#each secretVersionRows as row, idx (row.id)}
							<tr class="bg-[var(--bg-secondary)]">
								<td class="px-3 py-2 font-mono">
									v{row.version}
									{#if idx === 0}
										<Badge variant="success" size="sm" class="ml-2">Current</Badge>
									{/if}
								</td>
								<td class="px-3 py-2 text-[var(--text-secondary)]">
									{formatRelativeTime(row.updated_at)}
								</td>
								<td class="px-3 py-2 font-mono text-xs text-[var(--text-tertiary)]">
									{row.id.slice(0, 8)}…
								</td>
								<td class="px-3 py-2 text-right">
									<div class="flex flex-wrap justify-end gap-1">
										{#if idx > 0}
											<Button
												variant="outline"
												size="sm"
												onclick={() => submitActivateSecretVersion(row)}
												disabled={secretActionLoading}
											>
												Roll back here
											</Button>
										{/if}
										<Button
											variant="ghost"
											size="sm"
											class="text-red-600 hover:bg-red-50 dark:text-red-400 dark:hover:bg-red-950/40"
											onclick={() => {
												purgeVersionTarget = row;
												showPurgeVersionDialog = true;
											}}
											disabled={secretActionLoading}
										>
											Purge
										</Button>
									</div>
								</td>
							</tr>
						{/each}
					</tbody>
				</table>
			</div>
		{/if}
		<div class="mt-4 flex justify-end">
			<Button variant="outline" onclick={() => (showSecretVersionsDialog = false)}>Close</Button>
		</div>
	{/if}
</Dialog>

<Dialog
	bind:open={showPurgeVersionDialog}
	title="Purge version permanently?"
	onclose={() => {
		purgeVersionTarget = null;
	}}
>
	{#if purgeVersionTarget}
		<p class="text-sm text-[var(--text-secondary)]">
			Remove version <span class="font-mono">v{purgeVersionTarget.version}</span> row
			<span class="font-mono text-xs">{purgeVersionTarget.id}</span> from the database? This cannot be undone.
		</p>
		<div class="mt-6 flex justify-end gap-2">
			<Button variant="outline" onclick={() => (showPurgeVersionDialog = false)}>Cancel</Button>
			<Button
				variant="primary"
				class="bg-red-600 hover:bg-red-700"
				onclick={submitPurgeSecretVersion}
				loading={secretActionLoading}
			>
				Purge permanently
			</Button>
		</div>
	{/if}
</Dialog>

<Dialog bind:open={showCreateVariable} title="Add environment variable">
	<div class="space-y-4">
		<div>
			<label class="mb-1 block text-sm font-medium" for="v-name">Name</label>
			<Input id="v-name" bind:value={cvName} placeholder="e.g. NODE_VERSION" />
		</div>
		<div>
			<label class="mb-1 block text-sm font-medium" for="v-val">Value</label>
			<Input id="v-val" bind:value={cvValue} />
		</div>
		<label class="flex items-center gap-2 text-sm">
			<input type="checkbox" bind:checked={cvSensitive} class="rounded border-[var(--border-primary)]" />
			Mask value in API responses (sensitive)
		</label>
		<div>
			<label class="mb-1 block text-sm font-medium" for="v-scope">Scope</label>
			<Select id="v-scope" options={pipelineScopeOptions} bind:value={cvPipelineId} />
		</div>
		<div class="flex justify-end gap-2 pt-2">
			<Button variant="outline" onclick={() => (showCreateVariable = false)}>Cancel</Button>
			<Button
				variant="primary"
				onclick={submitCreateVariable}
				loading={variableActionLoading}
				disabled={!cvName.trim()}
			>
				Save
			</Button>
		</div>
	</div>
</Dialog>

<Dialog
	bind:open={showEditVariableDialog}
	title="Edit variable"
	onclose={() => {
		editVariableTarget = null;
	}}
>
	{#if editVariableTarget}
		<div class="space-y-4">
			<div>
				<label class="mb-1 block text-sm font-medium" for="ev-name">Name</label>
				<Input id="ev-name" bind:value={evName} />
			</div>
			<div>
				<label class="mb-1 block text-sm font-medium" for="ev-val">New value</label>
				<Input
					id="ev-val"
					bind:value={evValue}
					placeholder={editVariableTarget.is_sensitive ? 'Leave blank to keep current value' : ''}
				/>
			</div>
			<label class="flex items-center gap-2 text-sm">
				<input type="checkbox" bind:checked={evSensitive} class="rounded border-[var(--border-primary)]" />
				Mask value in API responses
			</label>
			<div class="flex justify-end gap-2 pt-2">
				<Button variant="outline" onclick={() => (showEditVariableDialog = false)}>Cancel</Button>
				<Button variant="primary" onclick={submitEditVariable} loading={variableActionLoading}>
					Save
				</Button>
			</div>
		</div>
	{/if}
</Dialog>

<Dialog
	bind:open={showDeleteVariableDialog}
	title="Delete variable?"
	onclose={() => {
		deleteVariableTarget = null;
	}}
>
	{#if deleteVariableTarget}
		<p class="text-sm text-[var(--text-secondary)]">
			Delete <span class="font-mono">{deleteVariableTarget.name}</span>? Running pipelines keep already-loaded values
			until the next run.
		</p>
		<div class="mt-6 flex justify-end gap-2">
			<Button variant="outline" onclick={() => (showDeleteVariableDialog = false)}>Cancel</Button>
			<Button
				variant="primary"
				class="bg-red-600 hover:bg-red-700"
				onclick={submitDeleteVariable}
				loading={variableActionLoading}
			>
				Delete
			</Button>
		</div>
	{/if}
</Dialog>
