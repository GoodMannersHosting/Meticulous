<script lang="ts">
	import { Badge, Button, StatusBadge, Input, CopyButton } from '$components/ui';
	import type { JobRunAgentAudit } from '$lib/utils/jobRunAgentAudit';
	import { agentBadgeStatusFromAudit } from '$lib/utils/jobRunAgentAudit';
	import {
		filterHostMetadataEntries,
		hostMetadataDisplayValue,
		hostMetadataRowLabel,
		hostMetadataValueCellClass
	} from '$lib/utils/agentHostMetadata';
	import { formatRelativeTime } from '$utils/format';
	import { Server, Search, ExternalLink } from 'lucide-svelte';

	let {
		jobName,
		jobStatus,
		audit,
		/** Set when the job had an `agent_id` but `agent_snapshot` was never stored (e.g. older controller). */
		assignedAgentId = null as string | null
	}: {
		jobName: string;
		jobStatus: string;
		audit: JobRunAgentAudit | null;
		assignedAgentId?: string | null;
	} = $props();

	let hostMetadataFilter = $state('');

	const hostRows = $derived(filterHostMetadataEntries(audit?.last_security_bundle ?? null, hostMetadataFilter));
</script>

<div
	class="rounded-xl border border-[var(--border-primary)] bg-[var(--bg-secondary)] overflow-hidden"
	aria-label="Agent details for job {jobName}"
>
	<div class="border-b border-[var(--border-secondary)] px-4 py-3 flex flex-wrap items-center gap-2">
		<StatusBadge status={jobStatus} size="sm" showIcon={true} />
		<span class="font-medium text-[var(--text-primary)]">{jobName}</span>
	</div>

	<div class="p-4 space-y-4">
		{#if audit}
			{#if audit.snapshotCapturedAt}
				<p class="text-xs text-[var(--text-tertiary)]">
					Agent details as captured when this job entered
					<span class="font-medium text-[var(--text-secondary)]">running</span>
					({formatRelativeTime(audit.snapshotCapturedAt)}).
				</p>
			{:else}
				<p class="text-xs text-[var(--text-tertiary)]">
					From the job-start snapshot; capture time was not recorded on this row.
				</p>
			{/if}

			<div class="flex items-start gap-3">
				<div
					class="flex h-11 w-11 shrink-0 items-center justify-center rounded-full bg-primary-100 dark:bg-primary-900/30"
				>
					<Server class="h-5 w-5 text-primary-600 dark:text-primary-400" />
				</div>
				<div class="min-w-0 flex-1">
					<div class="flex flex-wrap items-center gap-2">
						<h3 class="truncate text-lg font-semibold text-[var(--text-primary)]">{audit.name}</h3>
						<StatusBadge status={agentBadgeStatusFromAudit(audit)} size="sm" />
					</div>
					{#if audit.agent_id}
						<div class="mt-1 flex flex-wrap items-center gap-2">
							<code class="text-xs text-[var(--text-tertiary)]">{audit.agent_id}</code>
							<CopyButton text={audit.agent_id} size="sm" />
							<Button variant="ghost" size="sm" href="/agents" class="h-7 px-2 text-xs">
								<ExternalLink class="h-3.5 w-3.5" />
								Agents
							</Button>
						</div>
					{/if}
				</div>
			</div>

			<div class="grid grid-cols-1 gap-4 text-sm sm:grid-cols-2">
				<div>
					<p class="text-[var(--text-tertiary)]">OS / Architecture</p>
					<p class="font-medium text-[var(--text-primary)]">{audit.os} / {audit.arch}</p>
				</div>
				<div>
					<p class="text-[var(--text-tertiary)]">Version</p>
					<p class="font-medium text-[var(--text-primary)]">{audit.version}</p>
				</div>
				<div>
					<p class="text-[var(--text-tertiary)]">Pool</p>
					<p class="font-medium text-[var(--text-primary)]">{audit.pool ?? '—'}</p>
				</div>
				{#if audit.pool_tags.length > 0}
					<div class="sm:col-span-2">
						<p class="text-[var(--text-tertiary)]">Pool tags</p>
						<div class="mt-1 flex flex-wrap gap-1">
							{#each audit.pool_tags as pt (pt)}
								<Badge variant="secondary" size="sm">{pt}</Badge>
							{/each}
						</div>
					</div>
				{/if}
				<div>
					<p class="text-[var(--text-tertiary)]">Capacity</p>
					<p class="font-medium text-[var(--text-primary)]">
						{audit.running_jobs} / {audit.max_jobs} jobs
					</p>
				</div>
				<div>
					<p class="text-[var(--text-tertiary)]">Last heartbeat</p>
					<p class="font-medium text-[var(--text-primary)]">
						{audit.last_heartbeat_at ? formatRelativeTime(audit.last_heartbeat_at) : '—'}
					</p>
				</div>
				<div>
					<p class="text-[var(--text-tertiary)]">Registered</p>
					<p class="font-medium text-[var(--text-primary)]">
						{audit.created_at ? formatRelativeTime(audit.created_at) : '—'}
					</p>
				</div>
			</div>

			{#if audit.tags.length > 0}
				<div>
					<p class="mb-2 text-sm text-[var(--text-tertiary)]">Tags</p>
					<div class="flex flex-wrap gap-2">
						{#each audit.tags as tag (tag)}
							<Badge variant="outline">{tag}</Badge>
						{/each}
					</div>
				</div>
			{/if}

			<div class="border-t border-[var(--border-secondary)] pt-4">
				<div class="mb-3 flex flex-col gap-2">
					<p class="text-sm font-medium text-[var(--text-primary)]">Host metadata (registration snapshot)</p>
					<div class="relative w-full">
						<Search class="absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-[var(--text-tertiary)]" />
						<Input
							type="search"
							placeholder="Filter fields..."
							class="pl-10"
							bind:value={hostMetadataFilter}
						/>
					</div>
				</div>
				{#if !audit.last_security_bundle || typeof audit.last_security_bundle !== 'object'}
					<p class="text-sm text-[var(--text-secondary)]">No host metadata snapshot stored for this agent.</p>
				{:else if hostRows.length === 0}
					<p class="text-sm text-[var(--text-secondary)]">No fields match this filter.</p>
				{:else}
					<div
						class="max-h-[min(50vh,24rem)] overflow-auto rounded-md border border-[var(--border-secondary)]"
					>
						<table class="w-full text-left text-sm">
							<tbody class="divide-y divide-[var(--border-secondary)]">
								{#each hostRows as [key, val] (key)}
									<tr class="bg-[var(--bg-primary)]">
										<th
											class="w-[38%] whitespace-normal break-words px-3 py-2 font-medium text-[var(--text-secondary)] align-top"
										>
											{hostMetadataRowLabel(key)}
										</th>
										<td class={hostMetadataValueCellClass(key)}>
											{hostMetadataDisplayValue(key, val)}
										</td>
									</tr>
								{/each}
							</tbody>
						</table>
					</div>
				{/if}
			</div>
		{:else if assignedAgentId}
			<p class="text-sm text-[var(--text-secondary)]">
				No job-start snapshot is stored for this job run, so agent details at execution time are not available
				here. The controller is expected to persist a snapshot when the job enters
				<span class="font-medium text-[var(--text-primary)]">running</span>
				(redeploy the controller if this persists).
			</p>
			<div class="mt-3 flex flex-wrap items-center gap-2">
				<span class="text-xs text-[var(--text-tertiary)]">Assigned agent id</span>
				<code class="text-xs text-[var(--text-secondary)]">{assignedAgentId}</code>
				<CopyButton text={assignedAgentId} size="sm" />
			</div>
		{:else}
			<p class="text-sm text-[var(--text-secondary)]">
				No agent was assigned to this job yet, or it never reached a running agent.
			</p>
		{/if}
	</div>
</div>
