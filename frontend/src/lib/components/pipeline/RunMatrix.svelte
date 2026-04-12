<script lang="ts">
	import type { MatrixResponse, MatrixCell, MatrixEnvironment } from '$lib/api/types';
	import { goto } from '$app/navigation';
	import { onMount, onDestroy } from 'svelte';

	interface Props {
		data: MatrixResponse;
		onrefresh?: () => void;
	}

	let { data, onrefresh }: Props = $props();

	let hoverCell = $state<MatrixCell | null>(null);
	let hoverPos = $state({ x: 0, y: 0 });
	let hoverTimer: ReturnType<typeof setTimeout> | null = null;
	let showPopover = $state(false);
	let refreshInterval: ReturnType<typeof setInterval> | null = null;

	const hasRunning = $derived(
		data.cells.some((c) => c.status === 'running' || c.status === 'queued' || c.status === 'pending')
	);

	onMount(() => {
		refreshInterval = setInterval(() => {
			if (hasRunning && onrefresh) onrefresh();
		}, 15000);
	});

	onDestroy(() => {
		if (refreshInterval) clearInterval(refreshInterval);
		if (hoverTimer) clearTimeout(hoverTimer);
	});

	function cellForWfEnv(workflow: string, env: MatrixEnvironment): MatrixCell | undefined {
		return data.cells.find(
			(c) => c.workflow === workflow && (env.id === null ? c.environment === null : c.environment === env.name)
		);
	}

	function statusColor(status: string | null): string {
		switch (status) {
			case 'succeeded':
				return 'bg-green-500/20 border-green-500/50 text-green-400';
			case 'failed':
				return 'bg-red-500/20 border-red-500/50 text-red-400';
			case 'running':
				return 'bg-blue-500/20 border-blue-500/50 text-blue-400';
			case 'queued':
			case 'pending':
				return 'bg-zinc-500/20 border-zinc-500/50 text-zinc-400';
			case 'cancelled':
				return 'bg-amber-500/20 border-amber-500/50 text-amber-400';
			default:
				return 'border-dashed border-zinc-700 text-zinc-600';
		}
	}

	function statusLabel(status: string | null): string {
		if (!status) return '';
		return status.charAt(0).toUpperCase() + status.slice(1);
	}

	function formatDuration(ms: number | null): string {
		if (!ms || ms <= 0) return '';
		const s = Math.round(ms / 1000);
		if (s < 60) return `${s}s`;
		const m = Math.floor(s / 60);
		return `${m}m ${s % 60}s`;
	}

	function tierBadgeClass(tier: string): string {
		switch (tier) {
			case 'production':
				return 'bg-red-500/10 text-red-400';
			case 'staging':
				return 'bg-amber-500/10 text-amber-400';
			case 'development':
				return 'bg-green-500/10 text-green-400';
			default:
				return 'bg-zinc-500/10 text-zinc-400';
		}
	}

	function onCellEnter(cell: MatrixCell | undefined, e: MouseEvent) {
		if (!cell?.run_id) return;
		hoverCell = cell;
		hoverPos = { x: e.clientX, y: e.clientY };
		if (hoverTimer) clearTimeout(hoverTimer);
		hoverTimer = setTimeout(() => {
			showPopover = true;
		}, 1200);
	}

	function onCellMove(e: MouseEvent) {
		hoverPos = { x: e.clientX, y: e.clientY };
	}

	function onCellLeave() {
		if (hoverTimer) clearTimeout(hoverTimer);
		hoverTimer = null;
		showPopover = false;
		hoverCell = null;
	}

	function onCellClick(cell: MatrixCell | undefined) {
		if (cell?.run_id) {
			goto(`/runs/${cell.run_id}`);
		}
	}
</script>

<div class="overflow-x-auto">
	<table class="w-full border-collapse text-sm">
		<thead>
			<tr>
				<th class="px-3 py-2 text-left text-xs font-medium text-[var(--text-secondary)] border-b border-[var(--border-primary)]">
					Workflow
				</th>
				{#each data.environments as env}
					<th class="px-3 py-2 text-center text-xs font-medium text-[var(--text-secondary)] border-b border-[var(--border-primary)]">
						<span class="inline-block rounded-full px-2 py-0.5 text-[10px] {tierBadgeClass(env.tier)}">
							{env.name}
						</span>
					</th>
				{/each}
			</tr>
		</thead>
		<tbody>
			{#each data.workflows as wf}
				<tr>
					<td class="px-3 py-2 font-mono text-xs text-[var(--text-primary)] border-b border-[var(--border-primary)]">
						{wf}
					</td>
					{#each data.environments as env}
						{@const cell = cellForWfEnv(wf, env)}
						<td class="px-1 py-1 border-b border-[var(--border-primary)]">
							<button
								class="w-full rounded-md border px-2 py-2 text-center text-xs transition-all hover:scale-105 cursor-pointer {statusColor(cell?.status ?? null)}"
								onmouseenter={(e) => onCellEnter(cell, e)}
								onmousemove={onCellMove}
								onmouseleave={onCellLeave}
								onclick={() => onCellClick(cell)}
							>
								{#if cell?.run_number}
									<span class="font-mono font-semibold">#{cell.run_number}</span>
									{#if cell.duration_ms}
										<span class="ml-1 opacity-60">{formatDuration(cell.duration_ms)}</span>
									{/if}
								{:else}
									<span class="opacity-30">—</span>
								{/if}
							</button>
						</td>
					{/each}
				</tr>
			{:else}
				<tr>
					<td colspan={data.environments.length + 1} class="px-4 py-8 text-center text-[var(--text-secondary)]">
						No workflow invocations found. Run the pipeline to populate the matrix.
					</td>
				</tr>
			{/each}
		</tbody>
	</table>
</div>

{#if showPopover && hoverCell}
	<div
		class="fixed z-50 pointer-events-none rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] p-3 shadow-xl text-sm min-w-[220px]"
		style="left: {hoverPos.x + 12}px; top: {hoverPos.y + 12}px;"
	>
		<div class="flex items-center gap-2 mb-2">
			<span class="font-mono font-semibold text-[var(--text-primary)]">Run #{hoverCell.run_number}</span>
			<span class="rounded-full px-2 py-0.5 text-[10px] font-medium {statusColor(hoverCell.status)}">
				{statusLabel(hoverCell.status)}
			</span>
		</div>
		<div class="space-y-1 text-xs text-[var(--text-secondary)]">
			{#if hoverCell.branch}
				<p>Branch: <span class="text-[var(--text-primary)]">{hoverCell.branch}</span></p>
			{/if}
			{#if hoverCell.triggered_by}
				<p>Triggered by: <span class="text-[var(--text-primary)]">{hoverCell.triggered_by}</span></p>
			{/if}
			{#if hoverCell.duration_ms}
				<p>Duration: <span class="text-[var(--text-primary)]">{formatDuration(hoverCell.duration_ms)}</span></p>
			{/if}
			{#if hoverCell.started_at}
				<p>Started: <span class="text-[var(--text-primary)]">{new Date(hoverCell.started_at).toLocaleString()}</span></p>
			{/if}
		</div>
		<p class="mt-2 text-[10px] text-[var(--text-tertiary)]">Click to view run details</p>
	</div>
{/if}
