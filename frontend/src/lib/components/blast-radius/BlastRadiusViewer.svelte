<script lang="ts" module>
	export interface BlastRadiusNode {
		id: string;
		name: string;
		type: 'package' | 'binary' | 'service' | 'file';
		impacted: boolean;
		direct: boolean;
	}

	export interface BlastRadiusEdge {
		from: string;
		to: string;
		type: 'depends' | 'produces' | 'uses';
	}

	export interface BlastRadiusData {
		changed_packages: string[];
		affected_nodes: BlastRadiusNode[];
		edges: BlastRadiusEdge[];
		impact_score: number;
	}

	export interface BlastRadiusViewerProps {
		data: BlastRadiusData;
		class?: string;
	}
</script>

<script lang="ts">
	import { Card, Badge, Progress } from '$components/ui';
	import { AlertTriangle, Package, Box, Server, FileCode, ArrowRight } from 'lucide-svelte';

	let { data, class: className = '' }: BlastRadiusViewerProps = $props();

	const NODE_SIZE = 48;
	const PADDING = 60;

	const typeIcons = {
		package: Package,
		binary: Box,
		service: Server,
		file: FileCode
	};

	const typeColors = {
		package: { bg: 'fill-primary-100 dark:fill-primary-900/30', stroke: 'stroke-primary-500' },
		binary: { bg: 'fill-success-100 dark:fill-success-900/30', stroke: 'stroke-success-500' },
		service: { bg: 'fill-warning-100 dark:fill-warning-900/30', stroke: 'stroke-warning-500' },
		file: { bg: 'fill-secondary-100 dark:fill-secondary-800', stroke: 'stroke-secondary-500' }
	};

	interface LayoutNode extends BlastRadiusNode {
		x: number;
		y: number;
		level: number;
	}

	function computeLayout(nodes: BlastRadiusNode[], edges: BlastRadiusEdge[]): LayoutNode[] {
		const nodeMap = new Map<string, BlastRadiusNode>();
		nodes.forEach((n) => nodeMap.set(n.id, n));

		const incomingEdges = new Map<string, Set<string>>();
		edges.forEach((e) => {
			if (!incomingEdges.has(e.to)) incomingEdges.set(e.to, new Set());
			incomingEdges.get(e.to)!.add(e.from);
		});

		const levels = new Map<string, number>();

		function getLevel(id: string, visited = new Set<string>()): number {
			if (levels.has(id)) return levels.get(id)!;
			if (visited.has(id)) return 0;
			visited.add(id);

			const incoming = incomingEdges.get(id);
			if (!incoming || incoming.size === 0) {
				levels.set(id, 0);
				return 0;
			}

			const maxParentLevel = Math.max(...Array.from(incoming).map((p) => getLevel(p, visited)));
			const level = maxParentLevel + 1;
			levels.set(id, level);
			return level;
		}

		nodes.forEach((n) => getLevel(n.id));

		const levelGroups = new Map<number, string[]>();
		nodes.forEach((n) => {
			const level = levels.get(n.id)!;
			if (!levelGroups.has(level)) levelGroups.set(level, []);
			levelGroups.get(level)!.push(n.id);
		});

		const maxLevel = Math.max(...Array.from(levels.values()));
		const maxNodesInLevel = Math.max(...Array.from(levelGroups.values()).map((g) => g.length));

		const result: LayoutNode[] = [];

		for (let level = 0; level <= maxLevel; level++) {
			const group = levelGroups.get(level) ?? [];
			const groupHeight = group.length * (NODE_SIZE + 20);
			const startY = (maxNodesInLevel * (NODE_SIZE + 20) - groupHeight) / 2;

			group.forEach((id, index) => {
				const node = nodeMap.get(id)!;
				result.push({
					...node,
					x: PADDING + level * 180 + NODE_SIZE / 2,
					y: PADDING + startY + index * (NODE_SIZE + 20) + NODE_SIZE / 2,
					level
				});
			});
		}

		return result;
	}

	const layout = $derived(computeLayout(data.affected_nodes, data.edges));
	const layoutMap = $derived(new Map(layout.map((n) => [n.id, n])));

	const width = $derived(
		Math.max(400, PADDING * 2 + (Math.max(...layout.map((n) => n.level)) + 1) * 180)
	);
	const height = $derived(
		Math.max(300, PADDING * 2 + layout.length * (NODE_SIZE + 20))
	);

	const impactSeverity = $derived(() => {
		if (data.impact_score >= 80) return { label: 'Critical', color: 'error' };
		if (data.impact_score >= 50) return { label: 'High', color: 'warning' };
		if (data.impact_score >= 20) return { label: 'Medium', color: 'primary' };
		return { label: 'Low', color: 'success' };
	});

	const directlyImpacted = $derived(data.affected_nodes.filter((n) => n.direct).length);
	const transitivelyImpacted = $derived(data.affected_nodes.filter((n) => n.impacted && !n.direct).length);
</script>

<div class="space-y-6 {className}">
	<div class="grid gap-4 sm:grid-cols-4">
		<Card padding="sm">
			<div class="flex items-center gap-3">
				<div class="flex h-10 w-10 items-center justify-center rounded-lg bg-error-100 dark:bg-error-900/30">
					<AlertTriangle class="h-5 w-5 text-error-600 dark:text-error-400" />
				</div>
				<div>
					<p class="text-2xl font-bold text-[var(--text-primary)]">{data.changed_packages.length}</p>
					<p class="text-sm text-[var(--text-secondary)]">Changed</p>
				</div>
			</div>
		</Card>
		<Card padding="sm">
			<div class="flex items-center gap-3">
				<div class="flex h-10 w-10 items-center justify-center rounded-lg bg-warning-100 dark:bg-warning-900/30">
					<Package class="h-5 w-5 text-warning-600 dark:text-warning-400" />
				</div>
				<div>
					<p class="text-2xl font-bold text-[var(--text-primary)]">{directlyImpacted}</p>
					<p class="text-sm text-[var(--text-secondary)]">Direct</p>
				</div>
			</div>
		</Card>
		<Card padding="sm">
			<div class="flex items-center gap-3">
				<div class="flex h-10 w-10 items-center justify-center rounded-lg bg-primary-100 dark:bg-primary-900/30">
					<ArrowRight class="h-5 w-5 text-primary-600 dark:text-primary-400" />
				</div>
				<div>
					<p class="text-2xl font-bold text-[var(--text-primary)]">{transitivelyImpacted}</p>
					<p class="text-sm text-[var(--text-secondary)]">Transitive</p>
				</div>
			</div>
		</Card>
		<Card padding="sm">
			<div>
				<div class="flex items-center justify-between">
					<span class="text-sm text-[var(--text-secondary)]">Impact Score</span>
					<Badge
						variant={impactSeverity().color as 'success' | 'warning' | 'error' | 'primary'}
						size="sm"
					>
						{impactSeverity().label}
					</Badge>
				</div>
				<p class="mt-1 text-2xl font-bold text-[var(--text-primary)]">{data.impact_score}%</p>
				<Progress
					value={data.impact_score}
					variant={impactSeverity().color as 'success' | 'warning' | 'error' | 'primary'}
					size="sm"
					class="mt-2"
				/>
			</div>
		</Card>
	</div>

	<Card>
		<h3 class="mb-4 font-medium text-[var(--text-primary)]">Dependency Impact Graph</h3>
		<div class="overflow-auto rounded-lg bg-[var(--bg-tertiary)] p-4">
			<svg {width} {height} viewBox="0 0 {width} {height}">
				<defs>
					<marker
						id="blast-arrowhead"
						markerWidth="10"
						markerHeight="7"
						refX="9"
						refY="3.5"
						orient="auto"
					>
						<polygon
							points="0 0, 10 3.5, 0 7"
							class="fill-[var(--color-secondary-400)]"
						/>
					</marker>
				</defs>

				{#each data.edges as edge (edge.from + '-' + edge.to)}
					{@const from = layoutMap.get(edge.from)}
					{@const to = layoutMap.get(edge.to)}
					{#if from && to}
						<path
							d="M {from.x + NODE_SIZE / 2} {from.y} L {to.x - NODE_SIZE / 2} {to.y}"
							fill="none"
							stroke="var(--color-secondary-400)"
							stroke-width="2"
							marker-end="url(#blast-arrowhead)"
						/>
					{/if}
				{/each}

				{#each layout as node (node.id)}
					{@const colors = typeColors[node.type]}
					<g transform="translate({node.x - NODE_SIZE / 2}, {node.y - NODE_SIZE / 2})">
						<rect
							width={NODE_SIZE}
							height={NODE_SIZE}
							rx="8"
							class="{colors.bg} {node.impacted ? 'stroke-error-500' : colors.stroke}"
							stroke-width={node.impacted ? 3 : 2}
						/>
						{#if node.direct}
							<circle
								cx={NODE_SIZE - 4}
								cy="4"
								r="6"
								class="fill-error-500"
							/>
						{/if}
					</g>
					<text
						x={node.x}
						y={node.y + NODE_SIZE / 2 + 16}
						text-anchor="middle"
						class="fill-[var(--text-primary)] text-xs font-medium"
					>
						{node.name.length > 12 ? node.name.slice(0, 10) + '...' : node.name}
					</text>
				{/each}
			</svg>
		</div>

		<div class="mt-4 flex flex-wrap gap-4 text-sm">
			<div class="flex items-center gap-2">
				<div class="h-3 w-3 rounded border-2 border-error-500 bg-error-100 dark:bg-error-900/30"></div>
				<span class="text-[var(--text-secondary)]">Impacted</span>
			</div>
			<div class="flex items-center gap-2">
				<div class="relative h-3 w-3">
					<div class="h-3 w-3 rounded border-2 border-primary-500 bg-primary-100 dark:bg-primary-900/30"></div>
					<div class="absolute -right-1 -top-1 h-2 w-2 rounded-full bg-error-500"></div>
				</div>
				<span class="text-[var(--text-secondary)]">Changed (root cause)</span>
			</div>
			{#each Object.entries(typeColors) as [type, colors] (type)}
				<div class="flex items-center gap-2">
					<div class="h-3 w-3 rounded {colors.bg.replace('fill-', 'bg-')} border-2 {colors.stroke.replace('stroke-', 'border-')}"></div>
					<span class="capitalize text-[var(--text-secondary)]">{type}</span>
				</div>
			{/each}
		</div>
	</Card>

	{#if data.changed_packages.length > 0}
		<Card>
			<h3 class="mb-3 font-medium text-[var(--text-primary)]">Changed Packages</h3>
			<div class="flex flex-wrap gap-2">
				{#each data.changed_packages as pkg (pkg)}
					<Badge variant="error">{pkg}</Badge>
				{/each}
			</div>
		</Card>
	{/if}
</div>
