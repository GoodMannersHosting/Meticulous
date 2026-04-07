<script lang="ts">
	import { Button, Card, StatusBadge, Select } from '$components/ui';
	import type { SelectOption } from '$components/ui';
	import { Skeleton } from '$components/data';
	import { apiMethods } from '$api';
	import type { DashboardStats, RecentRun } from '$api';
	import { formatRelativeTime, formatDurationMs, formatNumber, formatRunTriggeredBy } from '$utils/format';
	import { goto } from '$app/navigation';
	import {
		Activity,
		CheckCircle2,
		History,
		Ban,
		AlertCircle,
		GitBranch,
		ArrowUpRight,
		Play,
		RefreshCw
	} from 'lucide-svelte';
	import { onMount } from 'svelte';

	let stats = $state<DashboardStats | null>(null);
	let recentRuns = $state<RecentRun[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);

	/** Matches API `window` query param */
	let timeWindow = $state('1d');

	const windowOptions: SelectOption[] = [
		{ value: '1h', label: 'Last 1 hour' },
		{ value: '4h', label: 'Last 4 hours' },
		{ value: '12h', label: 'Last 12 hours' },
		{ value: '1d', label: 'Last 24 hours' },
		{ value: '3d', label: 'Last 3 days' },
		{ value: '7d', label: 'Last 7 days' }
	];

	const windowShortLabel = $derived(
		windowOptions.find((o) => o.value === timeWindow)?.label ?? timeWindow
	);

	const recentRunsLimit = 5;

	onMount(() => {
		loadDashboard();

		const interval = setInterval(loadDashboard, 30000);
		return () => clearInterval(interval);
	});

	async function loadDashboard() {
		loading = true;
		error = null;
		try {
			const [statsRes, runsRes] = await Promise.all([
				apiMethods.dashboard.stats(timeWindow),
				apiMethods.dashboard.recentRuns(recentRunsLimit, timeWindow)
			]);

			stats = statsRes;
			recentRuns = runsRes;
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load dashboard';
			stats = null;
			recentRuns = [];
		} finally {
			loading = false;
		}
	}

	const statCards = $derived([
		{
			id: 'total',
			label: 'Total Runs',
			sub: `Created · ${windowShortLabel}`,
			value: stats ? formatNumber(stats.total_runs) : '—',
			icon: History,
			color: 'primary'
		},
		{
			id: 'completed',
			label: 'Completed',
			sub: windowShortLabel,
			value: stats ? formatNumber(stats.completed_runs) : '—',
			icon: CheckCircle2,
			color: 'success'
		},
		{
			id: 'active',
			label: 'Active Runs',
			sub: 'Now · pending, queued & running',
			value: stats ? formatNumber(stats.active_runs) : '—',
			icon: Activity,
			color: 'secondary'
		},
		{
			id: 'cancelled',
			label: 'Cancelled',
			sub: windowShortLabel,
			value: stats ? formatNumber(stats.cancelled_runs) : '—',
			icon: Ban,
			color: 'warning'
		},
		{
			id: 'failed',
			label: 'Failed',
			sub: windowShortLabel,
			value: stats ? formatNumber(stats.failed_runs) : '—',
			icon: AlertCircle,
			color: 'error'
		}
	]);

	const colorClasses: Record<string, string> = {
		primary: 'bg-primary-100 text-primary-600 dark:bg-primary-900/30 dark:text-primary-400',
		success: 'bg-success-100 text-success-600 dark:bg-success-900/30 dark:text-success-400',
		secondary: 'bg-secondary-100 text-secondary-600 dark:bg-secondary-800 dark:text-secondary-400',
		warning: 'bg-warning-100 text-warning-600 dark:bg-warning-900/30 dark:text-warning-500',
		error: 'bg-error-100 text-error-600 dark:bg-error-900/30 dark:text-error-400'
	};
</script>

<div class="space-y-6">
	<div class="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
		<div>
			<h1 class="text-2xl font-bold text-[var(--text-primary)]">Dashboard</h1>
			<p class="mt-1 text-[var(--text-secondary)]">
				Overview of your CI/CD platform.
			</p>
		</div>

		<div class="flex flex-wrap items-center gap-3">
			<div class="flex flex-col gap-1">
				<span class="text-xs font-medium text-[var(--text-secondary)]">Metrics period</span>
				<div class="w-[11rem]">
					<Select
						id="dashboard-window"
						options={windowOptions}
						bind:value={timeWindow}
						size="sm"
						onchange={() => loadDashboard()}
					/>
				</div>
			</div>
			<Button variant="ghost" size="sm" onclick={loadDashboard} loading={loading} title="Refresh now (also auto-refreshes every 30s)">
				<RefreshCw class="h-4 w-4" />
			</Button>
			<Button variant="outline" href="/pipelines">
				<GitBranch class="h-4 w-4" />
				Pipelines
			</Button>
			<Button variant="primary" href="/projects">
				<Play class="h-4 w-4" />
				New Run
			</Button>
		</div>
	</div>

	{#if error}
		<div class="rounded-lg border border-error-200 bg-error-50 p-4 text-sm text-error-700 dark:border-error-800 dark:bg-error-900/20 dark:text-error-400">
			{error}
		</div>
	{/if}

	{#if stats || loading}
		<div
			class="flex flex-wrap items-center gap-x-4 gap-y-2 rounded-xl border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm sm:gap-x-6 sm:px-4"
		>
			{#if loading}
				<Skeleton class="h-4 w-16 rounded" />
				<Skeleton class="h-4 w-20 rounded" />
				<Skeleton class="h-4 w-28 rounded" />
				<Skeleton class="h-4 w-24 rounded" />
			{:else if stats}
				<a
					href="/projects"
					class="group inline-flex items-baseline gap-1.5 text-[var(--text-secondary)] transition-colors hover:text-[var(--text-primary)]"
				>
					<span class="text-xs text-[var(--text-tertiary)] group-hover:text-[var(--text-secondary)]">Projects</span>
					<span class="font-semibold tabular-nums text-[var(--text-primary)]">{stats.projects_count}</span>
				</a>
				<span class="hidden h-3 w-px shrink-0 bg-[var(--border-primary)] sm:block" aria-hidden="true"></span>
				<a
					href="/pipelines"
					class="group inline-flex items-baseline gap-1.5 text-[var(--text-secondary)] transition-colors hover:text-[var(--text-primary)]"
				>
					<span class="text-xs text-[var(--text-tertiary)] group-hover:text-[var(--text-secondary)]">Pipelines</span>
					<span class="font-semibold tabular-nums text-[var(--text-primary)]">{stats.pipelines_count}</span>
				</a>
				<span class="hidden h-3 w-px shrink-0 bg-[var(--border-primary)] sm:block" aria-hidden="true"></span>
				<a
					href="/agents"
					class="group inline-flex items-baseline gap-1.5 text-[var(--text-secondary)] transition-colors hover:text-[var(--text-primary)]"
					title="{stats.agents_online} of {stats.agents_total} agents online"
				>
					<span class="text-xs text-[var(--text-tertiary)] group-hover:text-[var(--text-secondary)]">Agents</span>
					<span class="font-semibold tabular-nums text-[var(--text-primary)]">
						{stats.agents_online}/{stats.agents_total}
					</span>
					<span class="text-xs font-normal text-[var(--text-tertiary)]">online</span>
				</a>
				<span class="hidden h-3 w-px shrink-0 bg-[var(--border-primary)] sm:block" aria-hidden="true"></span>
				<div
					class="inline-flex items-baseline gap-1.5 text-[var(--text-secondary)]"
					title="Average duration of succeeded runs in this period"
				>
					<span class="text-xs text-[var(--text-tertiary)]">Avg time</span>
					<span class="font-semibold tabular-nums text-[var(--text-primary)]">{formatDurationMs(stats.avg_duration_ms)}</span>
				</div>
			{/if}
		</div>
	{/if}

	<div
		class="grid grid-cols-1 gap-3 sm:grid-cols-2 sm:gap-4 md:grid-cols-3 xl:grid-cols-5"
	>
		{#each statCards as stat (stat.id)}
			{@const Icon = stat.icon}
			<Card class="min-h-[5.5rem]">
				<div class="flex items-start justify-between gap-3">
					<div class="min-w-0 flex-1">
						<span class="text-sm font-medium leading-snug text-[var(--text-secondary)]">
							{stat.label}
						</span>
						<p class="mt-1 line-clamp-2 text-xs leading-snug text-[var(--text-tertiary)]">
							{stat.sub}
						</p>
					</div>
					<div class="shrink-0 rounded-lg p-2 {colorClasses[stat.color]}">
						<Icon class="h-4 w-4" />
					</div>
				</div>

				<div class="mt-3 min-h-8">
					{#if loading}
						<Skeleton class="h-8 w-20" />
					{:else}
						<span class="text-2xl font-bold text-[var(--text-primary)]">
							{stat.value}
						</span>
					{/if}
				</div>
			</Card>
		{/each}
	</div>

	<Card padding="sm" class="overflow-hidden">
		<div class="flex items-center justify-between gap-2 border-b border-[var(--border-primary)] px-3 py-2 sm:px-4">
			<h2 class="text-sm font-semibold text-[var(--text-primary)]">
				Recent runs
				<span class="ml-1.5 font-normal text-[var(--text-tertiary)]">({windowShortLabel})</span>
			</h2>
			<Button variant="ghost" size="sm" href="/runs" class="shrink-0 !px-2 text-xs">
				All runs
				<ArrowUpRight class="h-3.5 w-3.5" />
			</Button>
		</div>

		<div class="px-1 py-1 sm:px-2">
			{#if loading}
				<div class="divide-y divide-[var(--border-primary)]">
					{#each Array(recentRunsLimit) as _, i (i)}
						<div class="flex items-center gap-2 px-2 py-2">
							<Skeleton class="h-8 w-8 shrink-0 rounded-md" />
							<div class="min-w-0 flex-1 space-y-1.5">
								<Skeleton class="h-3.5 w-40" />
								<Skeleton class="h-3 w-24" />
							</div>
							<Skeleton class="h-5 w-14 shrink-0 rounded-full" />
						</div>
					{/each}
				</div>
			{:else if recentRuns.length === 0}
				<div class="px-3 py-6 text-center">
					<p class="text-sm text-[var(--text-secondary)]">No runs in this period.</p>
					<Button variant="outline" size="sm" class="mt-3" href="/pipelines">View pipelines</Button>
				</div>
			{:else}
				<div class="divide-y divide-[var(--border-primary)]">
					{#each recentRuns as run (run.id)}
						<button
							type="button"
							class="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left transition-colors hover:bg-[var(--bg-hover)] sm:gap-3 sm:py-2"
							onclick={() => goto(`/runs/${run.id}`)}
						>
							<div
								class="flex h-8 w-8 shrink-0 items-center justify-center rounded-md bg-[var(--bg-tertiary)] sm:h-7 sm:w-7"
							>
								<GitBranch class="h-3.5 w-3.5 text-[var(--text-secondary)] sm:h-3 sm:w-3" />
							</div>
							<div class="min-w-0 flex-1">
								<p class="truncate text-sm font-medium text-[var(--text-primary)]">
									{run.pipeline_name}
									<span class="font-normal text-[var(--text-tertiary)]"> · #{run.run_number}</span>
								</p>
								<p class="truncate text-xs text-[var(--text-secondary)]">
									{formatRunTriggeredBy(run.triggered_by, run.webhook_remote_addr)}
									<span class="text-[var(--text-tertiary)]"> · {formatRelativeTime(run.created_at)}</span>
								</p>
							</div>
							<StatusBadge status={run.status} size="sm" class="shrink-0 scale-90 sm:scale-100" />
						</button>
					{/each}
				</div>
			{/if}
		</div>
	</Card>
</div>
