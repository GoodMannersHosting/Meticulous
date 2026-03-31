<script lang="ts">
	import { Button, Card, Input, Select, Badge, StatusBadge } from '$components/ui';
	import { Skeleton, EmptyState } from '$components/data';
	import { apiMethods } from '$api/client';
	import type { Run, Pipeline, Project } from '$api/types';
	import { formatRelativeTime, formatDurationMs } from '$utils/format';
	import { Search, RefreshCw, Play, Filter } from 'lucide-svelte';
	import { goto } from '$app/navigation';

	let runs = $state<Run[]>([]);
	let pipelines = $state<Pipeline[]>([]);
	let projects = $state<Project[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let selectedProjectId = $state<string>('');
	let selectedPipelineId = $state<string>('');
	let statusFilter = $state<string>('');

	const statusOptions = [
		{ value: '', label: 'All Statuses' },
		{ value: 'running', label: 'Running' },
		{ value: 'succeeded', label: 'Succeeded' },
		{ value: 'failed', label: 'Failed' },
		{ value: 'pending', label: 'Pending' },
		{ value: 'cancelled', label: 'Cancelled' }
	];

	$effect(() => {
		loadInitialData();
	});

	async function loadInitialData() {
		loading = true;
		try {
			const projectsResponse = await apiMethods.projects.list();
			projects = projectsResponse.data;
			
			if (projects.length > 0) {
				selectedProjectId = projects[0].id;
				const pipelinesResponse = await apiMethods.pipelines.list({ project_id: selectedProjectId });
				pipelines = pipelinesResponse.data;
				
				if (pipelines.length > 0) {
					selectedPipelineId = pipelines[0].id;
					await loadRuns();
				}
			}
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load data';
		} finally {
			loading = false;
		}
	}

	async function loadPipelines() {
		if (!selectedProjectId) {
			pipelines = [];
			return;
		}
		try {
			const response = await apiMethods.pipelines.list({ project_id: selectedProjectId });
			pipelines = response.data;
			if (pipelines.length > 0) {
				selectedPipelineId = pipelines[0].id;
				await loadRuns();
			} else {
				selectedPipelineId = '';
				runs = [];
			}
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load pipelines';
		}
	}

	async function loadRuns() {
		if (!selectedPipelineId) {
			runs = [];
			return;
		}
		loading = true;
		try {
			const response = await apiMethods.runs.list({
				pipeline_id: selectedPipelineId,
				status: statusFilter || undefined
			});
			runs = response.data;
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load runs';
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		if (selectedProjectId) {
			loadPipelines();
		}
	});

	$effect(() => {
		if (selectedPipelineId) {
			loadRuns();
		}
	});

	function handleRunClick(run: Run) {
		goto(`/runs/${run.id}`);
	}

	const projectOptions = $derived(
		projects.map((p) => ({ value: p.id, label: p.name }))
	);

	const pipelineOptions = $derived(
		pipelines.map((p) => ({ value: p.id, label: p.name }))
	);
</script>

<svelte:head>
	<title>Runs | Meticulous</title>
</svelte:head>

<div class="space-y-6">
	<div class="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
		<div>
			<h1 class="text-2xl font-bold text-[var(--text-primary)]">Pipeline Runs</h1>
			<p class="mt-1 text-[var(--text-secondary)]">
				View and monitor your pipeline executions.
			</p>
		</div>

		<Button variant="ghost" size="sm" onclick={loadRuns}>
			<RefreshCw class="h-4 w-4" />
			Refresh
		</Button>
	</div>

	<div class="flex flex-wrap gap-4">
		<div class="w-48">
			<Select
				options={projectOptions}
				bind:value={selectedProjectId}
				placeholder="Select project..."
			/>
		</div>
		<div class="w-48">
			<Select
				options={pipelineOptions}
				bind:value={selectedPipelineId}
				placeholder="Select pipeline..."
			/>
		</div>
		<div class="w-40">
			<Select
				options={statusOptions}
				bind:value={statusFilter}
				placeholder="Status"
			/>
		</div>
	</div>

	{#if error}
		<div class="rounded-lg border border-error-200 bg-error-50 p-4 text-sm text-error-700 dark:border-error-800 dark:bg-error-900/20 dark:text-error-400">
			{error}
		</div>
	{/if}

	{#if loading}
		<Card>
			<div class="space-y-4">
				{#each Array(8) as _, i (i)}
					<div class="flex items-center gap-4">
						<Skeleton class="h-5 w-16" />
						<Skeleton class="h-6 w-24 rounded-full" />
						<Skeleton class="h-5 w-24" />
						<div class="flex-1"></div>
						<Skeleton class="h-5 w-16" />
						<Skeleton class="h-5 w-24" />
					</div>
				{/each}
			</div>
		</Card>
	{:else if !selectedPipelineId}
		<Card>
			<EmptyState
				title="Select a pipeline"
				description="Choose a project and pipeline to view runs."
			/>
		</Card>
	{:else if runs.length === 0}
		<Card>
			<EmptyState
				title="No runs yet"
				description="This pipeline hasn't been triggered yet."
			>
				<Button variant="primary" href="/pipelines/{selectedPipelineId}">
					<Play class="h-4 w-4" />
					Go to Pipeline
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
							<td class="px-4 py-3">
								<span class="font-mono text-sm">#{run.run_number}</span>
							</td>
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
							<td class="px-4 py-3 text-sm text-[var(--text-secondary)]">
								{formatRelativeTime(run.created_at)}
							</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
	{/if}
</div>
