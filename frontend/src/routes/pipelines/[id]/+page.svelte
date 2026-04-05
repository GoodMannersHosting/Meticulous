<script lang="ts">
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { Button, Card, Badge, Tabs, Dialog, Alert, StatusBadge, CopyButton } from '$components/ui';
	import { DataTable, EmptyState, Skeleton } from '$components/data';
	import { apiMethods } from '$api/client';
	import type { Pipeline, PipelineJob, Run, StoredSecret } from '$api/types';
	import { formatRelativeTime, formatDurationMs, truncateId } from '$utils/format';
	import {
		ArrowLeft,
		Play,
		Settings,
		Edit,
		Clock,
		GitCommit,
		User,
		MoreVertical,
		RefreshCw,
		Pause,
		Trash2,
		ExternalLink,
		KeyRound
	} from 'lucide-svelte';
	import type { Column } from '$components/data/DataTable.svelte';
	import DagViewer from '$components/pipeline/DagViewer.svelte';

	let pipeline = $state<Pipeline | null>(null);
	let runs = $state<Run[]>([]);
	let loading = $state(true);
	let runsLoading = $state(false);
	let error = $state<string | null>(null);
	let activeTab = $state('runs');
	let triggerLoading = $state(false);
	let syncGitLoading = $state(false);
	let pipelineSecrets = $state<StoredSecret[]>([]);
	let secretsLoading = $state(false);
	let secretsError = $state<string | null>(null);

	function definitionJobs(def: Pipeline['definition']): PipelineJob[] {
		if (def && typeof def === 'object' && 'jobs' in def) {
			const j = (def as { jobs: unknown }).jobs;
			if (Array.isArray(j)) return j as PipelineJob[];
		}
		return [];
	}

	const tabs = [
		{ id: 'runs', label: 'Runs', icon: Play },
		{ id: 'secrets', label: 'Secrets', icon: KeyRound },
		{ id: 'definition', label: 'Definition', icon: Settings }
	];

	$effect(() => {
		loadPipeline();
	});

	async function loadPipeline() {
		loading = true;
		error = null;
		try {
			const pipelineId = $page.params.id!;
			pipeline = await apiMethods.pipelines.get(pipelineId);
			await loadRuns();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load pipeline';
		} finally {
			loading = false;
		}
	}

	async function loadRuns() {
		if (!pipeline) return;
		runsLoading = true;
		try {
			const response = await apiMethods.runs.list({ pipeline_id: pipeline.id });
			runs = response.data;
		} catch (e) {
			console.error('Failed to load runs:', e);
		} finally {
			runsLoading = false;
		}
	}

	async function loadPipelineSecrets() {
		if (!pipeline) return;
		secretsLoading = true;
		secretsError = null;
		try {
			pipelineSecrets = await apiMethods.storedSecrets.list(pipeline.project_id, {
				pipeline_id: pipeline.id
			});
		} catch (e) {
			secretsError = e instanceof Error ? e.message : 'Failed to load secrets';
			pipelineSecrets = [];
		} finally {
			secretsLoading = false;
		}
	}

	$effect(() => {
		if (activeTab !== 'secrets' || !pipeline || loading) return;
		void loadPipelineSecrets();
	});

	async function triggerPipeline() {
		if (!pipeline) return;
		triggerLoading = true;
		try {
			const result = await apiMethods.pipelines.trigger(pipeline.id);
			goto(`/runs/${result.run_id}`);
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to trigger pipeline';
		} finally {
			triggerLoading = false;
		}
	}

	async function syncFromGit() {
		if (!pipeline) return;
		syncGitLoading = true;
		error = null;
		try {
			const updated = await apiMethods.pipelines.syncFromGit(pipeline.id, {});
			pipeline = updated;
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to sync from Git';
		} finally {
			syncGitLoading = false;
		}
	}

	const runColumns: Column<Run>[] = [
		{
			key: 'run_number',
			label: 'Run',
			width: '100px',
			render: (value) => `<span class="font-mono text-sm">#${value}</span>`
		},
		{
			key: 'status',
			label: 'Status',
			width: '140px'
		},
		{
			key: 'branch',
			label: 'Branch',
			render: (value, row) => {
				if (!value && !row.commit_sha) return '<span class="text-[var(--text-tertiary)]">—</span>';
				let html = '';
				if (value) html += `<span class="text-sm">${value}</span>`;
				if (row.commit_sha) {
					html += `<span class="ml-2 font-mono text-xs text-[var(--text-tertiary)]">${(row.commit_sha as string).slice(0, 7)}</span>`;
				}
				return html;
			}
		},
		{
			key: 'triggered_by',
			label: 'Triggered By',
			render: (value) => `<span class="text-sm">${value}</span>`
		},
		{
			key: 'duration_ms',
			label: 'Duration',
			render: (value) => formatDurationMs(value as number)
		},
		{
			key: 'created_at',
			label: 'Started',
			render: (value) => formatRelativeTime(value as string)
		}
	];

	function handleRunClick(run: Run) {
		goto(`/runs/${run.id}`);
	}

	function renderStatusCell(run: Run) {
		return run.status;
	}
</script>

<svelte:head>
	<title>{pipeline?.name ?? 'Pipeline'} | Meticulous</title>
</svelte:head>

<div class="space-y-6">
	<div class="flex items-start gap-4">
		<Button variant="ghost" size="sm" href="/pipelines">
			<ArrowLeft class="h-4 w-4" />
		</Button>

		{#if loading}
			<div class="flex-1 space-y-2">
				<Skeleton class="h-7 w-48" />
				<Skeleton class="h-4 w-32" />
			</div>
		{:else if pipeline}
			<div class="flex-1">
				<div class="flex items-center gap-3">
					<h1 class="text-2xl font-bold text-[var(--text-primary)]">{pipeline.name}</h1>
					{#if pipeline.enabled}
						<Badge variant="success" size="sm">Active</Badge>
					{:else}
						<Badge variant="secondary" size="sm">Disabled</Badge>
					{/if}
				</div>
				{#if pipeline.description}
					<p class="mt-1 text-[var(--text-secondary)]">{pipeline.description}</p>
				{/if}
				<div class="mt-2 flex items-center gap-4 text-sm text-[var(--text-tertiary)]">
					<span class="flex items-center gap-1">
						<Clock class="h-4 w-4" />
						Updated {formatRelativeTime(pipeline.updated_at)}
					</span>
					<span class="flex items-center gap-1 font-mono">
						{truncateId(pipeline.id)}
						<CopyButton text={pipeline.id} size="sm" />
					</span>
				</div>
			</div>

			<div class="flex flex-wrap items-center gap-2">
				{#if pipeline.scm_provider === 'github'}
					<Button variant="outline" size="sm" onclick={syncFromGit} loading={syncGitLoading}>
						<GitCommit class="h-4 w-4" />
						Sync from Git
					</Button>
				{/if}
				<Button variant="outline" size="sm">
					<Edit class="h-4 w-4" />
					Edit
				</Button>
				<Button variant="primary" onclick={triggerPipeline} loading={triggerLoading}>
					<Play class="h-4 w-4" />
					Run Pipeline
				</Button>
			</div>
		{/if}
	</div>

	{#if error}
		<Alert variant="error" title="Error" dismissible ondismiss={() => (error = null)}>
			{error}
		</Alert>
	{/if}

	{#if !loading && pipeline}
		<Tabs items={tabs} bind:value={activeTab} />

		{#if activeTab === 'runs'}
			<div class="flex items-center justify-between">
				<p class="text-sm text-[var(--text-secondary)]">
					{runs.length} {runs.length === 1 ? 'run' : 'runs'}
				</p>
				<Button variant="ghost" size="sm" onclick={loadRuns} loading={runsLoading}>
					<RefreshCw class="h-4 w-4" />
					Refresh
				</Button>
			</div>

			{#if runsLoading && runs.length === 0}
				<Card>
					<div class="space-y-4">
						{#each Array(5) as _, i (i)}
							<div class="flex items-center gap-4">
								<Skeleton class="h-5 w-16" />
								<Skeleton class="h-6 w-24 rounded-full" />
								<Skeleton class="h-5 w-32" />
								<div class="flex-1"></div>
								<Skeleton class="h-5 w-20" />
								<Skeleton class="h-5 w-24" />
							</div>
						{/each}
					</div>
				</Card>
			{:else if runs.length === 0}
				<Card>
					<EmptyState
						title="No runs yet"
						description="Trigger this pipeline to start your first run."
					>
						<Button variant="primary" onclick={triggerPipeline} loading={triggerLoading}>
							<Play class="h-4 w-4" />
							Run Pipeline
						</Button>
					</EmptyState>
				</Card>
			{:else}
				<div class="overflow-hidden rounded-lg border border-[var(--border-primary)]">
					<table class="w-full text-sm">
						<thead class="bg-[var(--bg-tertiary)]">
							<tr>
								<th class="px-4 py-3 text-left font-medium text-[var(--text-secondary)]">Run</th>
								<th class="px-4 py-3 text-left font-medium text-[var(--text-secondary)]">Status</th>
								<th class="px-4 py-3 text-left font-medium text-[var(--text-secondary)]">Branch</th>
								<th class="px-4 py-3 text-left font-medium text-[var(--text-secondary)]">Triggered By</th>
								<th class="px-4 py-3 text-left font-medium text-[var(--text-secondary)]">Duration</th>
								<th class="px-4 py-3 text-left font-medium text-[var(--text-secondary)]">Started</th>
							</tr>
						</thead>
						<tbody class="divide-y divide-[var(--border-secondary)]">
							{#each runs as run (run.id)}
								<tr
									class="cursor-pointer bg-[var(--bg-secondary)] transition-colors hover:bg-[var(--bg-hover)]"
									onclick={() => handleRunClick(run)}
								>
									<td class="px-4 py-3 font-mono text-sm">#{run.run_number}</td>
									<td class="px-4 py-3">
										<StatusBadge status={run.status} size="sm" />
									</td>
									<td class="px-4 py-3">
										{#if run.branch || run.commit_sha}
											<span class="text-sm">{run.branch ?? ''}</span>
											{#if run.commit_sha}
												<span class="ml-2 font-mono text-xs text-[var(--text-tertiary)]">
													{run.commit_sha.slice(0, 7)}
												</span>
											{/if}
										{:else}
											<span class="text-[var(--text-tertiary)]">—</span>
										{/if}
									</td>
									<td class="px-4 py-3 text-sm">{run.triggered_by}</td>
									<td class="px-4 py-3 text-sm">{formatDurationMs(run.duration_ms)}</td>
									<td class="px-4 py-3 text-sm text-[var(--text-secondary)]">{formatRelativeTime(run.created_at)}</td>
								</tr>
							{/each}
						</tbody>
					</table>
				</div>
			{/if}
		{:else if activeTab === 'secrets'}
			<div class="flex flex-wrap items-center justify-between gap-3">
				<p class="text-sm text-[var(--text-secondary)]">
					Secrets scoped to this pipeline. Create project-wide or other pipeline secrets from the project
					page.
				</p>
				<div class="flex gap-2">
					<Button variant="outline" size="sm" href="/projects/{pipeline.project_id}">
						Project secrets
					</Button>
					<Button variant="ghost" size="sm" onclick={loadPipelineSecrets} loading={secretsLoading}>
						<RefreshCw class="h-4 w-4" />
						Refresh
					</Button>
				</div>
			</div>
			{#if secretsError}
				<Alert variant="error" title="Secrets" dismissible ondismiss={() => (secretsError = null)}>
					{secretsError}
				</Alert>
			{/if}
			{#if secretsLoading && pipelineSecrets.length === 0}
				<Card>
					<div class="space-y-3 p-4">
						{#each Array(3) as _, i (i)}
							<Skeleton class="h-10 w-full" />
						{/each}
					</div>
				</Card>
			{:else if pipelineSecrets.length === 0}
				<Card>
					<EmptyState
						title="No pipeline-scoped secrets"
						description={`Add a secret on the project page and choose this pipeline as the scope, or use project-wide secrets with stored: { name: ... } in YAML.`}
					>
						<Button variant="primary" href="/projects/{pipeline.project_id}">
							Open project
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
								<th class="px-4 py-3 text-left font-medium text-[var(--text-secondary)]">Version</th>
								<th class="px-4 py-3 text-left font-medium text-[var(--text-secondary)]">Updated</th>
							</tr>
						</thead>
						<tbody class="divide-y divide-[var(--border-secondary)]">
							{#each pipelineSecrets as s (s.id)}
								<tr class="bg-[var(--bg-secondary)]">
									<td class="px-4 py-3 font-mono text-sm">{s.path}</td>
									<td class="px-4 py-3">{s.kind}</td>
									<td class="px-4 py-3 font-mono">v{s.version}</td>
									<td class="px-4 py-3 text-[var(--text-secondary)]">
										{formatRelativeTime(s.updated_at)}
									</td>
								</tr>
							{/each}
						</tbody>
					</table>
				</div>
			{/if}
		{:else if activeTab === 'definition'}
			<Card>
				<div class="space-y-4">
					<div class="flex items-center justify-between">
						<h3 class="font-medium text-[var(--text-primary)]">Pipeline Definition</h3>
						{#if pipeline.definition_path}
							<span class="text-sm text-[var(--text-secondary)]">
								From: <code class="font-mono">{pipeline.definition_path}</code>
							</span>
						{/if}
					</div>

					<pre class="overflow-x-auto rounded-lg bg-[var(--bg-tertiary)] p-4 text-sm"><code>{JSON.stringify(pipeline.definition, null, 2)}</code></pre>
				</div>
			</Card>

			{#if definitionJobs(pipeline.definition).length > 0}
				<Card>
					<h3 class="mb-4 font-medium text-[var(--text-primary)]">Job Graph</h3>
					<DagViewer jobs={definitionJobs(pipeline.definition)} />
				</Card>
			{/if}
		{/if}
	{/if}
</div>
