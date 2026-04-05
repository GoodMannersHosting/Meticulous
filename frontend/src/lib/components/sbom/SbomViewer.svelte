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
		/** Raw SPDX/CycloneDX JSON when the API returns `sbom`. */
		rawDocument?: Record<string, unknown> | null;
		/** API `format` field (`cyclonedx`, `spdx`, `json`) for labeling. */
		apiFormat?: string;
		/** Run id for default export filename. */
		runId?: string;
		/** When true, show a neutral empty state (no mock data). */
		empty?: boolean;
		class?: string;
	}
</script>

<script lang="ts">
	import { browser } from '$app/environment';
	import { Badge, Input, Card, Button } from '$components/ui';
	import { Skeleton } from '$components/data';
	import { Package, Plus, Minus, ArrowRight, Search, Download, FileJson } from 'lucide-svelte';
	import { parseSbomDocument, sbomExportFilename, type SbomDocumentKind } from '$lib/utils/sbomParse';

	let {
		packages,
		diff,
		rawDocument,
		apiFormat,
		runId,
		empty = false,
		class: className = ''
	}: SbomViewerProps = $props();

	let searchQuery = $state('');
	let selectedEcosystem = $state<string>('all');
	let rawViewMode = $state<'pretty' | 'raw'>('pretty');

	const parsedFromRaw = $derived(
		rawDocument && typeof rawDocument === 'object' && Object.keys(rawDocument).length > 0
			? parseSbomDocument(rawDocument)
			: null
	);

	function exportKindForFile(): SbomDocumentKind {
		if (!parsedFromRaw) return 'json';
		return parsedFromRaw.kind;
	}

	function downloadRawDocument() {
		if (!browser || !rawDocument) return;
		const kind = exportKindForFile();
		const filename = sbomExportFilename(kind, runId);
		const blob = new Blob([JSON.stringify(rawDocument, null, 2)], {
			type: 'application/json'
		});
		const url = URL.createObjectURL(blob);
		const a = document.createElement('a');
		a.href = url;
		a.download = filename;
		a.rel = 'noopener';
		a.click();
		URL.revokeObjectURL(url);
	}

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
		return filterPkgList(packages, searchQuery, selectedEcosystem);
	});

	const ecosystemColors: Record<string, string> = {
		cargo: 'bg-orange-100 text-orange-700 dark:bg-orange-900/30 dark:text-orange-400',
		npm: 'bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400',
		pip: 'bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400',
		go: 'bg-cyan-100 text-cyan-700 dark:bg-cyan-900/30 dark:text-cyan-400',
		maven: 'bg-purple-100 text-purple-700 dark:bg-purple-900/30 dark:text-purple-400',
		nuget: 'bg-indigo-100 text-indigo-800 dark:bg-indigo-900/30 dark:text-indigo-300',
		gem: 'bg-rose-100 text-rose-800 dark:bg-rose-900/30 dark:text-rose-300',
		hex: 'bg-violet-100 text-violet-800 dark:bg-violet-900/30 dark:text-violet-300'
	};

	function getEcosystemClass(ecosystem: string): string {
		return ecosystemColors[ecosystem.toLowerCase()] ?? 'bg-secondary-100 text-secondary-700 dark:bg-secondary-800 dark:text-secondary-300';
	}

	function filterPkgList(list: SbomPackage[], q: string, eco: string): SbomPackage[] {
		return list.filter((p) => {
			const matchesSearch =
				!q ||
				p.name.toLowerCase().includes(q.toLowerCase()) ||
				p.version.toLowerCase().includes(q.toLowerCase());
			const matchesEcosystem = eco === 'all' || p.ecosystem === eco;
			return matchesSearch && matchesEcosystem;
		});
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
	{:else if rawDocument && Object.keys(rawDocument).length > 0 && parsedFromRaw}
		<div class="flex flex-col gap-3 sm:flex-row sm:flex-wrap sm:items-center sm:justify-between">
			<div class="flex flex-wrap items-center gap-2">
				<span class="text-xs font-medium uppercase tracking-wide text-[var(--text-tertiary)]">View</span>
				<div class="inline-flex rounded-lg border border-[var(--border-primary)] p-0.5">
					<button
						type="button"
						class="rounded-md px-3 py-1.5 text-sm font-medium transition-colors {rawViewMode === 'pretty'
							? 'bg-[var(--bg-tertiary)] text-[var(--text-primary)] shadow-sm'
							: 'text-[var(--text-secondary)] hover:text-[var(--text-primary)]'}"
						onclick={() => (rawViewMode = 'pretty')}
					>
						Pretty
					</button>
					<button
						type="button"
						class="rounded-md px-3 py-1.5 text-sm font-medium transition-colors {rawViewMode === 'raw'
							? 'bg-[var(--bg-tertiary)] text-[var(--text-primary)] shadow-sm'
							: 'text-[var(--text-secondary)] hover:text-[var(--text-primary)]'}"
						onclick={() => (rawViewMode = 'raw')}
					>
						Raw JSON
					</button>
				</div>
			</div>
			<Button variant="outline" size="sm" onclick={downloadRawDocument}>
				<Download class="h-4 w-4" />
				Export {sbomExportFilename(exportKindForFile(), runId)}
			</Button>
		</div>

		{#if rawViewMode === 'raw'}
			<Card padding="sm">
				<h3 class="mb-2 flex items-center gap-2 text-sm font-medium text-[var(--text-primary)]">
					<FileJson class="h-4 w-4 text-[var(--text-tertiary)]" />
					SBOM document
				</h3>
				<pre
					class="max-h-[28rem] overflow-auto rounded-md border border-[var(--border-primary)] bg-[var(--bg-tertiary)] p-3 text-xs text-[var(--text-primary)]"
				>{JSON.stringify(rawDocument, null, 2)}</pre>
			</Card>
		{:else}
			<div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
				<Card padding="sm">
					<p class="text-xs font-medium text-[var(--text-secondary)]">Format</p>
					<p class="mt-1">
						<Badge variant="secondary" size="sm" class="uppercase">
							{apiFormat && apiFormat !== 'json' ? apiFormat : parsedFromRaw.formatLabel}
						</Badge>
					</p>
				</Card>
				<Card padding="sm">
					<p class="text-xs font-medium text-[var(--text-secondary)]">Spec version</p>
					<p class="mt-1 font-mono text-sm text-[var(--text-primary)]">
						{parsedFromRaw.specVersion ?? '—'}
					</p>
				</Card>
				<Card padding="sm">
					<p class="text-xs font-medium text-[var(--text-secondary)]">Components</p>
					<p class="mt-1 text-2xl font-semibold text-[var(--text-primary)]">
						{parsedFromRaw.componentCount}
					</p>
				</Card>
				<Card padding="sm">
					<p class="text-xs font-medium text-[var(--text-secondary)]">Root</p>
					<p
						class="mt-1 truncate text-sm text-[var(--text-primary)]"
						title={parsedFromRaw.rootName ? `${parsedFromRaw.rootName} ${parsedFromRaw.rootVersion ?? ''}` : ''}
					>
						{#if parsedFromRaw.rootName}
							<span class="font-medium">{parsedFromRaw.rootName}</span>
							{#if parsedFromRaw.rootVersion}
								<span class="font-mono text-[var(--text-tertiary)]">@{parsedFromRaw.rootVersion}</span>
							{/if}
						{:else}
							—
						{/if}
					</p>
				</Card>
			</div>

			{#if parsedFromRaw.kind === 'json' && parsedFromRaw.packages.length === 0}
				<Card padding="sm">
					<p class="text-sm font-medium text-[var(--text-primary)]">No structured component list</p>
					<p class="mt-2 text-sm text-[var(--text-secondary)]">
						This JSON is not recognized as CycloneDX or SPDX. Use <strong>Raw JSON</strong> to inspect
						the file, or <strong>Export</strong> to download it.
					</p>
				</Card>
			{:else}
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
						{#each ['all', ...new Set(parsedFromRaw.packages.map((p) => p.ecosystem))] as eco (eco)}
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
							{#each filterPkgList(parsedFromRaw.packages, searchQuery, selectedEcosystem) as pkg (pkg.name + pkg.version + pkg.ecosystem)}
								<tr class="bg-[var(--bg-secondary)]">
									<td class="px-4 py-3">
										<div class="flex items-center gap-2">
											<Package class="h-4 w-4 text-[var(--text-tertiary)]" />
											<span class="font-medium">{pkg.name}</span>
										</div>
									</td>
									<td class="px-4 py-3 font-mono text-sm">{pkg.version}</td>
									<td class="px-4 py-3">
										<span class="rounded px-2 py-0.5 text-xs {getEcosystemClass(pkg.ecosystem)}"
											>{pkg.ecosystem}</span
										>
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
					Showing {filterPkgList(parsedFromRaw.packages, searchQuery, selectedEcosystem).length} of
					{parsedFromRaw.packages.length} packages
				</p>
			{/if}
		{/if}
	{:else if empty}
		<div
			class="rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-6 py-12 text-center text-sm text-[var(--text-secondary)]"
		>
			<p class="font-medium text-[var(--text-primary)]">No SBOM for this run</p>
			<p class="mt-2">
				Nothing has been uploaded or generated yet. When your pipeline produces an SPDX or CycloneDX SBOM, it
				will appear here.
			</p>
		</div>
	{/if}
</div>
