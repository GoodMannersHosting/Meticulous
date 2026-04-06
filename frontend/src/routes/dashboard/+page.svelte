<script lang="ts">
	import { Button, Card, StatusBadge, Select } from '$components/ui';
	import type { SelectOption } from '$components/ui';
	import { Skeleton, EmptyState } from '$components/data';
	import { apiMethods } from '$api';
	import type { DashboardStats, RecentRun, Agent } from '$api';
	import { formatRelativeTime, formatDurationMs, formatNumber } from '$utils/format';
	import { goto } from '$app/navigation';
	import {
		Activity,
		CheckCircle2,
		Clock,
		AlertCircle,
		Server,
		GitBranch,
		ArrowUpRight,
		Play,
		RefreshCw
	} from 'lucide-svelte';
	import { onMount } from 'svelte';

	let stats = $state<DashboardStats | null>(null);
	let recentRuns = $state<RecentRun[]>([]);
	let agents = $state<Agent[]>([]);
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

	onMount(() => {
		loadDashboard();

		const interval = setInterval(loadDashboard, 30000);
		return () => clearInterval(interval);
	});

	async function loadDashboard() {
		loading = true;
		error = null;
		try {
			const [statsRes, runsRes, agentsRes] = await Promise.all([
				apiMethods.dashboard.stats(timeWindow),
				apiMethods.dashboard.recentRuns(10, timeWindow),
				apiMethods.agents.list({ per_page: 5 })
			]);

			stats = statsRes;
			recentRuns = runsRes;
			agents = agentsRes.data ?? [];
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load dashboard';
			stats = null;
			recentRuns = [];
			agents = [];
		} finally {
			loading = false;
		}
	}

	const statCards = $derived([
		{
			label: 'Active Runs',
			sub: 'Now (all statuses in progress)',
			value: stats ? formatNumber(stats.active_runs) : '—',
			icon: Activity,
			color: 'primary'
		},
		{
			label: 'Completed',
			sub: windowShortLabel,
			value: stats ? formatNumber(stats.completed_runs) : '—',
			icon: CheckCircle2,
			color: 'success'
		},
		{
			label: 'Avg Duration',
			sub: `Succeeded runs · ${windowShortLabel}`,
			value: stats ? formatDurationMs(stats.avg_duration_ms) : '—',
			icon: Clock,
			color: 'secondary'
		},
		{
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

	<div class="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
		{#each statCards as stat (stat.label)}
			{@const Icon = stat.icon}
			<Card>
				<div class="flex items-center justify-between gap-2">
					<div>
						<span class="text-sm font-medium text-[var(--text-secondary)]">
							{stat.label}
						</span>
						<p class="mt-0.5 text-xs text-[var(--text-tertiary)]">{stat.sub}</p>
					</div>
					<div class="rounded-lg p-2 {colorClasses[stat.color]}">
						<Icon class="h-4 w-4" />
					</div>
				</div>

				<div class="mt-3">
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

	<div class="grid gap-6 lg:grid-cols-2">
		<Card>
			<div class="flex items-center justify-between">
				<h2 class="font-semibold text-[var(--text-primary)]">
					Recent Runs
					<span class="ml-2 text-xs font-normal text-[var(--text-tertiary)]">({windowShortLabel})</span>
				</h2>
				<Button variant="ghost" size="sm" href="/runs">
					View all
					<ArrowUpRight class="h-4 w-4" />
				</Button>
			</div>

			<div class="mt-4">
				{#if loading}
					<div class="space-y-3">
						{#each Array(5) as _, i (i)}
							<div class="flex items-center gap-3">
								<Skeleton class="h-10 w-10 rounded-lg" />
								<div class="flex-1 space-y-2">
									<Skeleton class="h-4 w-32" />
									<Skeleton class="h-3 w-24" />
								</div>
								<Skeleton class="h-6 w-16 rounded-full" />
							</div>
						{/each}
					</div>
				{:else if recentRuns.length === 0}
					<EmptyState
						title="No recent runs"
						description="Trigger a pipeline to see runs here."
					>
						<Button variant="primary" size="sm" href="/pipelines">
							View Pipelines
						</Button>
					</EmptyState>
				{:else}
					<div class="space-y-3">
						{#each recentRuns as run (run.id)}
							<button
								type="button"
								class="flex w-full items-center gap-3 rounded-lg p-2 text-left transition-colors hover:bg-[var(--bg-hover)]"
								onclick={() => goto(`/runs/${run.id}`)}
							>
								<div class="flex h-10 w-10 items-center justify-center rounded-lg bg-[var(--bg-tertiary)]">
									<GitBranch class="h-5 w-5 text-[var(--text-secondary)]" />
								</div>
								<div class="flex-1 min-w-0">
									<p class="truncate font-medium text-[var(--text-primary)]">
										{run.pipeline_name}
									</p>
									<p class="text-sm text-[var(--text-secondary)]">
										#{run.run_number} • {run.triggered_by}
									</p>
								</div>
								<div class="flex flex-col items-end gap-1">
									<StatusBadge status={run.status} size="sm" />
									<span class="text-xs text-[var(--text-tertiary)]">
										{formatRelativeTime(run.created_at)}
									</span>
								</div>
							</button>
						{/each}
					</div>
				{/if}
			</div>
		</Card>

		<Card>
			<div class="flex items-center justify-between">
				<h2 class="font-semibold text-[var(--text-primary)]">Agents</h2>
				<Button variant="ghost" size="sm" href="/agents">
					View all
					<ArrowUpRight class="h-4 w-4" />
				</Button>
			</div>

			<div class="mt-4">
				{#if loading}
					<div class="space-y-3">
						{#each Array(3) as _, i (i)}
							<div class="flex items-center gap-3">
								<Skeleton class="h-10 w-10 rounded-full" />
								<div class="flex-1 space-y-2">
									<Skeleton class="h-4 w-28" />
									<Skeleton class="h-3 w-20" />
								</div>
								<Skeleton class="h-6 w-16 rounded-full" />
							</div>
						{/each}
					</div>
				{:else if agents.length === 0}
					<EmptyState
						title="No agents"
						description="Connect an agent to start running jobs."
					/>
				{:else}
					<div class="space-y-3">
						{#each agents as agent (agent.id)}
							<div class="flex items-center gap-3 rounded-lg p-2">
								<div class="flex h-10 w-10 items-center justify-center rounded-full bg-[var(--bg-tertiary)]">
									<Server class="h-5 w-5 text-[var(--text-secondary)]" />
								</div>
								<div class="flex-1 min-w-0">
									<p class="truncate font-medium text-[var(--text-primary)]">
										{agent.name}
									</p>
									<p class="text-sm text-[var(--text-tertiary)]">
										{agent.running_jobs}/{agent.max_jobs} jobs
									</p>
								</div>
								<StatusBadge status={agent.status} size="sm" />
							</div>
						{/each}
					</div>
				{/if}
			</div>

			{#if stats}
				<div class="mt-4 border-t border-[var(--border-primary)] pt-4">
					<div class="flex items-center justify-between text-sm">
						<span class="text-[var(--text-secondary)]">Total Agents</span>
						<span class="font-medium text-[var(--text-primary)]">
							{stats.agents_online} / {stats.agents_total} online
						</span>
					</div>
				</div>
			{/if}
		</Card>
	</div>

	{#if stats}
		<div class="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
			<Card>
				<div class="text-center">
					<p class="text-3xl font-bold text-[var(--text-primary)]">{stats.projects_count}</p>
					<p class="mt-1 text-sm text-[var(--text-secondary)]">Projects</p>
				</div>
			</Card>
			<Card>
				<div class="text-center">
					<p class="text-3xl font-bold text-[var(--text-primary)]">{stats.pipelines_count}</p>
					<p class="mt-1 text-sm text-[var(--text-secondary)]">Pipelines</p>
				</div>
			</Card>
			<Card>
				<div class="text-center">
					<p class="text-3xl font-bold text-[var(--text-primary)]">{stats.agents_online}</p>
					<p class="mt-1 text-sm text-[var(--text-secondary)]">Online Agents</p>
				</div>
			</Card>
			<Card>
				<div class="text-center">
					<p class="text-3xl font-bold text-[var(--text-primary)]">{formatDurationMs(stats.avg_duration_ms)}</p>
					<p class="mt-1 text-sm text-[var(--text-secondary)]">Avg Run Time</p>
				</div>
			</Card>
		</div>
	{/if}
</div>
