<script lang="ts">
	import { Button, Card, Select } from '$components/ui';
	import { DataTable, Skeleton, EmptyState } from '$components/data';
	import { apiMethods } from '$api/client';
	import type { Run, Pipeline, Project } from '$api/types';
	import type { Column, SortDirection } from '$components/data/DataTable.svelte';
	import { sortRunList } from '$utils/sortRuns';
	import {
		runNumberHtml,
		runPipelineLinkHtml,
		runProjectLinkHtml,
		runStatusBadgeHtml,
		effectiveRunStatusForBadge,
		runBranchColumnHtml,
		runTriggeredByHtml,
		runDurationHtml,
		runStartedAtHtml
	} from '$utils/runTableCells';
	import { RefreshCw, Play, ChevronLeft, ChevronRight, FolderOpen } from 'lucide-svelte';
	import { goto } from '$app/navigation';

	const ALL_PROJECTS = '__all_projects__';

	/** Sentinel value for “all pipelines in the current project” (must match nothing that is a UUID). */
	const ALL_PIPELINES_IN_PROJECT = '__all_pipelines__';

	let runs = $state<Run[]>([]);
	let pipelines = $state<Pipeline[]>([]);
	let projects = $state<Project[]>([]);
	let initialLoading = $state(true);
	let runsLoading = $state(false);
	let error = $state<string | null>(null);
	let selectedProjectId = $state<string>(ALL_PROJECTS);
	let selectedPipelineId = $state<string>('');
	let statusFilter = $state<string>('');
	let runSortKey = $state<string | null>(null);
	let runSortDirection = $state<SortDirection>(null);
	let runsPerPage = $state('20');
	let runsListOffset = $state(0);
	let runsHasMore = $state(false);

	const runsPageSizeOptions = [
		{ value: '20', label: '20 per page' },
		{ value: '50', label: '50 per page' },
		{ value: '100', label: '100 per page' }
	];

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
		initialLoading = true;
		error = null;
		try {
			const projectsResponse = await apiMethods.projects.list();
			projects = projectsResponse.data;
			selectedProjectId = ALL_PROJECTS;
			selectedPipelineId = ALL_PIPELINES_IN_PROJECT;
			pipelines = [];
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load data';
		} finally {
			initialLoading = false;
		}
	}

	async function loadPipelines() {
		if (!selectedProjectId || selectedProjectId === ALL_PROJECTS) {
			pipelines = [];
			selectedPipelineId = ALL_PIPELINES_IN_PROJECT;
			return;
		}
		selectedPipelineId = ALL_PIPELINES_IN_PROJECT;
		try {
			const response = await apiMethods.pipelines.list({ project_id: selectedProjectId });
			pipelines = response.data;
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load pipelines';
		}
	}

	$effect(() => {
		if (!selectedProjectId || selectedProjectId === ALL_PROJECTS) {
			pipelines = [];
			selectedPipelineId = ALL_PIPELINES_IN_PROJECT;
			return;
		}
		void loadPipelines();
	});

	$effect(() => {
		const pipelineSelection = selectedPipelineId;
		void statusFilter;
		void selectedProjectId;
		if (!pipelineSelection) {
			runs = [];
			runsHasMore = false;
			return;
		}
		runsListOffset = 0;
		void fetchRuns({ offset: 0 });
	});

	$effect(() => {
		if (selectedPipelineId !== ALL_PIPELINES_IN_PROJECT && runSortKey === 'pipeline_name') {
			runSortKey = null;
			runSortDirection = null;
		}
		if (!viewingAllProjects() && runSortKey === 'project_name') {
			runSortKey = null;
			runSortDirection = null;
		}
	});

	function viewingAllPipelinesInProject(): boolean {
		return selectedPipelineId === ALL_PIPELINES_IN_PROJECT;
	}

	function viewingAllProjects(): boolean {
		return selectedProjectId === ALL_PROJECTS;
	}

	async function fetchRuns(opts?: { offset?: number }) {
		if (!selectedPipelineId) {
			runs = [];
			return;
		}
		const offset = opts?.offset ?? runsListOffset;
		runsLoading = true;
		error = null;
		try {
			const listParams =
				selectedProjectId === ALL_PROJECTS
					? {
							status: statusFilter || undefined,
							per_page: Number(runsPerPage),
							cursor: offset > 0 ? String(offset) : undefined
						}
					: viewingAllPipelinesInProject()
						? {
								project_id: selectedProjectId,
								status: statusFilter || undefined,
								per_page: Number(runsPerPage),
								cursor: offset > 0 ? String(offset) : undefined
							}
						: {
								pipeline_id: selectedPipelineId,
								status: statusFilter || undefined,
								per_page: Number(runsPerPage),
								cursor: offset > 0 ? String(offset) : undefined
							};
			const response = await apiMethods.runs.list(listParams);
			runs = response.data;
			runsHasMore = response.pagination.has_more;
			runsListOffset = offset;
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load runs';
		} finally {
			runsLoading = false;
		}
	}

	function handleRunsPerPageChange() {
		runsListOffset = 0;
		void fetchRuns({ offset: 0 });
	}

	function runsPrevPage() {
		const step = Number(runsPerPage);
		if (runsListOffset <= 0) return;
		void fetchRuns({ offset: Math.max(0, runsListOffset - step) });
	}

	function runsNextPage() {
		if (!runsHasMore) return;
		void fetchRuns({ offset: runsListOffset + runs.length });
	}

	function handleRunClick(run: Run) {
		goto(`/runs/${run.id}`);
	}

	const projectOptions = $derived([
		{ value: ALL_PROJECTS, label: 'All Projects' },
		...projects.map((p) => ({ value: p.id, label: p.name }))
	]);

	const pipelineOptions = $derived(
		viewingAllProjects()
			? [{ value: ALL_PIPELINES_IN_PROJECT, label: 'All pipelines' }]
			: [
					{ value: ALL_PIPELINES_IN_PROJECT, label: 'All pipelines in project' },
					...pipelines.map((p) => ({ value: p.id, label: p.name }))
				]
	);

	const sortedRuns = $derived(sortRunList(runs, runSortKey, runSortDirection));

	const runsPageLabel = $derived.by(() => {
		if (runs.length === 0) return null;
		const from = runsListOffset + 1;
		const to = runsListOffset + runs.length;
		return `${from}–${to}`;
	});

	const runColumns = $derived.by((): Column<Run>[] => {
		const projectCol: Column<Run> = {
			key: 'project_name',
			label: 'Project',
			width: '160px',
			sortable: true,
			render: (v, row) => runProjectLinkHtml(v, row)
		};
		const pipelineCol: Column<Run> = {
			key: 'pipeline_name',
			label: 'Pipeline',
			width: '200px',
			sortable: true,
			render: (v, row) => runPipelineLinkHtml(v, row)
		};
		const cols: Column<Run>[] = [
			{
				key: 'run_number',
				label: 'Run',
				width: '100px',
				sortable: true,
				render: (value, row) => runNumberHtml(value, row)
			},
			...(viewingAllProjects() ? [projectCol] : []),
			...(viewingAllPipelinesInProject() ? [pipelineCol] : []),
			{
				key: 'status',
				label: 'Status',
				width: '140px',
				sortable: true,
				render: (_v, row) => runStatusBadgeHtml(effectiveRunStatusForBadge(row))
			},
			{
				key: 'branch',
				label: 'Branch',
				sortable: true,
				render: (value, row) => runBranchColumnHtml(value, row)
			},
			{
				key: 'triggered_by',
				label: 'Triggered By',
				sortable: true,
				render: (value, row) => runTriggeredByHtml(value, row)
			},
			{
				key: 'duration_ms',
				label: 'Duration',
				sortable: true,
				render: (value) => runDurationHtml(value)
			},
			{
				key: 'created_at',
				label: 'Started',
				sortable: true,
				render: (_value, row) => runStartedAtHtml(_value, row)
			}
		];
		return cols;
	});

	function handleRunSort(key: string, direction: SortDirection) {
		if (direction === null) {
			runSortKey = null;
			runSortDirection = null;
		} else {
			runSortKey = key;
			runSortDirection = direction;
		}
	}
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

		<Button
			variant="ghost"
			size="sm"
			onclick={() => fetchRuns({ offset: runsListOffset })}
			disabled={!selectedPipelineId || runsLoading}
		>
			<RefreshCw class="h-4 w-4" />
			Refresh
		</Button>
	</div>

	<div class="flex flex-wrap gap-4">
		<div class="w-56 min-w-[12rem]">
			<Select
				options={projectOptions}
				bind:value={selectedProjectId}
				placeholder="Select project..."
				searchable
				searchPlaceholder="Search projects…"
			/>
		</div>
		<div class="w-48">
			<Select
				options={pipelineOptions}
				bind:value={selectedPipelineId}
				placeholder="Select pipeline..."
				disabled={viewingAllProjects()}
			/>
		</div>
		<div class="w-40">
			<Select options={statusOptions} bind:value={statusFilter} placeholder="Status" />
		</div>
		<div class="w-36">
			<Select
				options={runsPageSizeOptions}
				bind:value={runsPerPage}
				size="sm"
				onchange={handleRunsPerPageChange}
			/>
		</div>
	</div>

	{#if error}
		<div class="rounded-lg border border-error-200 bg-error-50 p-4 text-sm text-error-700 dark:border-error-800 dark:bg-error-900/20 dark:text-error-400">
			{error}
		</div>
	{/if}

	{#if initialLoading}
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
	{:else if runs.length === 0 && runsListOffset === 0 && !runsLoading}
		<Card>
			<EmptyState
				title="No runs yet"
				description={viewingAllProjects()
					? 'No runs recorded yet across your projects.'
					: viewingAllPipelinesInProject()
						? 'This project has no recorded runs yet.'
						: "This pipeline hasn't been triggered yet."}
			>
				{#if viewingAllProjects()}
					<Button variant="primary" href="/projects">
						<FolderOpen class="h-4 w-4" />
						Browse projects
					</Button>
				{:else if viewingAllPipelinesInProject()}
					<Button variant="primary" href="/projects/{selectedProjectId}">
						<FolderOpen class="h-4 w-4" />
						Go to project
					</Button>
				{:else}
					<Button variant="primary" href="/pipelines/{selectedPipelineId}">
						<Play class="h-4 w-4" />
						Go to Pipeline
					</Button>
				{/if}
			</EmptyState>
		</Card>
	{:else}
		<div class="space-y-3">
			<div class="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
				<p class="text-sm text-[var(--text-secondary)]">
					{#if runs.length === 0 && runsListOffset > 0 && !runsLoading}
						No runs on this page — try previous page
					{:else if runsPageLabel}
						Showing runs {runsPageLabel}
						{#if runsHasMore}
							<span class="text-[var(--text-tertiary)]"> (more available)</span>
						{/if}
					{:else if runsLoading}
						Loading…
					{:else}
						—
					{/if}
				</p>
				<div class="flex items-center gap-1">
					<Button
						variant="outline"
						size="sm"
						disabled={runsListOffset <= 0 || runsLoading}
						onclick={runsPrevPage}
						title="Previous page"
					>
						<ChevronLeft class="h-4 w-4" />
					</Button>
					<Button
						variant="outline"
						size="sm"
						disabled={!runsHasMore || runsLoading}
						onclick={runsNextPage}
						title="Next page"
					>
						<ChevronRight class="h-4 w-4" />
					</Button>
				</div>
			</div>
			<DataTable
				columns={runColumns}
				data={sortedRuns}
				rowKey="id"
				sortKey={runSortKey}
				sortDirection={runSortDirection}
				onSort={handleRunSort}
				onRowClick={handleRunClick}
				loading={runsLoading && runs.length === 0}
			/>
		</div>
	{/if}
</div>
