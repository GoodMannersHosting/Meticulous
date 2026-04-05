<script lang="ts">
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { Button, Card, Badge, Tabs, Dialog, Input, Alert, Select } from '$components/ui';
	import { DataTable, EmptyState, Skeleton } from '$components/data';
	import { apiMethods } from '$api/client';
	import type { Project, Pipeline, StoredSecret } from '$api/types';
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
		RefreshCw
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

	const kindOptions = [
		{ value: 'kv', label: 'Key / value (kv)' },
		{ value: 'api_key', label: 'API key' },
		{ value: 'ssh_private_key', label: 'SSH private key (PEM)' },
		{ value: 'github_app', label: 'GitHub App' },
		{ value: 'x509_bundle', label: 'X.509 bundle (JSON)' }
	];

	const tabs = [
		{ id: 'pipelines', label: 'Pipelines', icon: GitBranch },
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
				value = JSON.stringify({
					app_id,
					installation_id,
					private_key_pem: ghPrivateKey.trim(),
					...(ghApiBase.trim() ? { github_api_base: ghApiBase.trim() } : {})
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

	const pipelineScopeOptions = $derived([
		{ value: '', label: 'Project-wide (all pipelines)' },
		...pipelines.map((p) => ({ value: p.id, label: p.name }))
	]);
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
									<td class="px-4 py-3 font-mono">v{s.version}</td>
									<td class="px-4 py-3 text-[var(--text-secondary)]">
										{formatRelativeTime(s.updated_at)}
									</td>
									<td class="px-4 py-3 text-right">
										<div class="flex justify-end gap-2">
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
		<div class="mt-4">
			<textarea
				bind:value={rotateValue}
				rows="4"
				class="w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm text-[var(--text-primary)] focus:outline-none focus:ring-2 focus:ring-primary-500"
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
				disabled={!rotateValue}
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
