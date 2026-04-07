<script lang="ts">
	import { browser } from '$app/environment';
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { Button, Card, Badge, Tabs, Alert, StatusBadge, CopyButton, Select } from '$components/ui';
	import { Skeleton } from '$components/data';
	import { apiMethods } from '$api/client';
	import type { Run, JobRun, Pipeline, JobAssignment, RunDagResponse, StepRun } from '$api/types';
	import JobRunAgentPanel from '$lib/components/agents/JobRunAgentPanel.svelte';
	import { auditFromSnapshot, type JobRunAgentAudit } from '$lib/utils/jobRunAgentAudit';
	import {
		formatRelativeTime,
		formatDurationMs,
		truncateId,
		formatDateTime,
		formatDateTimeForTitle,
		formatRunTriggeredBy
	} from '$utils/format';
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
		AlertTriangle,
		Fingerprint
	} from 'lucide-svelte';
	import { DagViewer } from '$components/pipeline';
	import LogViewer, {
		LOG_STEP_FILTER_ALL,
		LOG_STEP_FILTER_UNSCOPED
	} from '$components/logs/LogViewer.svelte';
	import { SbomViewer } from '$components/sbom';
	import { RunFootprintViewer } from '$components/blast-radius';
	import RunJobDefinitionsCompare from '$components/run-compare/RunJobDefinitionsCompare.svelte';
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
	let compareRuns = $state(false);
	let compareBaselineRunId = $state('');
	let compareRunCandidates = $state<Run[]>([]);
	let compareListLoading = $state(false);
	let prevJobRuns = $state<JobRun[]>([]);
	let prevSbomRes = $state<SbomApiResponse | null>(null);
	let prevSbomError = $state<string | null>(null);
	let prevFootprint = $state<RunFootprintResponse | null>(null);
	let prevFootprintError = $state<string | null>(null);
	let jobStepRuns = $state<StepRun[]>([]);
	let jobStepsLoading = $state(false);
	let logStepFilter = $state(LOG_STEP_FILTER_ALL);
	let logHasUnscopedLogLines = $state(false);
	let lastLogStepJobRunId = $state<string | null>(null);

	function catalogWorkflowRef(sw: Record<string, unknown> | undefined | null): string | null {
		if (!sw || typeof sw !== 'object') return null;
		const scope = sw['scope'];
		const name = sw['name'];
		const version = sw['version'];
		if (typeof scope === 'string' && typeof name === 'string' && typeof version === 'string') {
			return `${scope}/${name}@${version}`;
		}
		return null;
	}

	/** Pipeline workflow block → catalog ref → job inside the catalog workflow; steps are listed below. */
	function jobRunDisplay(jr: JobRun): {
		pipelineWorkflowLine: string;
		catalogLine: string | null;
		jobLine: string | null;
	} {
		const sw = jr.source_workflow;
		if (!sw || typeof sw !== 'object') {
			return { pipelineWorkflowLine: jr.job_name, catalogLine: null, jobLine: null };
		}
		const o = sw as Record<string, unknown>;
		const invName = typeof o.invocation_name === 'string' ? o.invocation_name : null;
		const invId = typeof o.invocation_id === 'string' ? o.invocation_id : null;
		const pipelineWorkflowLine = invName ?? (invId ? `Workflow · ${invId}` : jr.job_name);
		const catalogLine = catalogWorkflowRef(o);
		return {
			pipelineWorkflowLine,
			catalogLine,
			jobLine: `Job · ${jr.job_name}`
		};
	}

	const baselineRun = $derived(
		compareRunCandidates.find((c) => c.id === compareBaselineRunId) ?? null
	);

	const compareRunSelectOptions = $derived.by(() =>
		compareRunCandidates.map((r) => ({
			value: r.id,
			label: `#${r.run_number} · ${r.status} · ${formatRelativeTime(r.created_at)}`
		}))
	);

	const tabs = [
		{ id: 'jobs', label: 'Jobs', icon: Terminal },
		{ id: 'agents', label: 'Agents', icon: Fingerprint },
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

	function agentTabModel(jr: JobRun): {
		audit: JobRunAgentAudit | null;
		assignedAgentId: string | null;
	} {
		if (jr.agent_snapshot && typeof jr.agent_snapshot === 'object') {
			return {
				audit: auditFromSnapshot(
					jr.agent_snapshot as Record<string, unknown>,
					jr.agent_snapshot_captured_at
				),
				assignedAgentId: jr.agent_id ?? null
			};
		}
		return { audit: null, assignedAgentId: jr.agent_id ?? null };
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

	const logStepDisplayNames = $derived(
		Object.fromEntries(jobStepRuns.map((s) => [s.id, s.step_name]))
	);

	$effect(() => {
		const jid = selectedJobRunId;
		if (jid !== lastLogStepJobRunId) {
			lastLogStepJobRunId = jid;
			logStepFilter = LOG_STEP_FILTER_ALL;
		}
		if (!jid) {
			lastLogStepJobRunId = null;
		}
	});

	$effect(() => {
		const rid = run?.id;
		const jid = selectedJobRunId;
		const onJobsTab = activeTab === 'jobs';
		if (!rid || !jid || !onJobsTab) {
			jobStepRuns = [];
			jobStepsLoading = false;
			return;
		}
		let cancelled = false;
		jobStepsLoading = true;
		void apiMethods.runs
			.jobSteps(rid, jid)
			.then((rows) => {
				if (!cancelled) jobStepRuns = rows;
			})
			.catch(() => {
				if (!cancelled) jobStepRuns = [];
			})
			.finally(() => {
				if (!cancelled) jobStepsLoading = false;
			});
		return () => {
			cancelled = true;
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
		if (!browser || !run) return;
		void run.id;
		if (!compareRuns) {
			compareRunCandidates = [];
			compareBaselineRunId = '';
			prevJobRuns = [];
			prevSbomRes = null;
			prevSbomError = null;
			prevFootprint = null;
			prevFootprintError = null;
			compareListLoading = false;
			return;
		}

		let cancelled = false;
		compareListLoading = true;
		const pipelineId = run.pipeline_id;
		const currentRunId = run.id;
		const currentRunNumber = run.run_number;
		void (async () => {
			try {
				const res = await apiMethods.runs.list({
					pipeline_id: pipelineId,
					per_page: 80
				});
				if (cancelled) return;
				const others = res.data.filter((r) => r.id !== currentRunId);
				others.sort((a, b) => b.run_number - a.run_number);
				compareRunCandidates = others;
				const preferred =
					others.find((r) => r.run_number === currentRunNumber - 1) ?? others[0];
				const keepCurrent =
					compareBaselineRunId && others.some((r) => r.id === compareBaselineRunId);
				if (!keepCurrent) {
					compareBaselineRunId = preferred?.id ?? '';
				}
			} catch {
				if (!cancelled) {
					compareRunCandidates = [];
					compareBaselineRunId = '';
				}
			} finally {
				if (!cancelled) {
					compareListLoading = false;
				}
			}
		})();
		return () => {
			cancelled = true;
		};
	});

	$effect(() => {
		if (!browser || !compareRuns) {
			prevJobRuns = [];
			return;
		}
		if (!baselineRun) {
			prevJobRuns = [];
			return;
		}
		let cancelled = false;
		void apiMethods.runs.jobs(baselineRun.id).then(
			(j) => {
				if (!cancelled) prevJobRuns = j;
			},
			() => {
				if (!cancelled) prevJobRuns = [];
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
		void compareRuns;
		void baselineRun?.id;
		let cancelled = false;
		sbomLoading = true;
		sbomError = null;

		void (async () => {
			try {
				const cur = await apiMethods.artifacts.sbom(run.id);
				if (cancelled) return;
				sbomRes = cur;
				if (compareRuns && baselineRun) {
					try {
						const p = await apiMethods.artifacts.sbom(baselineRun.id);
						if (cancelled) return;
						prevSbomRes = p;
						prevSbomError = null;
					} catch (e) {
						if (!cancelled) {
							prevSbomRes = null;
							prevSbomError =
								e instanceof Error ? e.message : 'Failed to load baseline SBOM';
						}
					}
				} else {
					prevSbomRes = null;
					prevSbomError = null;
				}
			} catch (e) {
				if (!cancelled) {
					sbomError = e instanceof Error ? e.message : 'Failed to load SBOM';
					sbomRes = null;
					prevSbomRes = null;
					prevSbomError = null;
				}
			} finally {
				if (!cancelled) {
					sbomLoading = false;
				}
			}
		})();

		return () => {
			cancelled = true;
		};
	});

	$effect(() => {
		if (!browser || activeTab !== 'blast-radius' || !run) return;
		void run.id;
		void run.status;
		void compareRuns;
		void baselineRun?.id;
		let cancelled = false;
		footprintLoading = true;
		footprintError = null;

		void (async () => {
			try {
				const cur = await apiMethods.runs.footprint(run.id);
				if (cancelled) return;
				footprint = cur;
				if (compareRuns && baselineRun) {
					try {
						const p = await apiMethods.runs.footprint(baselineRun.id);
						if (cancelled) return;
						prevFootprint = p;
						prevFootprintError = null;
					} catch (e) {
						if (!cancelled) {
							prevFootprint = null;
							prevFootprintError =
								e instanceof Error ? e.message : 'Failed to load baseline footprint';
						}
					}
				} else {
					prevFootprint = null;
					prevFootprintError = null;
				}
			} catch (e) {
				if (!cancelled) {
					footprintError = e instanceof Error ? e.message : 'Failed to load footprint';
					footprint = null;
					prevFootprint = null;
					prevFootprintError = null;
				}
			} finally {
				if (!cancelled) {
					footprintLoading = false;
				}
			}
		})();

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
					<StatusBadge status={run.status_display ?? run.status} />
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
						{formatRunTriggeredBy(run.triggered_by, run.webhook_remote_addr)}
					</span>
					<span
						class="flex items-center gap-1"
						title={formatDateTimeForTitle(run.created_at) || undefined}
					>
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
			<div class="grid gap-6 lg:grid-cols-3 lg:items-stretch">
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
									{@const jdisp = jobRunDisplay(jobRun)}
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
											{#if jdisp.catalogLine}
												<p
													class="truncate font-mono text-[0.65rem] text-[var(--text-tertiary)]"
													title={jdisp.catalogLine}
												>
													{jdisp.catalogLine}
												</p>
											{/if}
											<p class="truncate font-medium text-[var(--text-primary)]">
												{jdisp.pipelineWorkflowLine}
											</p>
											{#if jdisp.jobLine}
												<p class="truncate text-sm text-[var(--text-secondary)]">{jdisp.jobLine}</p>
											{/if}
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
						{#if selectedJobRunId}
							<div class="border-t border-[var(--border-primary)] bg-[var(--bg-secondary)]/25 px-3 py-3">
								<p class="mb-2 text-xs font-medium text-[var(--text-secondary)]">Steps</p>
								<p class="mb-2 text-[0.65rem] leading-snug text-[var(--text-tertiary)]">
									Steps run in order for the selected job (see “Job · …” in the list).
								</p>
								<div class="flex flex-col gap-1.5">
									<button
										type="button"
										class="w-full rounded-lg border px-3 py-2 text-left transition-colors
											{logStepFilter === LOG_STEP_FILTER_ALL
											? 'border-primary-400 bg-primary-50 ring-1 ring-primary-400/40 dark:bg-primary-900/25'
											: 'border-[var(--border-primary)] hover:bg-[var(--bg-hover)]'}"
										onclick={() => (logStepFilter = LOG_STEP_FILTER_ALL)}
									>
										<span class="text-sm font-medium text-[var(--text-primary)]">All steps</span>
										<p class="mt-0.5 text-[0.65rem] text-[var(--text-tertiary)]">
											Full job log (newest at the bottom while running)
										</p>
									</button>
									{#if logHasUnscopedLogLines}
										<button
											type="button"
											class="w-full rounded-lg border px-3 py-2 text-left transition-colors
												{logStepFilter === LOG_STEP_FILTER_UNSCOPED
												? 'border-primary-400 bg-primary-50 ring-1 ring-primary-400/40 dark:bg-primary-900/25'
												: 'border-[var(--border-primary)] hover:bg-[var(--bg-hover)]'}"
											onclick={() => (logStepFilter = LOG_STEP_FILTER_UNSCOPED)}
										>
											<span class="text-sm font-medium text-[var(--text-primary)]">Unscoped lines</span>
											<p class="mt-0.5 text-[0.65rem] text-[var(--text-tertiary)]">
												Logs with no step id only
											</p>
										</button>
									{/if}
									{#if jobStepsLoading && jobStepRuns.length === 0}
										<div class="space-y-2 py-1">
											<Skeleton class="h-9 w-full rounded-lg" />
											<Skeleton class="h-9 w-full rounded-lg" />
										</div>
									{:else}
										{#each jobStepRuns as st (st.id)}
											<button
												type="button"
												class="w-full rounded-lg border px-3 py-2 text-left transition-colors
													{logStepFilter === st.id
													? 'border-primary-400 bg-primary-50 ring-1 ring-primary-400/40 dark:bg-primary-900/25'
													: 'border-[var(--border-primary)] hover:bg-[var(--bg-hover)]'}"
												onclick={() => (logStepFilter = st.id)}
											>
												<p class="text-[0.65rem] font-medium uppercase tracking-wide text-[var(--text-tertiary)]">
													Step
												</p>
												<div class="flex items-center gap-2">
													<StatusBadge status={st.status} size="sm" showIcon={true} />
													<span class="truncate text-sm font-medium text-[var(--text-primary)]">
														{st.step_name}
													</span>
												</div>
												<p
													class="mt-1 font-mono text-[0.65rem] leading-snug text-[var(--text-tertiary)]"
													title={st.id}
												>
													id {truncateId(st.id, 10)}…
												</p>
												{#if st.step_id}
													<p class="font-mono text-[0.65rem] text-[var(--text-tertiary)]" title={st.step_id}>
														step_id {st.step_id.length > 12 ? `${truncateId(st.step_id, 12)}…` : st.step_id}
													</p>
												{/if}
											</button>
										{/each}
									{/if}
								</div>
							</div>
						{/if}
					</Card>
				</div>

				<div class="lg:col-span-2 flex min-h-[calc(100dvh-11rem)] flex-col">
					{#if selectedJobRun}
						{@const sjdisp = jobRunDisplay(selectedJobRun)}
						<Card padding="none" class="flex min-h-0 flex-1 flex-col overflow-hidden">
							<div class="flex items-center justify-between border-b border-[var(--border-primary)] px-4 py-3">
								<div>
									{#if sjdisp.catalogLine}
										<p class="font-mono text-xs text-[var(--text-tertiary)]">{sjdisp.catalogLine}</p>
									{/if}
									<h3 class="font-medium text-[var(--text-primary)]">{sjdisp.pipelineWorkflowLine}</h3>
									{#if sjdisp.jobLine}
										<p class="mt-0.5 text-sm text-[var(--text-secondary)]">{sjdisp.jobLine}</p>
									{/if}
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
										Logs are filtered by time window per dispatch when a single attempt is selected. Choose a
										workflow step under the job list (left) to focus logs on one step.
									</p>
								</div>
							{/if}
							<div class="min-h-0 flex-1">
								{#key selectedJobRun.id}
									<LogViewer
										runId={run.id}
										jobRunId={selectedJobRun.id}
										jobStatus={selectedJobRun.status}
										logTimeFilter={logTimeFilterForViewer}
										bind:stepLogFilter={logStepFilter}
										stepDisplayNames={logStepDisplayNames}
										bind:hasUnscopedLogLines={logHasUnscopedLogLines}
									/>
								{/key}
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
		{:else if activeTab === 'agents'}
			<div class="space-y-4">
				<p class="text-sm text-[var(--text-secondary)]">
					Each panel shows the agent exactly as persisted on the job run when it entered
					<span class="font-medium text-[var(--text-primary)]">running</span>
					(same fields as the Agents page: OS, pool, capacity, registration bundle, host metadata). There is no live
					fallback—only the stored snapshot counts for forensics.
				</p>
				{#if jobRuns.length === 0}
					<Card>
						<div class="py-10 text-center text-sm text-[var(--text-secondary)]">No jobs on this run</div>
					</Card>
				{:else}
					{#each jobRuns as jr (jr.id)}
						{@const m = agentTabModel(jr)}
						{#key jr.id}
							<JobRunAgentPanel
								jobName={jr.job_name}
								jobStatus={jr.status}
								audit={m.audit}
								assignedAgentId={m.assignedAgentId}
							/>
						{/key}
					{/each}
				{/if}
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
						runId={run.id}
						jobs={runDag.nodes.map((n) => ({
							name: n.job_name,
							depends_on: n.depends_on,
							status: n.status,
							job_run_id: n.job_run_id ?? null,
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
				<div
					class="mb-4 flex flex-col gap-3 border-b border-[var(--border-primary)] pb-4 sm:flex-row sm:items-end sm:justify-between"
				>
					<label class="flex cursor-pointer items-center gap-2 text-sm text-[var(--text-primary)]">
						<input
							type="checkbox"
							class="rounded border-[var(--border-primary)]"
							bind:checked={compareRuns}
						/>
						Compare to another run
						<span class="text-xs text-[var(--text-tertiary)]">(defaults to previous #)</span>
					</label>
					{#if compareRuns && compareRunCandidates.length > 0}
						<div class="flex w-full flex-col gap-1 sm:w-auto sm:min-w-[18rem]">
							<span class="text-xs font-medium text-[var(--text-secondary)]">Baseline run</span>
							<Select
								id="sbom-baseline-select"
								options={compareRunSelectOptions}
								bind:value={compareBaselineRunId}
								size="sm"
								disabled={compareListLoading}
							/>
						</div>
					{/if}
				</div>

				{#if compareRuns}
					{#if compareListLoading}
						<div class="space-y-3 p-2">
							<Skeleton class="h-10 w-full" />
							<Skeleton class="h-40 w-full" />
						</div>
					{:else if compareRunCandidates.length === 0}
						<p class="p-2 text-sm text-[var(--text-secondary)]">
							No other runs on this pipeline to compare.
						</p>
					{:else if !baselineRun}
						<div class="space-y-3 p-2">
							<Skeleton class="h-10 w-full" />
							<Skeleton class="h-40 w-full" />
						</div>
					{:else if sbomLoading}
						<div class="grid gap-6 lg:grid-cols-2">
							<Skeleton class="h-64 w-full" />
							<Skeleton class="h-64 w-full" />
						</div>
					{:else if sbomError}
						<Alert variant="error" title="SBOM" dismissible ondismiss={() => (sbomError = null)}>
							{sbomError}
						</Alert>
					{:else}
						<div class="grid gap-6 lg:grid-cols-2">
							<div class="min-w-0 space-y-2 lg:rounded-lg lg:border lg:border-[var(--border-primary)] lg:bg-[var(--bg-secondary)]/40 lg:p-3">
								<div class="flex flex-wrap items-center gap-2 text-xs font-medium text-[var(--text-secondary)]">
									<span>Run #{baselineRun.run_number}</span>
									<StatusBadge status={baselineRun.status_display ?? baselineRun.status} size="sm" />
									<span class="text-[var(--text-tertiary)]">baseline</span>
								</div>
								{#if prevSbomRes?.job_name || prevSbomRes?.step_name}
									<p class="text-xs text-[var(--text-tertiary)]">
										<span class="font-medium text-[var(--text-secondary)]">SBOM from</span>
										{#if prevSbomRes?.job_name}
											<span class="text-[var(--text-primary)]"> job {prevSbomRes.job_name}</span>
										{/if}
										{#if prevSbomRes?.step_name}
											<span class="text-[var(--text-primary)]"> · step {prevSbomRes.step_name}</span>
										{/if}
									</p>
								{/if}
								{#if prevSbomError}
									<Alert variant="error" title="Baseline SBOM">{prevSbomError}</Alert>
								{:else if prevSbomRes?.sbom}
									<SbomViewer
										rawDocument={prevSbomRes.sbom}
										apiFormat={prevSbomRes.format}
										runId={prevSbomRes.run_id}
									/>
								{:else if prevSbomRes?.status === 'artifact_registered'}
									<div class="space-y-2 text-sm text-[var(--text-secondary)]">
										{#if prevSbomRes?.job_name || prevSbomRes?.step_name}
											<p class="text-xs text-[var(--text-tertiary)]">
												<span class="font-medium text-[var(--text-secondary)]">SBOM artifact from</span>
												{#if prevSbomRes?.job_name}
													<span class="text-[var(--text-primary)]"> job {prevSbomRes.job_name}</span>
												{/if}
												{#if prevSbomRes?.step_name}
													<span class="text-[var(--text-primary)]"> · step {prevSbomRes.step_name}</span>
												{/if}
											</p>
										{/if}
										<p class="font-medium text-[var(--text-primary)]">
											SBOM artifact linked, preview not loaded
										</p>
										<p class="text-xs text-[var(--text-tertiary)]">
											Download from artifacts or use inline metadata for preview.
										</p>
									</div>
								{:else}
									<SbomViewer empty />
								{/if}
							</div>
							<div class="min-w-0 space-y-2 lg:rounded-lg lg:border lg:border-[var(--border-primary)] lg:bg-[var(--bg-secondary)]/40 lg:p-3">
								<div class="flex flex-wrap items-center gap-2 text-xs font-medium text-[var(--text-secondary)]">
									<span>Run #{run.run_number}</span>
									<StatusBadge status={run.status_display ?? run.status} size="sm" />
									<span class="text-[var(--text-tertiary)]">this run</span>
								</div>
								{#if sbomRes?.job_name || sbomRes?.step_name}
									<p class="text-xs text-[var(--text-tertiary)]">
										<span class="font-medium text-[var(--text-secondary)]">SBOM from</span>
										{#if sbomRes?.job_name}
											<span class="text-[var(--text-primary)]"> job {sbomRes.job_name}</span>
										{/if}
										{#if sbomRes?.step_name}
											<span class="text-[var(--text-primary)]"> · step {sbomRes.step_name}</span>
										{/if}
									</p>
								{/if}
								{#if sbomRes?.sbom}
									<SbomViewer
										rawDocument={sbomRes.sbom}
										apiFormat={sbomRes.format}
										runId={sbomRes.run_id}
									/>
								{:else if sbomRes?.status === 'artifact_registered'}
									<div class="space-y-2 p-1 text-sm text-[var(--text-secondary)]">
										{#if sbomRes?.job_name || sbomRes?.step_name}
											<p class="text-xs text-[var(--text-tertiary)]">
												<span class="font-medium text-[var(--text-secondary)]">SBOM artifact from</span>
												{#if sbomRes?.job_name}
													<span class="text-[var(--text-primary)]"> job {sbomRes.job_name}</span>
												{/if}
												{#if sbomRes?.step_name}
													<span class="text-[var(--text-primary)]"> · step {sbomRes.step_name}</span>
												{/if}
											</p>
										{/if}
										<p class="font-medium text-[var(--text-primary)]">SBOM artifact linked, preview not loaded</p>
										<p>
											The run has an artifact whose name or path looks like an SBOM (e.g.
											<span class="font-mono text-xs">sbom.spdx.json</span>), but the API does not yet stream the blob
											into this view. Download it from the run&apos;s artifact list, or store the document under
											<span class="font-mono text-xs">metadata.sbom_json</span> on the artifact row for an inline preview.
										</p>
										<p class="text-xs text-[var(--text-tertiary)]">
											Trivy: <span class="font-mono">trivy fs --format spdx-json --output sbom.spdx.json .</span> then
											upload <span class="font-mono">sbom.spdx.json</span> as a run artifact.
										</p>
									</div>
								{:else}
									<SbomViewer empty />
								{/if}
							</div>
						</div>
						<div class="mt-8">
							<RunJobDefinitionsCompare
								currentRunId={run.id}
								previousRunId={baselineRun.id}
								currentRunLabel={`Run #${run.run_number}`}
								previousRunLabel={`Run #${baselineRun.run_number}`}
								jobRuns={jobRuns}
								prevJobRuns={prevJobRuns}
							/>
						</div>
					{/if}
				{:else if sbomLoading}
					<div class="space-y-3 p-2">
						<Skeleton class="h-10 w-full" />
						<Skeleton class="h-40 w-full" />
					</div>
				{:else if sbomError}
					<Alert variant="error" title="SBOM" dismissible ondismiss={() => (sbomError = null)}>
						{sbomError}
					</Alert>
				{:else if sbomRes?.sbom}
					{#if sbomRes.job_name || sbomRes.step_name}
						<p class="mb-3 text-xs text-[var(--text-tertiary)]">
							<span class="font-medium text-[var(--text-secondary)]">SBOM from</span>
							{#if sbomRes.job_name}
								<span class="text-[var(--text-primary)]"> job {sbomRes.job_name}</span>
							{/if}
							{#if sbomRes.step_name}
								<span class="text-[var(--text-primary)]"> · step {sbomRes.step_name}</span>
							{/if}
						</p>
					{/if}
					<SbomViewer
						rawDocument={sbomRes.sbom}
						apiFormat={sbomRes.format}
						runId={sbomRes.run_id}
					/>
				{:else if sbomRes?.status === 'artifact_registered'}
					<div class="space-y-2 p-4 text-sm text-[var(--text-secondary)]">
						{#if sbomRes.job_name || sbomRes.step_name}
							<p class="text-xs text-[var(--text-tertiary)]">
								<span class="font-medium text-[var(--text-secondary)]">SBOM artifact from</span>
								{#if sbomRes.job_name}
									<span class="text-[var(--text-primary)]"> job {sbomRes.job_name}</span>
								{/if}
								{#if sbomRes.step_name}
									<span class="text-[var(--text-primary)]"> · step {sbomRes.step_name}</span>
								{/if}
							</p>
						{/if}
						<p class="font-medium text-[var(--text-primary)]">SBOM artifact linked, preview not loaded</p>
						<p>
							The run has an artifact whose name or path looks like an SBOM (e.g.
							<span class="font-mono text-xs">sbom.spdx.json</span>), but the API does not yet stream the blob
							into this view. Download it from the run&apos;s artifact list, or store the document under
							<span class="font-mono text-xs">metadata.sbom_json</span> on the artifact row for an inline preview.
						</p>
						<p class="text-xs text-[var(--text-tertiary)]">
							Trivy: <span class="font-mono">trivy fs --format spdx-json --output sbom.spdx.json .</span> then
							upload <span class="font-mono">sbom.spdx.json</span> as a run artifact. Object-store layout (when
							enabled) follows <span class="font-mono">runs/&lt;run_id&gt;/sbom/spdx.json</span>.
						</p>
					</div>
				{:else}
					<SbomViewer empty />
				{/if}
			</Card>
		{:else if activeTab === 'blast-radius'}
			<Card>
				<div
					class="mb-4 flex flex-col gap-3 border-b border-[var(--border-primary)] pb-4 sm:flex-row sm:items-end sm:justify-between"
				>
					<label class="flex cursor-pointer items-center gap-2 text-sm text-[var(--text-primary)]">
						<input
							type="checkbox"
							class="rounded border-[var(--border-primary)]"
							bind:checked={compareRuns}
						/>
						Compare to another run
						<span class="text-xs text-[var(--text-tertiary)]">(defaults to previous #)</span>
					</label>
					{#if compareRuns && compareRunCandidates.length > 0}
						<div class="flex w-full flex-col gap-1 sm:w-auto sm:min-w-[18rem]">
							<span class="text-xs font-medium text-[var(--text-secondary)]">Baseline run</span>
							<Select
								options={compareRunSelectOptions}
								bind:value={compareBaselineRunId}
								size="sm"
								disabled={compareListLoading}
							/>
						</div>
					{/if}
				</div>

				{#if compareRuns}
					{#if compareListLoading}
						<div class="space-y-4 p-2">
							<Skeleton class="h-24 w-full" />
							<Skeleton class="h-48 w-full" />
						</div>
					{:else if compareRunCandidates.length === 0}
						<p class="p-2 text-sm text-[var(--text-secondary)]">
							No other runs on this pipeline to compare.
						</p>
					{:else if footprintLoading}
						<div class="grid gap-6 lg:grid-cols-2">
							<Skeleton class="min-h-64 w-full" />
							<Skeleton class="min-h-64 w-full" />
						</div>
					{:else if footprintError}
						<Alert variant="error" title="Blast radius" dismissible ondismiss={() => (footprintError = null)}>
							{footprintError}
						</Alert>
					{:else if !baselineRun}
						<div class="space-y-4 p-2">
							<Skeleton class="h-24 w-full" />
							<Skeleton class="h-48 w-full" />
						</div>
					{:else}
						<div class="grid gap-6 lg:grid-cols-2">
							<div class="min-w-0 space-y-2 lg:rounded-lg lg:border lg:border-[var(--border-primary)] lg:bg-[var(--bg-secondary)]/40 lg:p-3">
								<div class="flex flex-wrap items-center gap-2 text-xs font-medium text-[var(--text-secondary)]">
									<span>Run #{baselineRun.run_number}</span>
									<StatusBadge status={baselineRun.status_display ?? baselineRun.status} size="sm" />
									<span class="text-[var(--text-tertiary)]">baseline</span>
								</div>
								{#if prevFootprintError}
									<Alert variant="error" title="Baseline footprint">{prevFootprintError}</Alert>
								{:else if prevFootprint}
									<RunFootprintViewer data={prevFootprint} />
								{:else}
									<div class="py-8 text-center text-sm text-[var(--text-secondary)]">No data</div>
								{/if}
							</div>
							<div class="min-w-0 space-y-2 lg:rounded-lg lg:border lg:border-[var(--border-primary)] lg:bg-[var(--bg-secondary)]/40 lg:p-3">
								<div class="flex flex-wrap items-center gap-2 text-xs font-medium text-[var(--text-secondary)]">
									<span>Run #{run.run_number}</span>
									<StatusBadge status={run.status_display ?? run.status} size="sm" />
									<span class="text-[var(--text-tertiary)]">this run</span>
								</div>
								{#if footprint}
									<RunFootprintViewer data={footprint} />
								{:else}
									<div class="py-8 text-center text-sm text-[var(--text-secondary)]">No data</div>
								{/if}
							</div>
						</div>
						<div class="mt-8">
							<RunJobDefinitionsCompare
								currentRunId={run.id}
								previousRunId={baselineRun.id}
								currentRunLabel={`Run #${run.run_number}`}
								previousRunLabel={`Run #${baselineRun.run_number}`}
								jobRuns={jobRuns}
								prevJobRuns={prevJobRuns}
							/>
						</div>
					{/if}
				{:else if footprintLoading}
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
