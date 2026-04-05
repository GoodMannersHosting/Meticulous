<script lang="ts">
	import { Button, Card, StatusBadge } from '$components/ui';
	import { Skeleton, EmptyState } from '$components/data';
	import { apiMethods } from '$api/client';
	import type { JobQueueEntry } from '$api/client';
	import { formatRelativeTime } from '$utils/format';
	import { ListOrdered, RefreshCw, ExternalLink } from 'lucide-svelte';

	let rows = $state<JobQueueEntry[]>([]);
	let count = $state(0);
	let loading = $state(true);
	let error = $state<string | null>(null);

	const limit = 300;

	async function loadQueue() {
		loading = true;
		error = null;
		try {
			const res = await apiMethods.admin.ops.jobQueue({ limit });
			rows = res.data;
			count = res.count;
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load job queue';
			rows = [];
			count = 0;
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		loadQueue();
	});
</script>

<div class="space-y-4">
	<div class="flex flex-wrap items-center justify-between gap-3">
		<div class="flex items-center gap-3">
			<div
				class="flex h-10 w-10 items-center justify-center rounded-lg bg-[var(--bg-primary)] text-[var(--text-secondary)]"
			>
				<ListOrdered class="h-5 w-5" />
			</div>
			<div>
				<h2 class="text-lg font-semibold text-[var(--text-primary)]">Job queue</h2>
				<p class="text-sm text-[var(--text-secondary)]">
					Shows <strong>pending</strong> / <strong>queued</strong> job runs waiting for an agent, and runs still
					<strong>pending</strong> before any job rows exist.
				</p>
			</div>
		</div>
		<Button variant="secondary" size="sm" onclick={() => loadQueue()} disabled={loading}>
			<RefreshCw class="mr-1.5 h-4 w-4 {loading ? 'animate-spin' : ''}" />
			Refresh
		</Button>
	</div>

	{#if error}
		<div
			class="rounded-lg border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-800 dark:border-red-900 dark:bg-red-950/40 dark:text-red-200"
		>
			{error}
		</div>
	{/if}

	<Card class="overflow-hidden p-0">
		{#if loading && rows.length === 0}
			<div class="space-y-2 p-4">
				<Skeleton class="h-10 w-full" />
				<Skeleton class="h-10 w-full" />
				<Skeleton class="h-10 w-full" />
			</div>
		{:else if rows.length === 0}
			<div class="p-8">
				<EmptyState
					title="No queued jobs"
					description="When jobs are waiting for an agent, they appear here with project, pipeline, and run context."
				/>
			</div>
		{:else}
			<div class="overflow-x-auto">
				<table class="w-full min-w-[720px] text-left text-sm">
					<thead class="border-b border-[var(--border-primary)] bg-[var(--bg-secondary)]">
						<tr>
							<th class="px-4 py-3 font-medium text-[var(--text-secondary)]">Job</th>
							<th class="px-4 py-3 font-medium text-[var(--text-secondary)]">Status</th>
							<th class="px-4 py-3 font-medium text-[var(--text-secondary)]">Project</th>
							<th class="px-4 py-3 font-medium text-[var(--text-secondary)]">Pipeline</th>
							<th class="px-4 py-3 font-medium text-[var(--text-secondary)]">Run</th>
							<th class="px-4 py-3 font-medium text-[var(--text-secondary)]">Waiting since</th>
							<th class="px-4 py-3 font-medium text-[var(--text-secondary)]"></th>
						</tr>
					</thead>
					<tbody class="divide-y divide-[var(--border-primary)]">
						{#each rows as row (`${row.run_id}-${row.job_run_id ?? 'run'}`)}
							<tr class="bg-[var(--bg-primary)] hover:bg-[var(--bg-hover)]">
								<td class="px-4 py-3 font-medium text-[var(--text-primary)]">
									{row.job_name}
									{#if row.attempt > 1}
										<span class="ml-1 text-xs text-[var(--text-tertiary)]">(attempt {row.attempt})</span>
									{/if}
								</td>
								<td class="px-4 py-3">
									<StatusBadge status={row.job_status} size="sm" />
								</td>
								<td class="px-4 py-3 text-[var(--text-secondary)]">{row.project_slug}</td>
								<td class="px-4 py-3 text-[var(--text-secondary)]">{row.pipeline_name}</td>
								<td class="px-4 py-3 text-[var(--text-secondary)]">
									#{row.run_number}
									<span class="text-xs text-[var(--text-tertiary)]"> · run {row.run_status}</span>
								</td>
								<td class="px-4 py-3 text-[var(--text-secondary)]" title={row.job_run_created_at}>
									{formatRelativeTime(row.job_run_created_at)}
								</td>
								<td class="px-4 py-3 text-right">
									<a
										href="/runs/{row.run_id}"
										class="inline-flex items-center gap-1 text-primary-600 hover:text-primary-500 dark:text-primary-400"
									>
										Open
										<ExternalLink class="h-3.5 w-3.5 opacity-70" />
									</a>
								</td>
							</tr>
						{/each}
					</tbody>
				</table>
			</div>
			<div
				class="border-t border-[var(--border-primary)] bg-[var(--bg-secondary)] px-4 py-2 text-xs text-[var(--text-tertiary)]"
			>
				Showing {count} job run(s) (limit {limit}).
			</div>
		{/if}
	</Card>
</div>
