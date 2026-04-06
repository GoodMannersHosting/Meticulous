<script lang="ts" module>
	import type { RunFootprintResponse } from '$api/types';

	export interface RunFootprintViewerProps {
		data: RunFootprintResponse;
		class?: string;
	}
</script>

<script lang="ts">
	import { Card, CopyButton, Badge } from '$components/ui';
	import { Terminal, Globe, FolderTree, Info } from 'lucide-svelte';

	let { data, class: className = '' }: RunFootprintViewerProps = $props();

	/** Agent placeholder for binaries detected from step script text (not measured from disk). */
	const SCRIPT_INFERRED_SHA =
		'0000000000000000000000000000000000000000000000000000000000000000';

	const hasExec = $derived(data.executed_binaries.length > 0);
	const hasNet = $derived(data.network_connections.length > 0);
	const hasFs = $derived(data.filesystem_by_directory.length > 0);
	const empty = $derived(!hasExec && !hasNet && !hasFs);
</script>

<div class="space-y-6 {className}">
	<div
		class="flex gap-2 rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-4 py-3 text-xs text-[var(--text-secondary)]"
	>
		<Info class="mt-0.5 h-4 w-4 shrink-0 text-[var(--text-tertiary)]" />
		<div class="space-y-1">
			<p>
				<strong class="text-[var(--text-primary)]">Blast radius</strong> here is the run&apos;s observed
				<strong>execution surface</strong>: processes that actually ran, network flows we recorded, and
				directories inferred from executable paths (not a full filesystem audit).
			</p>
			<p>
				<strong class="text-[var(--text-primary)]">Hostnames:</strong> we store destination
				<strong>IP + port</strong> today. DNS names are not persisted unless the agent starts reporting them.
			</p>
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

	{#if hasExec}
		<Card padding="sm">
			<h3 class="mb-3 flex items-center gap-2 font-medium text-[var(--text-primary)]">
				<Terminal class="h-4 w-4" />
				Executed commands (binaries)
			</h3>
			<div class="overflow-x-auto">
				<table class="w-full min-w-[40rem] text-left text-xs">
					<thead>
						<tr class="border-b border-[var(--border-secondary)] text-[var(--text-tertiary)]">
							<th class="pb-2 pr-3 font-medium">Job</th>
							<th class="pb-2 pr-3 font-medium">Step</th>
							<th class="pb-2 pr-3 font-medium">Path</th>
							<th class="pb-2 pr-3 font-medium">SHA-256</th>
							<th class="pb-2 text-right font-medium">Exec count</th>
						</tr>
					</thead>
					<tbody class="text-[var(--text-primary)]">
						{#each data.executed_binaries as row (row.sha256 + row.binary_path + row.job_name + (row.step_name ?? ''))}
							<tr class="border-b border-[var(--border-secondary)]/80 align-top text-xs">
								<td class="py-2 pr-3">{row.job_name || '—'}</td>
								<td class="py-2 pr-3">{row.step_name?.trim() ? row.step_name : '—'}</td>
								<td class="py-2 pr-3 font-mono break-all">{row.binary_path}</td>
								<td class="py-2 pr-3">
									<div class="flex flex-wrap items-center gap-1">
										{#if row.sha256 === SCRIPT_INFERRED_SHA}
											<span
												class="text-[var(--text-secondary)]"
												title="Not a file hash; tool name detected in step run script"
											>
												Inferred (script)
											</span>
										{:else}
											<span class="font-mono break-all">{row.sha256 || '—'}</span>
											{#if row.sha256}
												<CopyButton text={row.sha256} size="sm" />
											{/if}
										{/if}
									</div>
								</td>
								<td class="py-2 text-right tabular-nums">{row.execution_count}</td>
							</tr>
						{/each}
					</tbody>
				</table>
			</div>
		</Card>
	{/if}

	{#if hasNet}
		<Card padding="sm">
			<h3 class="mb-3 flex items-center gap-2 font-medium text-[var(--text-primary)]">
				<Globe class="h-4 w-4" />
				Network (destination IP / port)
			</h3>
			<div class="overflow-x-auto">
				<table class="w-full min-w-[48rem] text-left text-xs">
					<thead>
						<tr class="border-b border-[var(--border-secondary)] text-[var(--text-tertiary)]">
							<th class="pb-2 pr-3 font-medium">Job</th>
							<th class="pb-2 pr-3 font-medium">Executable</th>
							<th class="pb-2 pr-3 font-medium">Destination</th>
							<th class="pb-2 pr-3 font-medium">Protocol</th>
							<th class="pb-2 pr-3 font-medium">Direction</th>
							<th class="pb-2 font-medium">Connected</th>
						</tr>
					</thead>
					<tbody>
						{#each data.network_connections as row (`${row.connected_at}-${row.dst_ip}-${row.dst_port}-${row.binary_sha256 ?? ''}`)}
							<tr class="border-b border-[var(--border-secondary)]/80 align-top text-[var(--text-primary)]">
								<td class="py-2 pr-3">{row.job_name ?? '—'}</td>
								<td class="py-2 pr-3 align-top font-mono text-xs">
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
								<td class="py-2 pr-3 font-mono">
									{row.dst_ip}:{row.dst_port}
									<div class="mt-0.5 font-sans text-[var(--text-tertiary)]">
										Hostname not stored
									</div>
								</td>
								<td class="py-2 pr-3">{row.protocol}</td>
								<td class="py-2 pr-3">{row.direction}</td>
								<td class="py-2 text-[var(--text-secondary)]">{row.connected_at}</td>
							</tr>
						{/each}
					</tbody>
				</table>
			</div>
		</Card>
	{/if}

	{#if hasFs}
		<Card padding="sm">
			<h3 class="mb-1 flex items-center gap-2 font-medium text-[var(--text-primary)]">
				<FolderTree class="h-4 w-4" />
				Directories (from executable paths)
			</h3>
			{#if data.filesystem_directories_truncated || data.filesystem_more_directory_count}
				<p class="mb-3 text-xs text-[var(--text-tertiary)]">
					Showing the first {data.filesystem_by_directory.length} director{data.filesystem_by_directory.length === 1 ? 'y' : 'ies'}.
					{#if data.filesystem_more_directory_count}
						Omitted: {data.filesystem_more_directory_count} additional director{data.filesystem_more_directory_count === 1 ? 'y' : 'ies'}.
					{/if}
				</p>
			{:else}
				<p class="mb-3 text-xs text-[var(--text-tertiary)]">
					Grouped by parent directory of observed executables (expandable). The last column lists
					<span class="text-[var(--text-secondary)]">job · step</span> when exec telemetry includes step linkage.
				</p>
			{/if}

			<div class="space-y-2">
				{#each data.filesystem_by_directory as group (group.directory)}
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
												<div class="flex flex-wrap items-center gap-1">
													<span class="font-mono">{e.sha256}</span>
													{#if e.sha256}
														<CopyButton text={e.sha256} size="sm" />
													{/if}
												</div>
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
