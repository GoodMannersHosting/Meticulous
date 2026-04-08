<script lang="ts" module>
	import type {
		RunFootprintResponse,
		FootprintBinaryRow,
		FootprintNetworkRow,
		FootprintDirectoryGroup
	} from '$api/types';

	export interface RunFootprintViewerProps {
		data: RunFootprintResponse;
		class?: string;
	}

	interface JobLayer {
		jobName: string;
		binaries: FootprintBinaryRow[];
		network: FootprintNetworkRow[];
		directoryTouch: boolean;
	}
</script>

<script lang="ts">
	import { Card, CopyButton, Badge, Input } from '$components/ui';
	import { Terminal, Globe, FolderTree, Info, ChevronDown, Layers } from 'lucide-svelte';

	let { data, class: className = '' }: RunFootprintViewerProps = $props();

	let searchQuery = $state('');

	/** Agent placeholder for binaries detected from step script text (not measured from disk). */
	const SCRIPT_INFERRED_SHA =
		'0000000000000000000000000000000000000000000000000000000000000000';

	const JOB_SENTINEL = '—';

	function normJob(name: string | null | undefined): string {
		const t = (name ?? '').trim();
		return t || JOB_SENTINEL;
	}

	function jobPartFromDirectoryLabel(label: string): string {
		const t = label.trim();
		if (!t) return JOB_SENTINEL;
		const i = t.indexOf(' · ');
		return i >= 0 ? (t.slice(0, i).trim() || JOB_SENTINEL) : t;
	}

	const q = $derived(searchQuery.trim().toLowerCase());

	function textMatches(hay: string | null | undefined): boolean {
		if (!q) return true;
		return (hay ?? '').toLowerCase().includes(q);
	}

	function binaryMatches(row: FootprintBinaryRow): boolean {
		if (!q) return true;
		return (
			textMatches(row.job_name) ||
			textMatches(row.step_name) ||
			textMatches(row.binary_path) ||
			textMatches(row.sha256)
		);
	}

	function netMatches(row: FootprintNetworkRow): boolean {
		if (!q) return true;
		return (
			textMatches(row.job_name) ||
			textMatches(row.binary_path) ||
			textMatches(row.binary_sha256) ||
			textMatches(row.dst_ip) ||
			textMatches(String(row.dst_port)) ||
			textMatches(row.protocol) ||
			textMatches(row.direction) ||
			textMatches(row.connected_at)
		);
	}

	function directoryGroupMatches(group: FootprintDirectoryGroup): boolean {
		if (!q) return true;
		if (textMatches(group.directory)) return true;
		return group.entries.some(
			(e) =>
				textMatches(e.binary_path) ||
				textMatches(e.sha256) ||
				e.job_names.some((j) => textMatches(j))
		);
	}

	const filteredBinaries = $derived(data.executed_binaries.filter(binaryMatches));
	const filteredNet = $derived(data.network_connections.filter(netMatches));
	const filteredFilesystem = $derived(data.filesystem_by_directory.filter(directoryGroupMatches));

	const jobLayers = $derived.by((): JobLayer[] => {
		const order: string[] = [];
		const map = new Map<string, JobLayer>();

		function touch(name: string) {
			const key = normJob(name);
			if (!map.has(key)) {
				map.set(key, { jobName: key, binaries: [], network: [], directoryTouch: false });
				order.push(key);
			}
			return map.get(key)!;
		}

		for (const row of filteredBinaries) {
			touch(row.job_name).binaries.push(row);
		}
		for (const row of filteredNet) {
			touch(row.job_name ?? '').network.push(row);
		}
		for (const g of data.filesystem_by_directory) {
			for (const e of g.entries) {
				for (const lab of e.job_names) {
					touch(jobPartFromDirectoryLabel(lab)).directoryTouch = true;
				}
			}
		}

		function sentinelRank(k: string): number {
			return k === JOB_SENTINEL ? 1 : 0;
		}
		order.sort((a, b) => {
			const da = sentinelRank(a);
			const db = sentinelRank(b);
			if (da !== db) return da - db;
			return a.localeCompare(b);
		});

		for (const layer of map.values()) {
			layer.binaries.sort((a, b) => {
				const st = (a.step_name ?? '').localeCompare(b.step_name ?? '');
				if (st !== 0) return st;
				return a.binary_path.localeCompare(b.binary_path);
			});
			layer.network.sort((a, b) => a.connected_at.localeCompare(b.connected_at));
		}

		return order.map((k) => map.get(k)!);
	});

	const visibleJobLayers = $derived.by(() => {
		const hasQ = searchQuery.trim().length > 0;
		return jobLayers.filter((layer) => {
			const hasTelemetry = layer.binaries.length > 0 || layer.network.length > 0;
			if (hasTelemetry) return true;
			return !hasQ && layer.directoryTouch;
		});
	});

	const hasExec = $derived(data.executed_binaries.length > 0);
	const hasNet = $derived(data.network_connections.length > 0);
	const hasFs = $derived(data.filesystem_by_directory.length > 0);
	const empty = $derived(!hasExec && !hasNet && !hasFs);

	const hasFilteredExec = $derived(filteredBinaries.length > 0);
	const hasFilteredNet = $derived(filteredNet.length > 0);
	const hasFilteredFs = $derived(filteredFilesystem.length > 0);
	const filteredAllEmpty = $derived(!hasFilteredExec && !hasFilteredNet && !hasFilteredFs);

	const hasScriptInferred = $derived(
		data.executed_binaries.some((r) => r.sha256 === SCRIPT_INFERRED_SHA)
	);

	const runWideJobCount = $derived.by(() => {
		const s = new Set<string>();
		for (const row of data.executed_binaries) s.add(normJob(row.job_name));
		for (const row of data.network_connections) s.add(normJob(row.job_name));
		for (const g of data.filesystem_by_directory) {
			for (const e of g.entries) {
				for (const lab of e.job_names) s.add(jobPartFromDirectoryLabel(lab));
			}
		}
		return s.size;
	});

	const scriptInferredTitle =
		'Matched from the step script text only—not confirmed by process telemetry or a binary hash for this run. ' +
		'It may not have executed here (for example if a branch was skipped). ' +
		'It could still run under other inputs or environments. Worth a quick trust review when assessing exposure, without assuming it actually ran in this job.';

</script>

<div class="space-y-6 {className}">
	<div
		class="flex gap-2 rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-4 py-3 text-xs text-[var(--text-secondary)]"
	>
		<Info class="mt-0.5 h-4 w-4 shrink-0 text-[var(--text-tertiary)]" />
		<div class="space-y-1">
			<p>
				<strong class="text-[var(--text-primary)]">Blast radius</strong> here is the run&apos;s observed
				<strong>execution surface</strong>, aggregated <strong>across every job</strong> in this run:
				processes that actually ran, network flows we recorded, and directories inferred from executable paths
				(not a full filesystem audit).
			</p>
			<p>
				Use <strong class="text-[var(--text-primary)]">By job</strong> below to focus on one pipeline stage;
				search filters binaries, network, and directories together.
			</p>
			<p>
				<strong class="text-[var(--text-primary)]">Hostnames:</strong> we store destination
				<strong>IP + port</strong> today. DNS names are not persisted unless the agent starts reporting them.
			</p>
			{#if data.network_connections_truncated}
				<p>
					<strong class="text-[var(--text-primary)]">Network cap:</strong> this view lists
					{data.network_connections.length} of {data.network_connections_total_count} stored connection
					rows (ordered by job, then time). Export or query the store for the full set if you hit the cap.
				</p>
			{:else if hasNet && data.network_connections_total_count > 0}
				<p class="text-[var(--text-tertiary)]">
					Network: {data.network_connections_total_count} row{data.network_connections_total_count === 1
						? ''
						: 's'} stored for this run.
				</p>
			{/if}
			{#if hasScriptInferred}
				<p>
					<strong class="text-[var(--text-primary)]">Script references:</strong> rows labeled as coming from
					the script are <strong>not proof something ran</strong>; they flag tools the script might invoke
					(including branches that were skipped). That is useful context for supply-chain and trust review
					without treating every reference as an executed attack surface.
				</p>
			{/if}
		</div>
	</div>

	{#if empty}
		<Card>
			<div class="py-12 text-center text-sm text-[var(--text-secondary)]">
				<p class="font-medium text-[var(--text-primary)]">No footprint data yet</p>
				<p class="mt-2">
					Run this pipeline on an agent with exec telemetry enabled. Network rows appear once connection
					metadata is ingested into <code class="rounded bg-[var(--bg-tertiary)] px-1">run_network_connections</code>.
				</p>
			</div>
		</Card>
	{/if}

	{#if !empty}
		<Card padding="sm">
			<div class="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
				<div class="flex items-start gap-2">
					<Layers class="mt-0.5 h-4 w-4 shrink-0 text-[var(--text-tertiary)]" />
					<div class="text-xs text-[var(--text-secondary)]">
						<p class="font-medium text-[var(--text-primary)]">Run aggregate</p>
						<p>
							<span class="text-[var(--text-tertiary)]">{runWideJobCount}</span> job layer{runWideJobCount === 1
								? ''
								: 's'} with footprint context ·
							<span class="text-[var(--text-tertiary)]">{data.executed_binaries.length}</span> binary row{data
								.executed_binaries.length === 1
								? ''
								: 's'} ·
							<span class="text-[var(--text-tertiary)]">{data.network_connections_total_count}</span> network
							row{data.network_connections_total_count === 1 ? '' : 's'} (storage) ·
							<span class="text-[var(--text-tertiary)]">{data.filesystem_by_directory.length}</span> directory
							group{data.filesystem_by_directory.length === 1 ? '' : 's'}
						</p>
					</div>
				</div>
				<div class="w-full min-w-[12rem] sm:max-w-xs">
					<label for="footprint-search" class="mb-1 block text-xs font-medium text-[var(--text-secondary)]">
						Search
					</label>
					<Input
						id="footprint-search"
						type="search"
						placeholder="Path, hash, job, IP, port…"
						bind:value={searchQuery}
						size="sm"
					/>
				</div>
			</div>
		</Card>
	{/if}

	{#if !empty && searchQuery.trim() && filteredAllEmpty}
		<Card>
			<p class="py-8 text-center text-sm text-[var(--text-secondary)]">No rows match this search.</p>
		</Card>
	{/if}

	{#if !empty && visibleJobLayers.length > 0 && !(searchQuery.trim() && filteredAllEmpty)}
		<div class="space-y-2">
			<h3 class="flex items-center gap-2 text-sm font-medium text-[var(--text-primary)]">
				<Layers class="h-4 w-4 text-[var(--text-tertiary)]" />
				By job
				<span class="text-xs font-normal text-[var(--text-tertiary)]">(collapsed by default)</span>
			</h3>
			{#each visibleJobLayers as layer (layer.jobName)}
					<details
						class="group rounded-lg border border-[var(--border-primary)] bg-[var(--surface-elevated)]/60 [&_summary::-webkit-details-marker]:hidden"
					>
						<summary
							class="flex cursor-pointer list-none items-center gap-2 px-3 py-3 text-left hover:bg-[var(--bg-secondary)]/80"
						>
							<ChevronDown
								class="mt-0.5 h-4 w-4 shrink-0 text-[var(--text-tertiary)] transition-transform group-open:rotate-180"
								aria-hidden="true"
							/>
							<div class="min-w-0 flex-1">
								<p class="text-sm font-medium text-[var(--text-primary)]">
									{layer.jobName === JOB_SENTINEL ? 'Job unknown' : layer.jobName}
								</p>
								<p class="mt-0.5 text-xs text-[var(--text-secondary)]">
									{layer.binaries.length} binary row{layer.binaries.length === 1 ? '' : 's'} ·
									{layer.network.length} network row{layer.network.length === 1 ? '' : 's'}
									{#if layer.directoryTouch}
										<span class="text-[var(--text-tertiary)]">
											· referenced under <strong class="text-[var(--text-secondary)]">Directories</strong
											></span
										>
									{/if}
								</p>
							</div>
						</summary>
						<div class="space-y-4 border-t border-[var(--border-primary)] px-3 py-3">
							{#if layer.binaries.length === 0 && layer.network.length === 0 && layer.directoryTouch}
								<p class="text-xs text-[var(--text-secondary)]">
									No binary or network rows under this job name; executable paths still appear in the
									shared <strong class="text-[var(--text-primary)]">Directories</strong> section (job ·
									step labels on each file).
								</p>
							{/if}
							{#if layer.binaries.length > 0}
								<div>
									<h4 class="mb-2 flex items-center gap-2 text-xs font-medium text-[var(--text-primary)]">
										<Terminal class="h-3.5 w-3.5" />
										Executed binaries
									</h4>
									<div class="overflow-x-auto rounded-md border border-[var(--border-primary)]">
										<table class="w-full min-w-[36rem] text-left text-xs">
											<thead>
												<tr class="border-b border-[var(--border-secondary)] bg-[var(--bg-tertiary)] text-[var(--text-tertiary)]">
													<th class="px-2 py-2 font-medium">Step</th>
													<th class="px-2 py-2 font-medium">Path</th>
													<th class="px-2 py-2 font-medium">SHA-256</th>
													<th class="px-2 py-2 text-right font-medium">Exec count</th>
												</tr>
											</thead>
											<tbody class="text-[var(--text-primary)]">
												{#each layer.binaries as row (row.sha256 + row.binary_path + row.job_name + (row.step_name ?? ''))}
													<tr class="border-b border-[var(--border-secondary)]/80 align-top last:border-0">
														<td class="px-2 py-2">{row.step_name?.trim() ? row.step_name : '—'}</td>
														<td class="px-2 py-2 font-mono break-all">{row.binary_path}</td>
														<td class="px-2 py-2">
															<div class="flex flex-wrap items-center gap-1">
																{#if row.sha256 === SCRIPT_INFERRED_SHA}
																	<span class="block max-w-[14rem] leading-snug">
																		<span
																			class="text-[var(--text-secondary)]"
																			title={scriptInferredTitle}
																		>
																			Script reference
																		</span>
																		<span
																			class="mt-0.5 block text-[var(--text-tertiary)]"
																			title={scriptInferredTitle}
																		>
																			Not verified executed — could run under other conditions
																		</span>
																	</span>
																{:else}
																	<span class="font-mono break-all">{row.sha256 || '—'}</span>
																	{#if row.sha256}
																		<CopyButton text={row.sha256} size="sm" />
																	{/if}
																{/if}
															</div>
														</td>
														<td class="px-2 py-2 text-right tabular-nums">{row.execution_count}</td>
													</tr>
												{/each}
											</tbody>
										</table>
									</div>
								</div>
							{/if}
							{#if layer.network.length > 0}
								<div>
									<h4 class="mb-2 flex items-center gap-2 text-xs font-medium text-[var(--text-primary)]">
										<Globe class="h-3.5 w-3.5" />
										Network (destination IP / port)
									</h4>
									<div class="overflow-x-auto rounded-md border border-[var(--border-primary)]">
										<table class="w-full min-w-[44rem] text-left text-xs">
											<thead>
												<tr class="border-b border-[var(--border-secondary)] bg-[var(--bg-tertiary)] text-[var(--text-tertiary)]">
													<th class="px-2 py-2 font-medium">Executable</th>
													<th class="px-2 py-2 font-medium">Destination</th>
													<th class="px-2 py-2 font-medium">Protocol</th>
													<th class="px-2 py-2 font-medium">Direction</th>
													<th class="px-2 py-2 font-medium">Connected</th>
												</tr>
											</thead>
											<tbody>
												{#each layer.network as row (`${row.connected_at}-${row.dst_ip}-${row.dst_port}-${row.binary_sha256 ?? ''}`)}
													<tr class="border-b border-[var(--border-secondary)]/80 align-top text-[var(--text-primary)] last:border-0">
														<td class="px-2 py-2 align-top font-mono text-xs">
															{#if row.binary_path}
																<div class="break-all">{row.binary_path}</div>
																{#if row.binary_sha256}
																	<div class="mt-1 flex flex-wrap items-center gap-1">
																		<span class="break-all text-[var(--text-secondary)]">{row.binary_sha256}</span>
																		<CopyButton text={row.binary_sha256} size="sm" />
																	</div>
																{/if}
															{:else}
																<span class="text-[var(--text-tertiary)]">—</span>
															{/if}
														</td>
														<td class="px-2 py-2 font-mono">
															{row.dst_ip}:{row.dst_port}
															<div class="mt-0.5 font-sans text-[var(--text-tertiary)]">Hostname not stored</div>
														</td>
														<td class="px-2 py-2">{row.protocol}</td>
														<td class="px-2 py-2">{row.direction}</td>
														<td class="px-2 py-2 text-[var(--text-secondary)]">{row.connected_at}</td>
													</tr>
												{/each}
											</tbody>
										</table>
									</div>
								</div>
							{/if}
						</div>
					</details>
			{/each}
		</div>
	{/if}

	{#if hasFilteredFs && !(searchQuery.trim() && filteredAllEmpty)}
		<Card padding="sm">
			<h3 class="mb-1 flex items-center gap-2 font-medium text-[var(--text-primary)]">
				<FolderTree class="h-4 w-4" />
				Directories (from executable paths)
			</h3>
			<p class="mb-3 text-xs text-[var(--text-tertiary)]">
				Shared across jobs. Each file lists <span class="text-[var(--text-secondary)]">job · step</span> labels
				from exec telemetry when available.
			</p>
			{#if data.filesystem_directories_truncated || data.filesystem_more_directory_count}
				<p class="mb-3 text-xs text-[var(--text-tertiary)]">
					Showing the first {data.filesystem_by_directory.length} director{data.filesystem_by_directory.length === 1 ? 'y' : 'ies'}.
					{#if data.filesystem_more_directory_count}
						Omitted: {data.filesystem_more_directory_count} additional director{data.filesystem_more_directory_count === 1 ? 'y' : 'ies'}.
					{/if}
				</p>
			{/if}

			<div class="space-y-2">
				{#each filteredFilesystem as group (group.directory)}
					<details
						class="rounded-lg border border-[var(--border-primary)] bg-[var(--bg-tertiary)] [&_summary::-webkit-details-marker]:hidden"
					>
						<summary
							class="flex cursor-pointer list-none items-center gap-2 px-3 py-2 text-sm font-medium text-[var(--text-primary)] hover:bg-[var(--surface-elevated)]"
						>
							<span class="text-[var(--text-tertiary)]">▸</span>
							<span class="font-mono text-xs">{group.directory}</span>
							<Badge variant="secondary" size="sm">{group.entries.length} executable(s)</Badge>
							{#if group.entries_truncated}
								<Badge variant="warning" size="sm">truncated</Badge>
							{/if}
						</summary>
						<div class="border-t border-[var(--border-secondary)] px-3 py-2">
							<table class="w-full text-left text-xs">
								<tbody>
									{#each group.entries as e (e.binary_path + e.sha256)}
										<tr class="border-b border-[var(--border-secondary)]/60 last:border-0">
											<td class="py-1.5 pr-2 font-mono break-all">{e.binary_path}</td>
											<td class="py-1.5 pr-2">
												{#if e.sha256 === SCRIPT_INFERRED_SHA}
													<span class="block max-w-[14rem] leading-snug">
														<span class="text-[var(--text-secondary)]" title={scriptInferredTitle}>
															Script reference
														</span>
														<span
															class="mt-0.5 block text-[var(--text-tertiary)]"
															title={scriptInferredTitle}
														>
															Not verified executed — could run under other conditions
														</span>
													</span>
												{:else}
													<div class="flex flex-wrap items-center gap-1">
														<span class="font-mono">{e.sha256}</span>
														{#if e.sha256}
															<CopyButton text={e.sha256} size="sm" />
														{/if}
													</div>
												{/if}
											</td>
											<td class="py-1.5 pr-2 tabular-nums">{e.execution_count}×</td>
											<td class="py-1.5 text-[var(--text-secondary)]">
												{e.job_names.filter(Boolean).join(', ') || '—'}
											</td>
										</tr>
									{/each}
								</tbody>
							</table>
						</div>
					</details>
				{/each}
			</div>
		</Card>
	{/if}
</div>
