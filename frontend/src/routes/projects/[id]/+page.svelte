<script lang="ts">
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { Button, Card, Badge, Tabs, Dialog, Input, Alert } from '$components/ui';
	import { DataTable, EmptyState, Skeleton } from '$components/data';
	import { StatusBadge } from '$components/ui';
	import { apiMethods } from '$api/client';
	import type { Project, Pipeline, Run } from '$api/types';
	import { formatRelativeTime, formatDurationMs } from '$utils/format';
	import {
		ArrowLeft,
		Plus,
		GitBranch,
		Play,
		Settings,
		Trash2,
		Edit,
		MoreVertical
	} from 'lucide-svelte';
	import type { Column } from '$components/data/DataTable.svelte';

	let { data } = $props();

	let project = $state<Project | null>(null);
	let pipelines = $state<Pipeline[]>([]);
	let recentRuns = $state<Run[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let activeTab = $state('pipelines');

	const tabs = [
		{ id: 'pipelines', label: 'Pipelines', icon: GitBranch },
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
