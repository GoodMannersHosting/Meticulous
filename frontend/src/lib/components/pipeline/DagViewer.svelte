<script lang="ts" module>
	export interface DagJob {
		name: string;
		depends_on?: string[];
		status?: string;
		job_run_id?: string | null;
		executed_binaries?: {
			binary_path: string;
			sha256: string;
			execution_count: number;
		}[];
	}

	export interface DagViewerProps {
		jobs: DagJob[];
		/** When set, clicking a node loads workflow steps via the runs API. */
		runId?: string | null;
		class?: string;
	}
</script>

<script lang="ts">
	import { browser } from '$app/environment';
	import { Button, StatusBadge, CopyButton } from '$components/ui';
	import { apiMethods } from '$api/client';
	import type { StepRun } from '$api/types';
	import { ZoomIn, ZoomOut, Maximize2, X, GitBranch } from 'lucide-svelte';

	let { jobs, runId = null, class: className = '' }: DagViewerProps = $props();

	interface LayoutNode {
		job: DagJob;
		cx: number;
		cy: number;
		level: number;
	}

	interface Edge {
		from: string;
		to: string;
		fromX: number;
		fromY: number;
		toX: number;
		toY: number;
	}

	const NODE_WIDTH = 168;
	const NODE_HEIGHT = 56;
	const COL_GAP = 72;
	const ROW_GAP = 24;
	const PADDING = 28;

	/** Left-to-right DAG: `level` is column index (X), rows stack on Y within the column. */
	function computeLayoutLR(jobs: DagJob[]): {
		nodes: LayoutNode[];
		edges: Edge[];
		width: number;
		height: number;
	} {
		const jobMap = new Map<string, DagJob>();
		jobs.forEach((j) => jobMap.set(j.name, j));

		const levels = new Map<string, number>();

		function getLevel(name: string): number {
			if (levels.has(name)) return levels.get(name)!;
			const job = jobMap.get(name);
			if (!job || !job.depends_on || job.depends_on.length === 0) {
				levels.set(name, 0);
				return 0;
			}
			const maxParentLevel = Math.max(...job.depends_on.map(getLevel));
			const level = maxParentLevel + 1;
			levels.set(name, level);
			return level;
		}

		jobs.forEach((j) => getLevel(j.name));

		const levelGroups = new Map<number, DagJob[]>();
		jobs.forEach((j) => {
			const level = levels.get(j.name)!;
			if (!levelGroups.has(level)) levelGroups.set(level, []);
			levelGroups.get(level)!.push(j);
		});

		const maxLevel = Math.max(0, ...Array.from(levels.values()));
		let maxColHeight = 0;
		for (let c = 0; c <= maxLevel; c++) {
			const g = levelGroups.get(c) ?? [];
			const h = g.length * NODE_HEIGHT + (g.length > 0 ? (g.length - 1) * ROW_GAP : 0);
			maxColHeight = Math.max(maxColHeight, h);
		}

		const nodes: LayoutNode[] = [];
		const nodeCenters = new Map<string, { cx: number; cy: number }>();

		for (let level = 0; level <= maxLevel; level++) {
			const group = levelGroups.get(level) ?? [];
			const colHeight = group.length * NODE_HEIGHT + (group.length > 0 ? (group.length - 1) * ROW_GAP : 0);
			const yStart = PADDING + (maxColHeight - colHeight) / 2;
			const cx = PADDING + level * (NODE_WIDTH + COL_GAP) + NODE_WIDTH / 2;

			group.forEach((job, index) => {
				const cy = yStart + index * (NODE_HEIGHT + ROW_GAP) + NODE_HEIGHT / 2;
				nodes.push({ job, cx, cy, level });
				nodeCenters.set(job.name, { cx, cy });
			});
		}

		const edges: Edge[] = [];
		jobs.forEach((job) => {
			if (!job.depends_on) return;
			job.depends_on.forEach((dep) => {
				const from = nodeCenters.get(dep);
				const to = nodeCenters.get(job.name);
				if (from && to) {
					edges.push({
						from: dep,
						to: job.name,
						fromX: from.cx + NODE_WIDTH / 2,
						fromY: from.cy,
						toX: to.cx - NODE_WIDTH / 2,
						toY: to.cy
					});
				}
			});
		});

		const width =
			PADDING * 2 + (maxLevel + 1) * NODE_WIDTH + maxLevel * COL_GAP;
		const height = PADDING * 2 + maxColHeight;

		return { nodes, edges, width, height };
	}

	const layout = $derived(computeLayoutLR(jobs));

	let scale = $state(1);
	let panX = $state(0);
	let panY = $state(0);
	let selectedName = $state<string | null>(null);
	let panning = $state(false);
	let panStartX = $state(0);
	let panStartY = $state(0);
	let panOriginX = $state(0);
	let panOriginY = $state(0);

	let stepRuns = $state<StepRun[]>([]);
	let stepsLoading = $state(false);
	let stepsError = $state<string | null>(null);

	const selectedJob = $derived(jobs.find((j) => j.name === selectedName) ?? null);

	const jobsWithBinaries = $derived(
		jobs.filter((j) => j.executed_binaries && j.executed_binaries.length > 0)
	);

	const graphTopologySig = $derived(
		`${runId ?? ''}::${jobs
			.map((j) => `${j.name}|${(j.depends_on ?? []).slice().sort().join(',')}`)
			.sort()
			.join(';;')}`
	);

	$effect(() => {
		// Reset view when the run or DAG topology changes (not when only statuses refresh).
		void graphTopologySig;
		scale = 1;
		panX = 0;
		panY = 0;
		selectedName = null;
	});

	$effect(() => {
		if (!browser) return;
		const job = selectedJob;
		const rid = runId?.trim();
		const jrid = job?.job_run_id?.trim();
		if (!job || !rid || !jrid) {
			stepRuns = [];
			stepsLoading = false;
			stepsError = null;
			return;
		}
		let cancelled = false;
		stepsLoading = true;
		stepsError = null;
		stepRuns = [];
		void apiMethods.runs
			.jobSteps(rid, jrid)
			.then((rows) => {
				if (!cancelled) stepRuns = rows;
			})
			.catch((e) => {
				if (!cancelled) {
					stepsError = e instanceof Error ? e.message : 'Failed to load steps';
					stepRuns = [];
				}
			})
			.finally(() => {
				if (!cancelled) stepsLoading = false;
			});
		return () => {
			cancelled = true;
		};
	});

	function getStatusColor(status?: string): string {
		if (!status) return 'var(--color-secondary-300)';
		switch (status.toLowerCase()) {
			case 'running':
				return 'var(--color-primary-500)';
			case 'succeeded':
				return 'var(--color-success-500)';
			case 'failed':
				return 'var(--color-error-500)';
			case 'cancelled':
			case 'skipped':
				return 'var(--color-secondary-400)';
			default:
				return 'var(--color-secondary-300)';
		}
	}

	function resetView() {
		scale = 1;
		panX = 0;
		panY = 0;
	}

	function zoomIn() {
		scale = Math.min(3, scale * 1.2);
	}
	function zoomOut() {
		scale = Math.max(0.2, scale / 1.2);
	}

	function onWheel(e: WheelEvent) {
		e.preventDefault();
		const direction = e.deltaY < 0 ? 1 : -1;
		const factor = direction > 0 ? 1.08 : 1 / 1.08;
		scale = Math.min(3, Math.max(0.2, scale * factor));
	}

	function onPointerDownSurface(e: PointerEvent) {
		// Middle button, or Alt+primary — avoids fighting plain click node selection.
		const panGesture = e.button === 1 || (e.button === 0 && e.altKey);
		if (!panGesture) return;
		e.preventDefault();
		(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
		panning = true;
		panStartX = e.clientX;
		panStartY = e.clientY;
		panOriginX = panX;
		panOriginY = panY;
	}

	function onPointerMoveSurface(e: PointerEvent) {
		if (!panning) return;
		panX = panOriginX + (e.clientX - panStartX);
		panY = panOriginY + (e.clientY - panStartY);
	}

	function onPointerUpSurface(e: PointerEvent) {
		if (panning) {
			try {
				(e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
			} catch {
				/* ignore */
			}
		}
		panning = false;
	}

	function selectNode(job: DagJob) {
		selectedName = job.name;
	}

	function closePanel() {
		selectedName = null;
	}

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === 'Escape') closePanel();
	}
</script>

<svelte:window onkeydown={handleKeydown} />

<div
	class="flex min-h-[min(70vh,520px)] flex-col rounded-lg border border-[var(--border-primary)] bg-[var(--bg-tertiary)] {className}"
>
	<div
		class="flex flex-wrap items-center gap-2 border-b border-[var(--border-primary)] bg-[var(--bg-secondary)]/60 px-3 py-2"
	>
		<div class="flex items-center gap-1">
			<Button variant="outline" size="sm" onclick={zoomOut} title="Zoom out">
				<ZoomOut class="h-4 w-4" />
			</Button>
			<Button variant="outline" size="sm" onclick={zoomIn} title="Zoom in">
				<ZoomIn class="h-4 w-4" />
			</Button>
			<Button variant="outline" size="sm" onclick={resetView} title="Reset pan & zoom">
				<Maximize2 class="h-4 w-4" />
			</Button>
		</div>
		<span class="text-xs text-[var(--text-tertiary)]">
			Wheel: zoom · Middle-drag or Alt+drag: pan · Click a job: steps
		</span>
	</div>

	<div class="flex min-h-0 flex-1">
		<div
			class="relative min-h-[360px] min-w-0 flex-1 touch-none overflow-hidden"
			role="application"
			aria-label="Pipeline graph"
			onwheel={onWheel}
			onpointerdown={onPointerDownSurface}
			onpointermove={onPointerMoveSurface}
			onpointerup={onPointerUpSurface}
			onpointercancel={onPointerUpSurface}
		>
			<div
				class="absolute inset-0 flex items-center justify-center {panning ? 'cursor-grabbing' : ''}"
				style="transform: translate({panX}px, {panY}px) scale({scale}); transform-origin: center center;"
			>
				<svg
					width={layout.width}
					height={layout.height}
					viewBox="0 0 {layout.width} {layout.height}"
					class="max-h-none max-w-none select-none"
				>
					<defs>
						<marker
							id="dag-arrow-lr"
							markerWidth="10"
							markerHeight="7"
							refX="9"
							refY="3.5"
							orient="auto"
						>
							<polygon points="0 0, 10 3.5, 0 7" fill="var(--color-secondary-400)" />
						</marker>
					</defs>

					{#each layout.edges as edge (edge.from + '-' + edge.to)}
						<path
							d="M {edge.fromX} {edge.fromY} C {edge.fromX + 48} {edge.fromY}, {edge.toX - 48} {edge.toY}, {edge.toX} {edge.toY}"
							fill="none"
							stroke="var(--color-secondary-400)"
							stroke-width="2"
							class="pointer-events-none"
							marker-end="url(#dag-arrow-lr)"
						/>
					{/each}

					{#each layout.nodes as node (node.job.name)}
						<g
							transform="translate({node.cx - NODE_WIDTH / 2}, {node.cy - NODE_HEIGHT / 2})"
							class="cursor-pointer"
							role="button"
							tabindex="0"
							aria-label="Job {node.job.name}"
							onclick={(ev) => {
								ev.stopPropagation();
								selectNode(node.job);
							}}
							onkeydown={(ev) => {
								if (ev.key === 'Enter' || ev.key === ' ') {
									ev.preventDefault();
									selectNode(node.job);
								}
							}}
						>
							<rect
								width={NODE_WIDTH}
								height={NODE_HEIGHT}
								rx="8"
								fill="var(--bg-secondary)"
								stroke={getStatusColor(node.job.status)}
								stroke-width={selectedName === node.job.name ? 3 : 2}
								class="transition-colors"
							/>
							<text
								x={NODE_WIDTH / 2}
								y={NODE_HEIGHT / 2 - 8}
								text-anchor="middle"
								dominant-baseline="middle"
								class="pointer-events-none fill-[var(--text-primary)] text-xs font-medium"
							>
								{node.job.name.length > 22
									? node.job.name.slice(0, 20) + '…'
									: node.job.name}
							</text>
							{#if node.job.status}
								<text
									x={NODE_WIDTH / 2}
									y={NODE_HEIGHT / 2 + 10}
									text-anchor="middle"
									dominant-baseline="middle"
									class="pointer-events-none fill-[var(--text-secondary)] text-[0.65rem] capitalize"
								>
									{node.job.status}
								</text>
							{/if}
						</g>
					{/each}
				</svg>
			</div>
		</div>

		{#if selectedJob}
			<aside
				class="flex w-[min(100%,20rem)] shrink-0 flex-col border-l border-[var(--border-primary)] bg-[var(--bg-secondary)]"
			>
				<div class="flex items-start justify-between gap-2 border-b border-[var(--border-primary)] p-3">
					<div class="min-w-0">
						<p class="flex items-center gap-1 text-[0.65rem] font-medium uppercase tracking-wide text-[var(--text-tertiary)]">
							<GitBranch class="h-3 w-3" />
							Workflow
						</p>
						<p class="mt-1 text-sm font-medium leading-snug text-[var(--text-primary)]">
							{selectedJob.name}
						</p>
						{#if selectedJob.status}
							<div class="mt-2">
								<StatusBadge status={selectedJob.status} size="sm" showIcon={true} />
							</div>
						{/if}
					</div>
					<Button variant="ghost" size="sm" onclick={closePanel} title="Close">
						<X class="h-4 w-4" />
					</Button>
				</div>
				<div class="flex-1 overflow-y-auto p-3">
					{#if !runId}
						<p class="text-xs text-[var(--text-secondary)]">Run id missing; cannot load steps.</p>
					{:else if !selectedJob.job_run_id}
						<p class="text-xs text-[var(--text-secondary)]">
							No job run id for this node yet (pending or not materialized for this run).
						</p>
					{:else if stepsLoading && stepRuns.length === 0}
						<p class="text-xs text-[var(--text-secondary)]">Loading steps…</p>
					{:else if stepsError}
						<p class="text-xs text-rose-500">{stepsError}</p>
					{:else if stepRuns.length === 0}
						<p class="text-xs text-[var(--text-secondary)]">No steps recorded for this job run.</p>
					{:else}
						<p class="mb-2 text-xs font-medium text-[var(--text-secondary)]">Steps</p>
						<ol class="space-y-2">
							{#each stepRuns as st (st.id)}
								<li
									class="rounded-lg border border-[var(--border-secondary)] bg-[var(--bg-tertiary)]/80 px-2.5 py-2"
								>
									<div class="flex flex-wrap items-center gap-2">
										<StatusBadge status={st.status} size="sm" showIcon={true} />
										<span class="text-sm font-medium text-[var(--text-primary)]">{st.step_name}</span>
									</div>
									{#if st.exit_code != null}
										<p class="mt-1 font-mono text-[0.65rem] text-[var(--text-tertiary)]">
											exit {st.exit_code}
										</p>
									{/if}
								</li>
							{/each}
						</ol>
					{/if}
				</div>
			</aside>
		{/if}
	</div>

	{#if jobsWithBinaries.length > 0}
		<div class="space-y-6 border-t border-[var(--border-primary)] p-4">
			<h3 class="text-sm font-semibold text-[var(--text-primary)]">Executed binaries (SHA-256)</h3>
			<p class="text-xs text-[var(--text-secondary)]">
				Unique executables observed for this run, aggregated per job. Count is exec events (same path and digest).
			</p>
			{#each jobsWithBinaries as job (job.name)}
				<div class="rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] p-4">
					<h4 class="mb-3 text-sm font-medium text-[var(--text-primary)]">{job.name}</h4>
					<div class="overflow-x-auto">
						<table class="w-full min-w-[32rem] text-left text-xs">
							<thead>
								<tr class="border-b border-[var(--border-secondary)] text-[var(--text-tertiary)]">
									<th class="pb-2 pr-3 font-medium">Path</th>
									<th class="pb-2 pr-3 font-medium">SHA-256</th>
									<th class="pb-2 text-right font-medium">Exec count</th>
								</tr>
							</thead>
							<tbody class="text-[var(--text-primary)]">
								{#each job.executed_binaries ?? [] as row (row.sha256 + row.binary_path)}
									<tr class="border-b border-[var(--border-secondary)]/80 align-top">
										<td class="break-all py-2 pr-3 font-mono">{row.binary_path}</td>
										<td class="py-2 pr-3">
											<div class="flex flex-wrap items-center gap-1">
												<span class="break-all font-mono">{row.sha256}</span>
												{#if row.sha256}
													<CopyButton text={row.sha256} size="sm" />
												{/if}
											</div>
										</td>
										<td class="py-2 text-right tabular-nums">{row.execution_count}</td>
									</tr>
								{/each}
							</tbody>
						</table>
					</div>
				</div>
			{/each}
		</div>
	{/if}
</div>
