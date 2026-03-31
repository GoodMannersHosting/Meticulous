<script lang="ts">
	import { Button, Card, Input, Select, Badge } from '$components/ui';
	import { DataTable, EmptyState, Skeleton } from '$components/data';
	import { StatusBadge } from '$components/ui';
	import { apiMethods } from '$api/client';
	import type { Pipeline, Project } from '$api/types';
	import { formatRelativeTime } from '$utils/format';
	import { Plus, Search, GitBranch, Filter } from 'lucide-svelte';
	import type { Column } from '$components/data/DataTable.svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/stores';

	let pipelines = $state<Pipeline[]>([]);
	let projects = $state<Project[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let searchQuery = $state('');
	let selectedProjectId = $state<string>('');

	$effect(() => {
		const projectId = $page.url.searchParams.get('project');
		if (projectId) {
			selectedProjectId = projectId;
		}
		loadData();
	});

	async function loadData() {
		loading = true;
		error = null;
		try {
			const projectsResponse = await apiMethods.projects.list();
			projects = projectsResponse.items;

			if (selectedProjectId) {
				const pipelinesResponse = await apiMethods.pipelines.list({ project_id: selectedProjectId });
				pipelines = pipelinesResponse.items;
			} else if (projects.length > 0) {
				selectedProjectId = projects[0].id;
				const pipelinesResponse = await apiMethods.pipelines.list({ project_id: selectedProjectId });
				pipelines = pipelinesResponse.items;
			} else {
				pipelines = [];
			}
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load pipelines';
			pipelines = [];
		} finally {
			loading = false;
		}
	}

	async function loadPipelines() {
		if (!selectedProjectId) {
			pipelines = [];
			return;
		}
		loading = true;
		try {
			const response = await apiMethods.pipelines.list({ project_id: selectedProjectId });
			pipelines = response.items;
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load pipelines';
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		if (selectedProjectId) {
			loadPipelines();
		}
	});

	const columns: Column<Pipeline>[] = [
		{
			key: 'name',
			label: 'Pipeline',
			sortable: true,
			render: (_, row) => `
				<div class="flex items-center gap-3">
					<div class="flex h-10 w-10 items-center justify-center rounded-lg bg-[var(--bg-tertiary)]">
						<svg class="h-5 w-5 text-[var(--text-secondary)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
							<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z"/>
						</svg>
					</div>
					<div>
						<div class="font-medium text-[var(--text-primary)]">${row.name}</div>
						<div class="text-sm text-[var(--text-secondary)]">${row.slug}</div>
					</div>
				</div>
			`
		},
		{
			key: 'description',
			label: 'Description',
			render: (value) => (value as string) || '<span class="text-[var(--text-tertiary)]">—</span>'
		},
		{
			key: 'enabled',
			label: 'Status',
			render: (value) =>
				value
					? '<span class="inline-flex items-center gap-1.5 text-sm"><span class="h-2 w-2 rounded-full bg-success-500"></span>Active</span>'
					: '<span class="inline-flex items-center gap-1.5 text-sm text-secondary-500"><span class="h-2 w-2 rounded-full bg-secondary-400"></span>Disabled</span>'
		},
		{
			key: 'updated_at',
			label: 'Last Updated',
			sortable: true,
			render: (value) => formatRelativeTime(value as string)
		}
	];

	function handleRowClick(pipeline: Pipeline) {
		goto(`/pipelines/${pipeline.id}`);
	}

	const projectOptions = $derived(
		projects.map((p) => ({ value: p.id, label: p.name }))
	);
</script>

<svelte:head>
	<title>Pipelines | Meticulous</title>
</svelte:head>

<div class="space-y-6">
	<div class="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
		<div>
			<h1 class="text-2xl font-bold text-[var(--text-primary)]">Pipelines</h1>
			<p class="mt-1 text-[var(--text-secondary)]">
				Define and manage your CI/CD workflows.
			</p>
		</div>

		<Button variant="primary" href="/pipelines/new{selectedProjectId ? `?project=${selectedProjectId}` : ''}">
			<Plus class="h-4 w-4" />
			New Pipeline
		</Button>
	</div>

	<div class="flex flex-wrap gap-4">
		<div class="w-64">
			<Select
				options={projectOptions}
				bind:value={selectedProjectId}
				placeholder="Select project..."
			/>
		</div>
		<div class="relative flex-1 max-w-md">
			<Search class="absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-[var(--text-tertiary)]" />
			<Input
				type="search"
				placeholder="Search pipelines..."
				class="pl-10"
				bind:value={searchQuery}
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
				{#each Array(5) as _, i (i)}
					<div class="flex items-center gap-4">
						<Skeleton class="h-10 w-10 rounded-lg" />
						<div class="flex-1 space-y-2">
							<Skeleton class="h-4 w-48" />
							<Skeleton class="h-3 w-32" />
						</div>
						<Skeleton class="h-4 w-20" />
						<Skeleton class="h-4 w-24" />
					</div>
				{/each}
			</div>
		</Card>
	{:else if !selectedProjectId}
		<Card>
			<EmptyState
				title="Select a project"
				description="Choose a project to view its pipelines."
			/>
		</Card>
	{:else if pipelines.length === 0}
		<Card>
			<EmptyState
				title="No pipelines yet"
				description="Create your first pipeline to automate your builds."
			>
				<Button variant="primary" href="/pipelines/new?project={selectedProjectId}">
					<Plus class="h-4 w-4" />
					Create Pipeline
				</Button>
			</EmptyState>
		</Card>
	{:else}
		<DataTable
			{columns}
			data={pipelines}
			rowKey="id"
			onRowClick={handleRowClick}
		/>
	{/if}
</div>
