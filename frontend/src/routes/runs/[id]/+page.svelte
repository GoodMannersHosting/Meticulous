<script lang="ts">
	import { browser } from '$app/environment';
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { Button, Card, Badge, Tabs, Alert, StatusBadge, CopyButton, Select } from '$components/ui';
	import { Skeleton } from '$components/data';
	import { apiMethods } from '$api/client';
	import type { Run, JobRun, Pipeline, JobAssignment, RunDagResponse } from '$api/types';
	import { formatRelativeTime, formatDurationMs, truncateId, formatDateTime } from '$utils/format';
	import {
		ArrowLeft,
		RefreshCw,
		XCircle,
		RotateCcw,
		Clock,
		User,
		GitBranch,
		GitCommit,
		Terminal,
		ChevronRight,
		Package,
		AlertTriangle
	} from 'lucide-svelte';
	import { DagViewer } from '$components/pipeline';
	import LogViewer from '$components/logs/LogViewer.svelte';
	import { SbomViewer } from '$components/sbom';
	import { RunFootprintViewer } from '$components/blast-radius';
	import type { RunFootprintResponse, SbomApiResponse } from '$api/types';

	let run = $state<Run | null>(null);
	let pipeline = $state<Pipeline | null>(null);
	let jobRuns = $state<JobRun[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let activeTab = $state('jobs');
	let selectedJobRunId = $state<string | null>(null);
	let jobAssignments = $state<JobAssignment[]>([]);
	let assignmentsLoading = $state(false);
	/** 'all' or dispatch attempt number as string (matches `JobAssignment.attempt`). */
	let dispatchFilter = $state<string>('all');
	let cancelLoading = $state(false);
	let retryLoading = $state(false);
	let runDag = $state<RunDagResponse | null>(null);
	let graphDagLoading = $state(false);
	let graphDagError = $state<string | null>(null);
	let sbomRes = $state<SbomApiResponse | null>(null);
	let sbomLoading = $state(false);
	let sbomError = $state<string | null>(null);
	let footprint = $state<RunFootprintResponse | null>(null);
	let footprintLoading = $state(false);
	let footprintError = $state<string | null>(null);

	const tabs = [
		{ id: 'jobs', label: 'Jobs', icon: Terminal },
		{ id: 'graph', label: 'Graph', icon: GitBranch },
		{ id: 'sbom', label: 'SBOM', icon: Package },
		{ id: 'blast-radius', label: 'Blast Radius', icon: AlertTriangle }
	];

	$effect(() => {
		loadRun();
	});

	async function loadAssignmentsForJob(jobRunId: string, opts?: { silent?: boolean }) {
		if (!run) return;
		if (!opts?.silent) assignmentsLoading = true;
		const prev = dispatchFilter;
		try {
			jobAssignments = await apiMethods.runs.assignments(run.id, jobRunId);
			if (jobAssignments.length > 1) {
				const keepPrev =
					opts?.silent &&
					prev !== 'all' &&
					jobAssignments.some((a) => String(a.attempt) === prev);
				dispatchFilter = keepPrev
					? prev
					: String(Math.max(...jobAssignments.map((a) => a.attempt)));
			} else {
				dispatchFilter = 'all';
			}
		} catch {
			if (!opts?.silent) {
				jobAssignments = [];
				dispatchFilter = 'all';
			}
		} finally {
			assignmentsLoading = false;
		}
	}

	async function refreshRunAndJobsQuietly(runId: string) {
		try {
			const r = await apiMethods.runs.get(runId);
			run = r;
			jobRuns = await apiMethods.runs.jobs(runId);
			if (selectedJobRunId && !jobRuns.find((j) => j.id === selectedJobRunId)) {
				selectedJobRunId = jobRuns.length > 0 ? jobRuns[0].id : null;
			} else if (jobRuns.length > 0 && !selectedJobRunId) {
				selectedJobRunId = jobRuns[0].id;
			}
			if (selectedJobRunId) {
				await loadAssignmentsForJob(selectedJobRunId, { silent: true });
			}
		} catch {
			/* keep current data on transient poll failures */
		}
	}

	async function loadRun() {
		loading = true;
		error = null;
		try {
			const runId = $page.params.id!;
			run = await apiMethods.runs.get(runId);
			pipeline = await apiMethods.pipelines.get(run.pipeline_id);
			jobRuns = await apiMethods.runs.jobs(runId);

			if (jobRuns.length === 0) {
				selectedJobRunId = null;
			} else if (
				!selectedJobRunId ||
				!jobRuns.some((j) => j.id === selectedJobRunId)
			) {
				selectedJobRunId = jobRuns[0].id;
			}
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load run';
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		const jobId = selectedJobRunId;
		const r = run;
		if (!jobId || !r || loading) return;
		void loadAssignmentsForJob(jobId);
	});

	async function cancelRun() {
		if (!run) return;
		cancelLoading = true;
		try {
			await apiMethods.runs.cancel(run.id);
			await loadRun();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to cancel run';
		} finally {
			cancelLoading = false;
		}
	}

	async function retryRun() {
		if (!run) return;
		retryLoading = true;
		try {
			const result = await apiMethods.runs.retry(run.id);
			goto(`/runs/${result.new_run_id}`);
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to retry run';
		} finally {
			retryLoading = false;
		}
	}

	const isTerminal = $derived(
		run?.status && ['succeeded', 'failed', 'cancelled', 'timed_out'].includes(run.status)
	);

	$effect(() => {
		if (!browser) return;
		const runId = $page.params.id;
		if (!runId || !run || isTerminal) return;

		const interval = setInterval(() => {
			void refreshRunAndJobsQuietly(runId);
		}, 2500);

		return () => clearInterval(interval);
	});

	const selectedJobRun = $derived(
		jobRuns.find((jr) => jr.id === selectedJobRunId)
	);

	const dispatchSelectOptions = $derived.by(() => {
		const base = [{ value: 'all', label: 'All dispatch attempts' }];
		if (jobAssignments.length <= 1) return base;
		const sorted = [...jobAssignments].sort((a, b) => a.attempt - b.attempt);
		return [
			...base,
			...sorted.map((a) => ({
				value: String(a.attempt),
				label: `Dispatch ${a.attempt} (${a.status} · ${truncateId(a.agent_id)})`
			}))
		];
	});

	const logTimeFilterForViewer = $derived.by(() => {
		if (dispatchFilter === 'all') return null;
		const attempt = Number(dispatchFilter);
		const a = jobAssignments.find((x) => x.attempt === attempt);
		if (!a) return null;
		return {
			startIso: a.started_at ?? a.accepted_at,
			endIso: a.completed_at ?? null
		};
	});

	$effect(() => {
		if (!browser || activeTab !== 'graph' || !run) return;
		const runId = run.id;
		void run.status;
		let cancelled = false;
		graphDagLoading = true;
		graphDagError = null;
		void apiMethods.runs.dag(runId).then(
			(d) => {
				if (!cancelled) {
					runDag = d;
					graphDagLoading = false;
				}
			},
			(e) => {
				if (!cancelled) {
					graphDagError = e instanceof Error ? e.message : 'Failed to load run graph';
					graphDagLoading = false;
				}
			}
		);
		return () => {
			cancelled = true;
		};
	});

	$effect(() => {
		if (!browser || activeTab !== 'sbom' || !run) return;
		void run.id;
		void run.status;
		let cancelled = false;
		sbomLoading = true;
		sbomError = null;
		void apiMethods.artifacts.sbom(run.id).then(
			(d) => {
				if (!cancelled) {
					sbomRes = d;
					sbomLoading = false;
				}
			},
			(e) => {
				if (!cancelled) {
					sbomError = e instanceof Error ? e.message : 'Failed to load SBOM';
					sbomLoading = false;
				}
			}
		);
		return () => {
			cancelled = true;
		};
	});

	$effect(() => {
		if (!browser || activeTab !== 'blast-radius' || !run) return;
		void run.id;
		void run.status;
		let cancelled = false;
		footprintLoading = true;
		footprintError = null;
		void apiMethods.runs.footprint(run.id).then(
			(d) => {
				if (!cancelled) {
					footprint = d;
					footprintLoading = false;
				}
			},
			(e) => {
				if (!cancelled) {
					footprintError = e instanceof Error ? e.message : 'Failed to load footprint';
					footprintLoading = false;
				}
			}
		);
		return () => {
			cancelled = true;
		};
	});
</script>

<svelte:head>
	<title>Run #{run?.run_number ?? ''} | Meticulous</title>
</svelte:head>

<div class="space-y-6">
	<div class="flex items-start gap-4">
		<Button variant="ghost" size="sm" href="/runs">
			<ArrowLeft class="h-4 w-4" />
		</Button>

		{#if loading}
			<div class="flex-1 space-y-2">
				<Skeleton class="h-7 w-48" />
				<Skeleton class="h-4 w-64" />
			</div>
		{:else if run && pipeline}
			<div class="flex-1">
				<div class="flex items-center gap-3">
					<h1 class="text-2xl font-bold text-[var(--text-primary)]">
						Run #{run.run_number}
					</h1>
					<StatusBadge status={run.status} />
				</div>
				{#if run.parent_run_id}
					<div
						class="mt-3 rounded-lg border border-amber-200/80 bg-amber-50/90 px-4 py-2.5 text-sm dark:border-amber-900/60 dark:bg-amber-950/40"
					>
						<span class="text-[var(--text-secondary)]">This run is a </span>
						<span class="font-medium text-[var(--text-primary)]">retry</span>
						<span class="text-[var(--text-secondary)]"> of </span>
						{#if run.parent_run_number != null}
							<a
								href="/runs/{run.parent_run_id}"
								class="font-mono font-medium text-primary-600 hover:underline dark:text-primary-400"
							>
								run #{run.parent_run_number}
							</a>
						{:else}
							<a
								href="/runs/{run.parent_run_id}"
								class="font-medium text-primary-600 hover:underline dark:text-primary-400"
							>
								the previous run
							</a>
						{/if}
						<span class="text-[var(--text-secondary)]">.</span>
					</div>
				{/if}
				<div class="mt-2 flex flex-wrap items-center gap-4 text-sm text-[var(--text-secondary)]">
					<a href="/pipelines/{pipeline.id}" class="flex items-center gap-1 hover:text-primary-600">
						<GitBranch class="h-4 w-4" />
						{pipeline.name}
					</a>
					{#if run.branch}
						<span class="flex items-center gap-1">
							<GitBranch class="h-4 w-4" />
							{run.branch}
						</span>
					{/if}
					{#if run.commit_sha}
						<span class="flex items-center gap-1 font-mono text-xs">
							<GitCommit class="h-4 w-4" />
							{run.commit_sha.slice(0, 7)}
							<CopyButton text={run.commit_sha} size="sm" />
						</span>
					{/if}
					<span class="flex items-center gap-1">
						<User class="h-4 w-4" />
						{run.triggered_by}
					</span>
					<span class="flex items-center gap-1">
						<Clock class="h-4 w-4" />
						{formatRelativeTime(run.created_at)}
					</span>
					{#if run.duration_ms}
						<span>Duration: {formatDurationMs(run.duration_ms)}</span>
					{/if}
				</div>
			</div>

			<div class="flex items-center gap-2">
				<Button variant="ghost" size="sm" onclick={loadRun}>
					<RefreshCw class="h-4 w-4" />
				</Button>
				{#if !isTerminal}
					<Button variant="outline" size="sm" onclick={cancelRun} loading={cancelLoading}>
						<XCircle class="h-4 w-4" />
						Cancel
					</Button>
				{:else}
					<Button variant="outline" size="sm" onclick={retryRun} loading={retryLoading}>
						<RotateCcw class="h-4 w-4" />
						Retry
					</Button>
				{/if}
			</div>
		{/if}
	</div>

	{#if error}
		<Alert variant="error" title="Error" dismissible ondismiss={() => (error = null)}>
			{error}
		</Alert>
	{/if}

	{#if !loading && run}
		<Tabs items={tabs} bind:value={activeTab} />

		{#if activeTab === 'jobs'}
			<div class="grid gap-6 lg:grid-cols-3">
				<div class="lg:col-span-1">
					<Card padding="none">
						<div class="border-b border-[var(--border-primary)] px-4 py-3">
							<h3 class="font-medium text-[var(--text-primary)]">Jobs</h3>
						</div>
						<div class="divide-y divide-[var(--border-secondary)]">
							{#if jobRuns.length === 0}
								<div class="p-4 text-center text-sm text-[var(--text-secondary)]">
									No jobs found
								</div>
							{:else}
								{#each jobRuns as jobRun (jobRun.id)}
									<button
										type="button"
										class="
											flex w-full items-center gap-3 px-4 py-3 text-left transition-colors
											{selectedJobRunId === jobRun.id
												? 'bg-primary-50 dark:bg-primary-900/20'
												: 'hover:bg-[var(--bg-hover)]'}
										"
										onclick={() => (selectedJobRunId = jobRun.id)}
									>
										<StatusBadge status={jobRun.status} size="sm" showIcon={true} />
										<div class="flex-1 min-w-0">
											<p class="truncate font-medium text-[var(--text-primary)]">
												{jobRun.job_name}
											</p>
											{#if jobRun.scheduling_note}
												<p class="mt-0.5 text-xs leading-snug text-[var(--text-secondary)] line-clamp-2">
													{jobRun.scheduling_note}
												</p>
											{/if}
											{#if jobRun.started_at}
												<p class="text-xs text-[var(--text-tertiary)]">
													{formatDurationMs(
														jobRun.finished_at
															? new Date(jobRun.finished_at).getTime() - new Date(jobRun.started_at).getTime()
															: Date.now() - new Date(jobRun.started_at).getTime()
													)}
												</p>
											{/if}
										</div>
										<ChevronRight class="h-4 w-4 text-[var(--text-tertiary)]" />
									</button>
								{/each}
							{/if}
						</div>
					</Card>
				</div>

				<div class="lg:col-span-2">
					{#if selectedJobRun}
						<Card padding="none">
							<div class="flex items-center justify-between border-b border-[var(--border-primary)] px-4 py-3">
								<div>
									<h3 class="font-medium text-[var(--text-primary)]">{selectedJobRun.job_name}</h3>
									<div class="flex items-center gap-2 mt-1">
										<StatusBadge status={selectedJobRun.status} size="sm" />
										{#if selectedJobRun.attempt > 1}
											<Badge variant="secondary" size="sm">Retry cycle {selectedJobRun.attempt}</Badge>
										{/if}
									</div>
									{#if selectedJobRun.scheduling_note}
										<p class="mt-2 text-sm text-[var(--text-secondary)]">
											{selectedJobRun.scheduling_note}
										</p>
									{/if}
									{#if selectedJobRun.error_message}
										<p
											class="mt-2 text-sm rounded-md border border-[var(--border-primary)] bg-[var(--surface-elevated)] px-3 py-2 text-[var(--color-error-700)] whitespace-pre-wrap break-words"
											role="alert"
										>
											{selectedJobRun.error_message}
										</p>
									{/if}
								</div>
								<div class="text-sm text-[var(--text-secondary)]">
									{#if selectedJobRun.agent_id}
										<span class="font-mono text-xs">{truncateId(selectedJobRun.agent_id)}</span>
									{/if}
								</div>
							</div>
							{#if jobAssignments.length > 1}
								<div class="border-b border-[var(--border-primary)] px-4 py-3">
									<label class="mb-1 block text-xs font-medium text-[var(--text-secondary)]" for="dispatch-filter"
										>Agent dispatch attempt</label
									>
									<Select
										id="dispatch-filter"
										options={dispatchSelectOptions}
										bind:value={dispatchFilter}
										size="sm"
										class="max-w-lg"
										disabled={assignmentsLoading}
									/>
									<p class="mt-1 text-xs text-[var(--text-tertiary)]">
										Logs are filtered by time window per dispatch when a single attempt is selected.
									</p>
								</div>
							{/if}
							<div class="h-96">
								<LogViewer
									runId={run.id}
									jobRunId={selectedJobRun.id}
									jobStatus={selectedJobRun.status}
									logTimeFilter={logTimeFilterForViewer}
								/>
							</div>
						</Card>
					{:else}
						<Card>
							<div class="flex items-center justify-center py-12 text-[var(--text-secondary)]">
								Select a job to view logs
							</div>
						</Card>
					{/if}
				</div>
			</div>
		{:else if activeTab === 'graph'}
			<Card>
				{#if graphDagLoading}
					<div class="space-y-4 p-2">
						<Skeleton class="h-48 w-full" />
						<Skeleton class="h-24 w-full" />
					</div>
				{:else if graphDagError}
					<Alert variant="error" title="Graph" dismissible ondismiss={() => (graphDagError = null)}>
						{graphDagError}
					</Alert>
				{:else if runDag && runDag.nodes.length > 0}
					<DagViewer
						jobs={runDag.nodes.map((n) => ({
							name: n.job_name,
							depends_on: n.depends_on,
							status: n.status,
							executed_binaries: n.executed_binaries
						}))}
					/>
				{:else}
					<div class="flex items-center justify-center py-12 text-[var(--text-secondary)]">
						No job graph available
					</div>
				{/if}
			</Card>
		{:else if activeTab === 'sbom'}
			<Card>
				{#if sbomLoading}
					<div class="space-y-3 p-2">
						<Skeleton class="h-10 w-full" />
						<Skeleton class="h-40 w-full" />
					</div>
				{:else if sbomError}
					<Alert variant="error" title="SBOM" dismissible ondismiss={() => (sbomError = null)}>
						{sbomError}
					</Alert>
				{:else if sbomRes?.sbom}
					<SbomViewer rawDocument={sbomRes.sbom} />
				{:else}
					<SbomViewer empty />
				{/if}
			</Card>
		{:else if activeTab === 'blast-radius'}
			<Card>
				{#if footprintLoading}
					<div class="space-y-4 p-2">
						<Skeleton class="h-24 w-full" />
						<Skeleton class="h-48 w-full" />
					</div>
				{:else if footprintError}
					<Alert variant="error" title="Blast radius" dismissible ondismiss={() => (footprintError = null)}>
						{footprintError}
					</Alert>
				{:else if footprint}
					<RunFootprintViewer data={footprint} />
				{:else}
					<div class="py-12 text-center text-sm text-[var(--text-secondary)]">No data</div>
				{/if}
			</Card>
		{/if}
	{/if}
</div>
