<script lang="ts">
	import { Badge, Button, Card, StatusBadge } from '$components/ui';
	import { Skeleton, EmptyState } from '$components/data';
	import { apiMethods, getWebSocketManager, createConnectionStateStore } from '$api';
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
		RefreshCw,
		Wifi,
		WifiOff
	} from 'lucide-svelte';
	import { onMount } from 'svelte';

	let stats = $state<DashboardStats | null>(null);
	let recentRuns = $state<RecentRun[]>([]);
	let agents = $state<Agent[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);

	const connectionState = createConnectionStateStore();

	onMount(() => {
		loadDashboard();
		const ws = getWebSocketManager();
		ws?.connect();

		const interval = setInterval(loadDashboard, 30000);
		return () => clearInterval(interval);
	});

	async function loadDashboard() {
		loading = true;
		error = null;
		try {
			const [statsRes, runsRes, agentsRes] = await Promise.all([
				apiMethods.dashboard.stats().catch(() => null),
				apiMethods.dashboard.recentRuns(10).catch(() => []),
				apiMethods.agents.list({ per_page: 5 }).catch(() => ({ items: [] }))
			]);

			stats = statsRes ?? {
				active_runs: 0,
				completed_today: 0,
				failed_today: 0,
				avg_duration_ms: 0,
				agents_online: 0,
				agents_total: 0,
				pipelines_count: 0,
				projects_count: 0
			};
			recentRuns = runsRes ?? [];
			agents = agentsRes.items ?? [];
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load dashboard';
		} finally {
			loading = false;
		}
	}

	const statCards = $derived([
		{
			label: 'Active Runs',
			value: stats ? formatNumber(stats.active_runs) : '—',
			icon: Activity,
			color: 'primary'
		},
		{
			label: 'Completed Today',
			value: stats ? formatNumber(stats.completed_today) : '—',
			icon: CheckCircle2,
			color: 'success'
		},
		{
			label: 'Avg Duration',
			value: stats ? formatDurationMs(stats.avg_duration_ms) : '—',
			icon: Clock,
			color: 'secondary'
		},
		{
			label: 'Failed Today',
			value: stats ? formatNumber(stats.failed_today) : '—',
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

		<div class="flex items-center gap-3">
			<div class="flex items-center gap-2 text-sm text-[var(--text-secondary)]">
				{#if connectionState.current === 'connected'}
					<Wifi class="h-4 w-4 text-success-500" />
					<span>Live</span>
				{:else if connectionState.current === 'connecting' || connectionState.current === 'reconnecting'}
					<Wifi class="h-4 w-4 animate-pulse text-warning-500" />
					<span>Connecting...</span>
				{:else}
					<WifiOff class="h-4 w-4 text-secondary-400" />
					<span>Offline</span>
				{/if}
			</div>
			<Button variant="ghost" size="sm" onclick={loadDashboard} loading={loading}>
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
				<div class="flex items-center justify-between">
					<span class="text-sm font-medium text-[var(--text-secondary)]">
						{stat.label}
					</span>
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
				<h2 class="font-semibold text-[var(--text-primary)]">Recent Runs</h2>
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
