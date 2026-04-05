<script lang="ts">
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { Button, Card, Badge, Tabs, Alert, StatusBadge, CopyButton } from '$components/ui';
	import { Skeleton } from '$components/data';
	import { apiMethods } from '$api/client';
	import type { Run, JobRun, Pipeline, PipelineJob } from '$api/types';
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
	import { BlastRadiusViewer } from '$components/blast-radius';
	import type { SbomDiff } from '$components/sbom';
	import type { BlastRadiusData } from '$components/blast-radius';

	let run = $state<Run | null>(null);
	let pipeline = $state<Pipeline | null>(null);
	let jobRuns = $state<JobRun[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let activeTab = $state('jobs');
	let selectedJobRunId = $state<string | null>(null);
	let cancelLoading = $state(false);
	let retryLoading = $state(false);

	const tabs = [
		{ id: 'jobs', label: 'Jobs', icon: Terminal },
		{ id: 'graph', label: 'Graph', icon: GitBranch },
		{ id: 'sbom', label: 'SBOM', icon: Package },
		{ id: 'blast-radius', label: 'Blast Radius', icon: AlertTriangle }
	];

	const sampleSbomDiff: SbomDiff = {
		added: [
			{ name: 'serde_json', version: '1.0.120', ecosystem: 'cargo', direct: true },
			{ name: 'tokio-util', version: '0.7.11', ecosystem: 'cargo', direct: false }
		],
		removed: [
			{ name: 'serde_yaml', version: '0.9.0', ecosystem: 'cargo', direct: true }
		],
		updated: [
			{ name: 'tokio', ecosystem: 'cargo', from_version: '1.37.0', to_version: '1.38.0' },
			{ name: 'axum', ecosystem: 'cargo', from_version: '0.7.4', to_version: '0.7.5' }
		]
	};

	const sampleBlastRadius: BlastRadiusData = {
		changed_packages: ['met-core', 'met-store'],
		impact_score: 45,
		affected_nodes: [
			{ id: 'met-core', name: 'met-core', type: 'package', impacted: false, direct: true },
			{ id: 'met-store', name: 'met-store', type: 'package', impacted: false, direct: true },
			{ id: 'met-api', name: 'met-api', type: 'package', impacted: true, direct: false },
			{ id: 'met-agent', name: 'met-agent', type: 'package', impacted: true, direct: false },
			{ id: 'api-binary', name: 'meticulous-api', type: 'binary', impacted: true, direct: false },
			{ id: 'agent-binary', name: 'meticulous-agent', type: 'binary', impacted: true, direct: false }
		],
		edges: [
			{ from: 'met-core', to: 'met-store', type: 'depends' },
			{ from: 'met-core', to: 'met-api', type: 'depends' },
			{ from: 'met-store', to: 'met-api', type: 'depends' },
			{ from: 'met-core', to: 'met-agent', type: 'depends' },
			{ from: 'met-api', to: 'api-binary', type: 'produces' },
			{ from: 'met-agent', to: 'agent-binary', type: 'produces' }
		]
	};

	$effect(() => {
		loadRun();
	});

	async function loadRun() {
		loading = true;
		error = null;
		try {
			const runId = $page.params.id!;
			run = await apiMethods.runs.get(runId);
			pipeline = await apiMethods.pipelines.get(run.pipeline_id);
			jobRuns = await apiMethods.runs.jobs(runId);
			
			if (jobRuns.length > 0 && !selectedJobRunId) {
				selectedJobRunId = jobRuns[0].id;
			}
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load run';
		} finally {
			loading = false;
		}
	}

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

	function jobsFromPipelineDef(def: Pipeline['definition'] | undefined): PipelineJob[] {
		if (!def || typeof def !== 'object' || !('jobs' in def)) return [];
		const j = (def as { jobs: unknown }).jobs;
		return Array.isArray(j) ? (j as PipelineJob[]) : [];
	}

	const dagJobs = $derived(() => {
		const jobs = jobsFromPipelineDef(pipeline?.definition);
		return jobs.map((job: PipelineJob) => {
			const jobRun = jobRuns.find((jr) => jr.job_name === job.name);
			return {
				name: job.name,
				depends_on: job.depends_on ?? [],
				status: jobRun?.status
			};
		});
	});

	const selectedJobRun = $derived(
		jobRuns.find((jr) => jr.id === selectedJobRunId)
	);
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
										{#if selectedJobRun.attempt > 0}
											<Badge variant="secondary" size="sm">Attempt {selectedJobRun.attempt + 1}</Badge>
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
							<div class="h-96">
								<LogViewer runId={run.id} jobRunId={selectedJobRun.id} jobStatus={selectedJobRun.status} />
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
				{#if dagJobs().length > 0}
					<DagViewer jobs={dagJobs()} />
				{:else}
					<div class="flex items-center justify-center py-12 text-[var(--text-secondary)]">
						No job graph available
					</div>
				{/if}
			</Card>
		{:else if activeTab === 'sbom'}
			<SbomViewer diff={sampleSbomDiff} />
		{:else if activeTab === 'blast-radius'}
			<BlastRadiusViewer data={sampleBlastRadius} />
		{/if}
	{/if}
</div>
