<script lang="ts" module>
	export interface DagJob {
		name: string;
		depends_on?: string[];
		status?: string;
		executed_binaries?: {
			binary_path: string;
			sha256: string;
			execution_count: number;
		}[];
	}

	export interface DagViewerProps {
		jobs: DagJob[];
		class?: string;
	}
</script>

<script lang="ts">
	import { StatusBadge, CopyButton } from '$components/ui';

	let { jobs, class: className = '' }: DagViewerProps = $props();

	interface LayoutNode {
		job: DagJob;
		x: number;
		y: number;
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

	const NODE_WIDTH = 160;
	const NODE_HEIGHT = 60;
	const LEVEL_GAP = 100;
	const NODE_GAP = 20;

	function computeLayout(jobs: DagJob[]): { nodes: LayoutNode[]; edges: Edge[]; width: number; height: number } {
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

		const maxLevel = Math.max(...Array.from(levels.values()));
		const maxNodesInLevel = Math.max(...Array.from(levelGroups.values()).map((g) => g.length));

		const nodes: LayoutNode[] = [];
		const nodePositions = new Map<string, { x: number; y: number }>();

		for (let level = 0; level <= maxLevel; level++) {
			const group = levelGroups.get(level) ?? [];
			const groupWidth = group.length * NODE_WIDTH + (group.length - 1) * NODE_GAP;
			const startX = (maxNodesInLevel * NODE_WIDTH + (maxNodesInLevel - 1) * NODE_GAP - groupWidth) / 2;

			group.forEach((job, index) => {
				const x = startX + index * (NODE_WIDTH + NODE_GAP) + NODE_WIDTH / 2;
				const y = level * (NODE_HEIGHT + LEVEL_GAP) + NODE_HEIGHT / 2;
				nodes.push({ job, x, y, level });
				nodePositions.set(job.name, { x, y });
			});
		}

		const edges: Edge[] = [];
		jobs.forEach((job) => {
			if (job.depends_on) {
				job.depends_on.forEach((dep) => {
					const from = nodePositions.get(dep);
					const to = nodePositions.get(job.name);
					if (from && to) {
						edges.push({
							from: dep,
							to: job.name,
							fromX: from.x,
							fromY: from.y + NODE_HEIGHT / 2,
							toX: to.x,
							toY: to.y - NODE_HEIGHT / 2
						});
					}
				});
			}
		});

		const width = maxNodesInLevel * NODE_WIDTH + (maxNodesInLevel - 1) * NODE_GAP + 40;
		const height = (maxLevel + 1) * (NODE_HEIGHT + LEVEL_GAP);

		return { nodes, edges, width, height };
	}

	const layout = $derived(computeLayout(jobs));

	const jobsWithBinaries = $derived(
		jobs.filter((j) => j.executed_binaries && j.executed_binaries.length > 0)
	);

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
</script>

<div class="overflow-auto rounded-lg bg-[var(--bg-tertiary)] p-4 {className}">
	<svg
		width={layout.width}
		height={layout.height}
		viewBox="0 0 {layout.width} {layout.height}"
		class="mx-auto"
	>
		<defs>
			<marker
				id="arrowhead"
				markerWidth="10"
				markerHeight="7"
				refX="9"
				refY="3.5"
				orient="auto"
			>
				<polygon
					points="0 0, 10 3.5, 0 7"
					fill="var(--color-secondary-400)"
				/>
			</marker>
		</defs>

		{#each layout.edges as edge (edge.from + '-' + edge.to)}
			<path
				d="M {edge.fromX} {edge.fromY} C {edge.fromX} {edge.fromY + 40}, {edge.toX} {edge.toY - 40}, {edge.toX} {edge.toY}"
				fill="none"
				stroke="var(--color-secondary-400)"
				stroke-width="2"
				marker-end="url(#arrowhead)"
			/>
		{/each}

		{#each layout.nodes as node (node.job.name)}
			<g transform="translate({node.x - NODE_WIDTH / 2}, {node.y - NODE_HEIGHT / 2})">
				<rect
					width={NODE_WIDTH}
					height={NODE_HEIGHT}
					rx="8"
					fill="var(--bg-secondary)"
					stroke={getStatusColor(node.job.status)}
					stroke-width="2"
					class="transition-colors"
				/>
				<text
					x={NODE_WIDTH / 2}
					y={NODE_HEIGHT / 2 - 6}
					text-anchor="middle"
					dominant-baseline="middle"
					class="fill-[var(--text-primary)] text-sm font-medium"
				>
					{node.job.name.length > 18 ? node.job.name.slice(0, 16) + '...' : node.job.name}
				</text>
				{#if node.job.status}
					<text
						x={NODE_WIDTH / 2}
						y={NODE_HEIGHT / 2 + 12}
						text-anchor="middle"
						dominant-baseline="middle"
						class="fill-[var(--text-secondary)] text-xs capitalize"
					>
						{node.job.status}
					</text>
				{/if}
			</g>
		{/each}
	</svg>

	{#if jobsWithBinaries.length > 0}
		<div class="mt-6 space-y-6 border-t border-[var(--border-primary)] pt-6">
			<h3 class="text-sm font-semibold text-[var(--text-primary)]">Executed binaries (SHA-256)</h3>
			<p class="text-xs text-[var(--text-secondary)]">
				Unique executables observed for this run, aggregated per job. Count is exec events (same path and
				digest).
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
									<th class="pb-2 font-medium text-right">Exec count</th>
								</tr>
							</thead>
							<tbody class="text-[var(--text-primary)]">
								{#each job.executed_binaries ?? [] as row (row.sha256 + row.binary_path)}
									<tr class="border-b border-[var(--border-secondary)]/80 align-top">
										<td class="py-2 pr-3 font-mono break-all">{row.binary_path}</td>
										<td class="py-2 pr-3">
											<div class="flex flex-wrap items-center gap-1">
												<span class="font-mono break-all">{row.sha256}</span>
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
