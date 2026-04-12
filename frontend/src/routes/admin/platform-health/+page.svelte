<script lang="ts">
	import { onMount } from 'svelte';
	import { Button, Card } from '$components/ui';
	import { Skeleton, EmptyState } from '$components/data';
	import { apiMethods } from '$api/client';
	import type { PlatformHealthResponse } from '$api/client';
	import { formatBytes } from '$utils/format';
	import {
		Activity,
		AlertTriangle,
		Database,
		ExternalLink,
		HardDrive,
		Radio,
		RefreshCw,
		Server
	} from 'lucide-svelte';

	let data = $state<PlatformHealthResponse | null>(null);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let deepScan = $state(false);

	async function load() {
		loading = true;
		error = null;
		try {
			data = await apiMethods.admin.ops.platformHealth({
				deep_object_store: deepScan || undefined
			});
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load platform health';
			data = null;
		} finally {
			loading = false;
		}
	}

	onMount(load);
</script>

<div class="space-y-4">
	<div class="flex flex-wrap items-center justify-between gap-3">
		<div class="flex items-center gap-3">
			<div
				class="flex h-10 w-10 items-center justify-center rounded-lg bg-[var(--bg-primary)] text-[var(--text-secondary)]"
			>
				<Activity class="h-5 w-5" />
			</div>
			<div>
				<h2 class="text-lg font-semibold text-[var(--text-primary)]">Platform Health</h2>
				<p class="text-sm text-[var(--text-secondary)]">
					Database size, org artifact totals, object storage reachability, and NATS JetStream
					footprint. Total host or volume capacity is not available here—use your infra metrics.
				</p>
			</div>
		</div>
		<div class="flex flex-wrap items-center gap-2">
			<label class="flex cursor-pointer items-center gap-2 text-sm text-[var(--text-secondary)]">
				<input
					type="checkbox"
					bind:checked={deepScan}
					class="rounded border-[var(--border-primary)]"
					onchange={load}
				/>
				Capped object-store scan
			</label>
			<Button variant="secondary" size="sm" onclick={() => load()} disabled={loading}>
				<RefreshCw class="mr-1.5 h-4 w-4 {loading ? 'animate-spin' : ''}" />
				Refresh
			</Button>
		</div>
	</div>

	{#if error}
		<div
			class="rounded-lg border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-800 dark:border-red-900 dark:bg-red-950/40 dark:text-red-200"
		>
			{error}
		</div>
	{/if}

	{#if loading && !data}
		<div class="grid gap-4 md:grid-cols-2">
			{#each Array(4) as _}
				<Card class="p-4"><Skeleton class="h-24 w-full" /></Card>
			{/each}
		</div>
	{:else if data}
		<div class="grid gap-4 lg:grid-cols-2">
			<Card class="p-4">
				<div class="mb-3 flex items-center gap-2">
					<Server class="h-4 w-4 text-[var(--text-secondary)]" />
					<h3 class="font-medium text-[var(--text-primary)]">Control plane</h3>
				</div>
				<dl class="space-y-2 text-sm">
					<div class="flex justify-between gap-4">
						<dt class="text-[var(--text-secondary)]">API version</dt>
						<dd class="text-[var(--text-primary)]">{data.api_version}</dd>
					</div>
					<div class="flex justify-between gap-4">
						<dt class="text-[var(--text-secondary)]">Pipeline engine</dt>
						<dd class="text-[var(--text-primary)]">
							{data.engine_initialized ? 'Initialized' : 'Unavailable'}
						</dd>
					</div>
				</dl>
				{#if data.engine_init_error}
					<div
						class="mt-3 flex gap-2 rounded-md border border-amber-200 bg-amber-50 px-3 py-2 text-xs text-amber-900 dark:border-amber-900 dark:bg-amber-950/40 dark:text-amber-100"
					>
						<AlertTriangle class="mt-0.5 h-4 w-4 shrink-0" />
						<span>{data.engine_init_error}</span>
					</div>
				{/if}
			</Card>

			<Card class="p-4">
				<div class="mb-3 flex items-center gap-2">
					<Database class="h-4 w-4 text-[var(--text-secondary)]" />
					<h3 class="font-medium text-[var(--text-primary)]">PostgreSQL</h3>
				</div>
				<p class="text-sm text-[var(--text-primary)]">
					<span class="text-[var(--text-secondary)]">Database size</span>
					<span class="ml-2 font-medium">{formatBytes(Math.max(0, data.postgres.database_bytes))}</span>
				</p>
				<p class="mt-1 text-xs text-[var(--text-tertiary)]">
					Physical size of the current database (includes indexes and bloat). Not per-org quota.
				</p>
				{#if data.postgres.top_relations.length > 0}
					<div class="mt-3 max-h-48 overflow-auto rounded border border-[var(--border-primary)]">
						<table class="w-full text-left text-xs">
							<thead class="sticky top-0 bg-[var(--bg-secondary)] text-[var(--text-secondary)]">
								<tr>
									<th class="px-2 py-1.5">Relation</th>
									<th class="px-2 py-1.5 text-right">Size</th>
								</tr>
							</thead>
							<tbody class="divide-y divide-[var(--border-primary)]">
								{#each data.postgres.top_relations as rel (`${rel.schema}.${rel.name}`)}
									<tr>
										<td class="px-2 py-1.5 text-[var(--text-primary)]">
											{rel.schema}.{rel.name}
										</td>
										<td class="px-2 py-1.5 text-right text-[var(--text-secondary)]">
											{formatBytes(Math.max(0, rel.total_bytes))}
										</td>
									</tr>
								{/each}
							</tbody>
						</table>
					</div>
				{/if}
			</Card>

			<Card class="p-4">
				<div class="mb-3 flex items-center gap-2">
					<HardDrive class="h-4 w-4 text-[var(--text-secondary)]" />
					<h3 class="font-medium text-[var(--text-primary)]">Artifacts (your org)</h3>
				</div>
				<dl class="space-y-2 text-sm">
					<div class="flex justify-between gap-4">
						<dt class="text-[var(--text-secondary)]">Recorded bytes</dt>
						<dd class="text-[var(--text-primary)]">
							{formatBytes(Math.max(0, data.org_artifacts.total_bytes))}
						</dd>
					</div>
					<div class="flex justify-between gap-4">
						<dt class="text-[var(--text-secondary)]">Artifact rows</dt>
						<dd class="text-[var(--text-primary)]">{data.org_artifacts.artifact_count}</dd>
					</div>
				</dl>
				<p class="mt-2 text-xs text-[var(--text-tertiary)]">
					Sum of <code class="rounded bg-[var(--bg-secondary)] px-1">size_bytes</code> in Postgres;
					may differ from raw object storage (logs, uncatalogued keys).
				</p>
			</Card>

			<Card class="p-4">
				<div class="mb-3 flex items-center gap-2">
					<HardDrive class="h-4 w-4 text-[var(--text-secondary)]" />
					<h3 class="font-medium text-[var(--text-primary)]">Object storage (S3-compatible)</h3>
				</div>
				<dl class="space-y-2 text-sm">
					<div class="flex justify-between gap-4">
						<dt class="text-[var(--text-secondary)]">Endpoint</dt>
						<dd class="break-all text-right text-[var(--text-primary)]">
							{data.object_storage.endpoint_display}
						</dd>
					</div>
					<div class="flex justify-between gap-4">
						<dt class="text-[var(--text-secondary)]">Bucket</dt>
						<dd class="text-[var(--text-primary)]">{data.object_storage.bucket}</dd>
					</div>
					<div class="flex justify-between gap-4">
						<dt class="text-[var(--text-secondary)]">Path-style</dt>
						<dd class="text-[var(--text-primary)]">{data.object_storage.path_style ? 'yes' : 'no'}</dd>
					</div>
					<div class="flex justify-between gap-4">
						<dt class="text-[var(--text-secondary)]">Client</dt>
						<dd class="text-[var(--text-primary)]">
							{data.object_storage.client_initialized ? 'initialized' : 'not initialized'}
						</dd>
					</div>
					<div class="flex justify-between gap-4">
						<dt class="text-[var(--text-secondary)]">Bucket reachable</dt>
						<dd class="text-[var(--text-primary)]">
							{data.object_storage.reachable ? 'yes' : 'no'}
						</dd>
					</div>
				</dl>
				{#if data.object_storage.reachability_error}
					<p class="mt-2 text-xs text-red-700 dark:text-red-300">
						{data.object_storage.reachability_error}
					</p>
				{/if}
				{#if data.object_storage.deep_scan}
					<div class="mt-3 rounded border border-[var(--border-primary)] bg-[var(--bg-secondary)] p-3 text-xs">
						<p class="font-medium text-[var(--text-primary)]">Prefix scan: {data.object_storage.deep_scan.prefix}</p>
						{#if data.object_storage.deep_scan.error}
							<p class="mt-1 text-red-700 dark:text-red-300">{data.object_storage.deep_scan.error}</p>
						{:else}
							<p class="mt-1 text-[var(--text-secondary)]">
								Objects sampled: {data.object_storage.deep_scan.objects_scanned} · Bytes summed:
								{formatBytes(data.object_storage.deep_scan.bytes_summed)} · List calls:
								{data.object_storage.deep_scan.list_pages}
								{#if data.object_storage.deep_scan.truncated}
									<span class="text-amber-700 dark:text-amber-300"> · truncated (cap hit)</span>
								{/if}
							</p>
						{/if}
					</div>
				{/if}
			</Card>

			<Card class="p-4 lg:col-span-2">
				<div class="mb-3 flex flex-wrap items-center justify-between gap-2">
					<div class="flex items-center gap-2">
						<Radio class="h-4 w-4 text-[var(--text-secondary)]" />
						<h3 class="font-medium text-[var(--text-primary)]">NATS JetStream</h3>
					</div>
					<a
						href="/admin/job-queue"
						class="inline-flex items-center gap-1 text-xs text-primary-600 hover:text-primary-500 dark:text-primary-400"
					>
						Job queue
						<ExternalLink class="h-3 w-3" />
					</a>
				</div>
				{#if !data.nats_jetstream.available}
					<p class="text-sm text-amber-800 dark:text-amber-200">
						{data.nats_jetstream.unavailable_reason ?? 'NATS unavailable.'}
					</p>
				{:else if data.nats_jetstream.streams.length === 0}
					<EmptyState title="No stream data" description="JetStream did not return stream stats." />
				{:else}
					<div class="overflow-x-auto">
						<table class="w-full min-w-[640px] text-left text-sm">
							<thead class="border-b border-[var(--border-primary)] bg-[var(--bg-secondary)]">
								<tr>
									<th class="px-3 py-2 font-medium text-[var(--text-secondary)]">Stream</th>
									<th class="px-3 py-2 font-medium text-[var(--text-secondary)]">Messages</th>
									<th class="px-3 py-2 font-medium text-[var(--text-secondary)]">Bytes</th>
									<th class="px-3 py-2 font-medium text-[var(--text-secondary)]">Error</th>
								</tr>
							</thead>
							<tbody class="divide-y divide-[var(--border-primary)]">
								{#each data.nats_jetstream.streams as s (s.name)}
									<tr class="bg-[var(--bg-primary)]">
										<td class="px-3 py-2 font-medium text-[var(--text-primary)]">{s.name}</td>
										<td class="px-3 py-2 text-[var(--text-secondary)]">{s.messages}</td>
										<td class="px-3 py-2 text-[var(--text-secondary)]">
											{formatBytes(s.bytes)}
										</td>
										<td class="px-3 py-2 text-xs text-red-700 dark:text-red-300">
											{s.error ?? '—'}
										</td>
									</tr>
								{/each}
							</tbody>
						</table>
					</div>
				{/if}
			</Card>
		</div>
	{/if}
</div>
