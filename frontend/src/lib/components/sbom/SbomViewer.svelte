<script lang="ts" module>
	export interface SbomPackage {
		name: string;
		version: string;
		license?: string;
		ecosystem: string;
		direct: boolean;
	}

	export interface SbomDiff {
		added: SbomPackage[];
		removed: SbomPackage[];
		updated: Array<{
			name: string;
			ecosystem: string;
			from_version: string;
			to_version: string;
		}>;
	}

	export interface SbomViewerProps {
		packages?: SbomPackage[];
		diff?: SbomDiff;
		class?: string;
	}
</script>

<script lang="ts">
	import { Badge, Input, Card } from '$components/ui';
	import { Skeleton } from '$components/data';
	import { Package, Plus, Minus, ArrowRight, Search, Filter } from 'lucide-svelte';

	let { packages, diff, class: className = '' }: SbomViewerProps = $props();

	let searchQuery = $state('');
	let selectedEcosystem = $state<string>('all');

	const ecosystems = $derived(() => {
		if (packages) {
			return ['all', ...new Set(packages.map((p) => p.ecosystem))];
		}
		if (diff) {
			const allPackages = [...diff.added, ...diff.removed, ...diff.updated.map((u) => ({ ecosystem: u.ecosystem }))];
			return ['all', ...new Set(allPackages.map((p) => p.ecosystem))];
		}
		return ['all'];
	});

	const filteredPackages = $derived(() => {
		if (!packages) return [];
		return packages.filter((p) => {
			const matchesSearch = !searchQuery || 
				p.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
				p.version.toLowerCase().includes(searchQuery.toLowerCase());
			const matchesEcosystem = selectedEcosystem === 'all' || p.ecosystem === selectedEcosystem;
			return matchesSearch && matchesEcosystem;
		});
	});

	const ecosystemColors: Record<string, string> = {
		cargo: 'bg-orange-100 text-orange-700 dark:bg-orange-900/30 dark:text-orange-400',
		npm: 'bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400',
		pip: 'bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400',
		go: 'bg-cyan-100 text-cyan-700 dark:bg-cyan-900/30 dark:text-cyan-400',
		maven: 'bg-purple-100 text-purple-700 dark:bg-purple-900/30 dark:text-purple-400'
	};

	function getEcosystemClass(ecosystem: string): string {
		return ecosystemColors[ecosystem.toLowerCase()] ?? 'bg-secondary-100 text-secondary-700 dark:bg-secondary-800 dark:text-secondary-300';
	}
</script>

<div class="space-y-4 {className}">
	{#if diff}
		<div class="grid gap-4 sm:grid-cols-3">
			<Card padding="sm">
				<div class="flex items-center gap-3">
					<div class="flex h-10 w-10 items-center justify-center rounded-lg bg-success-100 dark:bg-success-900/30">
						<Plus class="h-5 w-5 text-success-600 dark:text-success-400" />
					</div>
					<div>
						<p class="text-2xl font-bold text-[var(--text-primary)]">{diff.added.length}</p>
						<p class="text-sm text-[var(--text-secondary)]">Added</p>
					</div>
				</div>
			</Card>
			<Card padding="sm">
				<div class="flex items-center gap-3">
					<div class="flex h-10 w-10 items-center justify-center rounded-lg bg-error-100 dark:bg-error-900/30">
						<Minus class="h-5 w-5 text-error-600 dark:text-error-400" />
					</div>
					<div>
						<p class="text-2xl font-bold text-[var(--text-primary)]">{diff.removed.length}</p>
						<p class="text-sm text-[var(--text-secondary)]">Removed</p>
					</div>
				</div>
			</Card>
			<Card padding="sm">
				<div class="flex items-center gap-3">
					<div class="flex h-10 w-10 items-center justify-center rounded-lg bg-warning-100 dark:bg-warning-900/30">
						<ArrowRight class="h-5 w-5 text-warning-600 dark:text-warning-400" />
					</div>
					<div>
						<p class="text-2xl font-bold text-[var(--text-primary)]">{diff.updated.length}</p>
						<p class="text-sm text-[var(--text-secondary)]">Updated</p>
					</div>
				</div>
			</Card>
		</div>

		{#if diff.added.length > 0}
			<div>
				<h3 class="mb-2 flex items-center gap-2 font-medium text-success-600 dark:text-success-400">
					<Plus class="h-4 w-4" />
					Added Packages
				</h3>
				<div class="rounded-lg border border-success-200 bg-success-50/50 dark:border-success-800 dark:bg-success-900/10">
					{#each diff.added as pkg (pkg.name + pkg.ecosystem)}
						<div class="flex items-center justify-between border-b border-success-200 px-4 py-2 last:border-0 dark:border-success-800">
							<div class="flex items-center gap-3">
								<Package class="h-4 w-4 text-success-600 dark:text-success-400" />
								<span class="font-medium">{pkg.name}</span>
								<Badge variant="secondary" size="sm">{pkg.version}</Badge>
							</div>
							<span class="text-xs px-2 py-0.5 rounded {getEcosystemClass(pkg.ecosystem)}">{pkg.ecosystem}</span>
						</div>
					{/each}
				</div>
			</div>
		{/if}

		{#if diff.removed.length > 0}
			<div>
				<h3 class="mb-2 flex items-center gap-2 font-medium text-error-600 dark:text-error-400">
					<Minus class="h-4 w-4" />
					Removed Packages
				</h3>
				<div class="rounded-lg border border-error-200 bg-error-50/50 dark:border-error-800 dark:bg-error-900/10">
					{#each diff.removed as pkg (pkg.name + pkg.ecosystem)}
						<div class="flex items-center justify-between border-b border-error-200 px-4 py-2 last:border-0 dark:border-error-800">
							<div class="flex items-center gap-3">
								<Package class="h-4 w-4 text-error-600 dark:text-error-400" />
								<span class="font-medium line-through opacity-75">{pkg.name}</span>
								<Badge variant="secondary" size="sm">{pkg.version}</Badge>
							</div>
							<span class="text-xs px-2 py-0.5 rounded {getEcosystemClass(pkg.ecosystem)}">{pkg.ecosystem}</span>
						</div>
					{/each}
				</div>
			</div>
		{/if}

		{#if diff.updated.length > 0}
			<div>
				<h3 class="mb-2 flex items-center gap-2 font-medium text-warning-600 dark:text-warning-400">
					<ArrowRight class="h-4 w-4" />
					Updated Packages
				</h3>
				<div class="rounded-lg border border-warning-200 bg-warning-50/50 dark:border-warning-800 dark:bg-warning-900/10">
					{#each diff.updated as update (update.name + update.ecosystem)}
						<div class="flex items-center justify-between border-b border-warning-200 px-4 py-2 last:border-0 dark:border-warning-800">
							<div class="flex items-center gap-3">
								<Package class="h-4 w-4 text-warning-600 dark:text-warning-400" />
								<span class="font-medium">{update.name}</span>
								<div class="flex items-center gap-1">
									<Badge variant="secondary" size="sm">{update.from_version}</Badge>
									<ArrowRight class="h-3 w-3 text-[var(--text-tertiary)]" />
									<Badge variant="primary" size="sm">{update.to_version}</Badge>
								</div>
							</div>
							<span class="text-xs px-2 py-0.5 rounded {getEcosystemClass(update.ecosystem)}">{update.ecosystem}</span>
						</div>
					{/each}
				</div>
			</div>
		{/if}
	{:else if packages}
		<div class="flex gap-4">
			<div class="relative flex-1">
				<Search class="absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-[var(--text-tertiary)]" />
				<Input
					type="search"
					placeholder="Search packages..."
					class="pl-10"
					bind:value={searchQuery}
				/>
			</div>
			<select
				bind:value={selectedEcosystem}
				class="rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm"
			>
				{#each ecosystems() as eco (eco)}
					<option value={eco}>{eco === 'all' ? 'All ecosystems' : eco}</option>
				{/each}
			</select>
		</div>

		<div class="rounded-lg border border-[var(--border-primary)]">
			<table class="w-full text-sm">
				<thead class="bg-[var(--bg-tertiary)]">
					<tr>
						<th class="px-4 py-3 text-left font-medium text-[var(--text-secondary)]">Package</th>
						<th class="px-4 py-3 text-left font-medium text-[var(--text-secondary)]">Version</th>
						<th class="px-4 py-3 text-left font-medium text-[var(--text-secondary)]">Ecosystem</th>
						<th class="px-4 py-3 text-left font-medium text-[var(--text-secondary)]">License</th>
						<th class="px-4 py-3 text-left font-medium text-[var(--text-secondary)]">Type</th>
					</tr>
				</thead>
				<tbody class="divide-y divide-[var(--border-secondary)]">
					{#each filteredPackages() as pkg (pkg.name + pkg.ecosystem)}
						<tr class="bg-[var(--bg-secondary)]">
							<td class="px-4 py-3">
								<div class="flex items-center gap-2">
									<Package class="h-4 w-4 text-[var(--text-tertiary)]" />
									<span class="font-medium">{pkg.name}</span>
								</div>
							</td>
							<td class="px-4 py-3 font-mono text-sm">{pkg.version}</td>
							<td class="px-4 py-3">
								<span class="text-xs px-2 py-0.5 rounded {getEcosystemClass(pkg.ecosystem)}">{pkg.ecosystem}</span>
							</td>
							<td class="px-4 py-3 text-[var(--text-secondary)]">{pkg.license ?? '—'}</td>
							<td class="px-4 py-3">
								{#if pkg.direct}
									<Badge variant="primary" size="sm">Direct</Badge>
								{:else}
									<Badge variant="secondary" size="sm">Transitive</Badge>
								{/if}
							</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>

		<p class="text-sm text-[var(--text-tertiary)]">
			Showing {filteredPackages().length} of {packages.length} packages
		</p>
	{/if}
</div>
