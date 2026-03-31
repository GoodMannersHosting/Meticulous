<script lang="ts">
	import { Button, Card, Input, Select, Badge, StatusBadge, Alert, Dialog } from '$components/ui';
	import { DataTable, Skeleton, EmptyState } from '$components/data';
	import { apiMethods } from '$api/client';
	import type { Agent } from '$api/types';
	import { formatRelativeTime } from '$utils/format';
	import { Server, RefreshCw, Search, Filter, Power, Play, MoreVertical, Info } from 'lucide-svelte';
	import { goto } from '$app/navigation';
	import type { Column } from '$components/data/DataTable.svelte';

	let agents = $state<Agent[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let searchQuery = $state('');
	let statusFilter = $state<string>('');
	let poolFilter = $state<string>('');
	let selectedAgent = $state<Agent | null>(null);
	let showDetailsDialog = $state(false);
	let actionLoading = $state(false);

	const statusOptions = [
		{ value: '', label: 'All Statuses' },
		{ value: 'online', label: 'Online' },
		{ value: 'busy', label: 'Busy' },
		{ value: 'draining', label: 'Draining' },
		{ value: 'offline', label: 'Offline' }
	];

	$effect(() => {
		loadAgents();
	});

	async function loadAgents() {
		loading = true;
		error = null;
		try {
			const response = await apiMethods.agents.list({
				status: statusFilter || undefined,
				pool: poolFilter || undefined
			});
			agents = response.items;
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load agents';
			agents = [];
		} finally {
			loading = false;
		}
	}

	async function drainAgent(agent: Agent) {
		actionLoading = true;
		try {
			await apiMethods.agents.drain(agent.id);
			await loadAgents();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to drain agent';
		} finally {
			actionLoading = false;
		}
	}

	async function resumeAgent(agent: Agent) {
		actionLoading = true;
		try {
			await apiMethods.agents.resume(agent.id);
			await loadAgents();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to resume agent';
		} finally {
			actionLoading = false;
		}
	}

	function showDetails(agent: Agent) {
		selectedAgent = agent;
		showDetailsDialog = true;
	}

	const filteredAgents = $derived(
		searchQuery
			? agents.filter(
					(a) =>
						a.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
						a.tags.some((t) => t.toLowerCase().includes(searchQuery.toLowerCase()))
				)
			: agents
	);

	const pools = $derived([...new Set(agents.map((a) => a.pool).filter(Boolean))]);

	const poolOptions = $derived([
		{ value: '', label: 'All Pools' },
		...pools.map((p) => ({ value: p as string, label: p as string }))
	]);

	const stats = $derived({
		total: agents.length,
		online: agents.filter((a) => a.status === 'online').length,
		busy: agents.filter((a) => a.status === 'busy').length,
		draining: agents.filter((a) => a.status === 'draining').length,
		offline: agents.filter((a) => a.status === 'offline').length,
		totalCapacity: agents.reduce((sum, a) => sum + a.max_jobs, 0),
		usedCapacity: agents.reduce((sum, a) => sum + a.running_jobs, 0)
	});
</script>

<svelte:head>
	<title>Agents | Meticulous</title>
</svelte:head>

<div class="space-y-6">
	<div class="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
		<div>
			<h1 class="text-2xl font-bold text-[var(--text-primary)]">Agents</h1>
			<p class="mt-1 text-[var(--text-secondary)]">
				Monitor and manage your build agents.
			</p>
		</div>

		<Button variant="ghost" size="sm" onclick={loadAgents} loading={loading}>
			<RefreshCw class="h-4 w-4" />
			Refresh
		</Button>
	</div>

	<div class="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
		<Card>
			<div class="flex items-center gap-3">
				<div class="flex h-10 w-10 items-center justify-center rounded-lg bg-success-100 dark:bg-success-900/30">
					<Server class="h-5 w-5 text-success-600 dark:text-success-400" />
				</div>
				<div>
					<p class="text-2xl font-bold text-[var(--text-primary)]">{stats.online}</p>
					<p class="text-sm text-[var(--text-secondary)]">Online</p>
				</div>
			</div>
		</Card>
		<Card>
			<div class="flex items-center gap-3">
				<div class="flex h-10 w-10 items-center justify-center rounded-lg bg-warning-100 dark:bg-warning-900/30">
					<Server class="h-5 w-5 text-warning-600 dark:text-warning-400" />
				</div>
				<div>
					<p class="text-2xl font-bold text-[var(--text-primary)]">{stats.busy}</p>
					<p class="text-sm text-[var(--text-secondary)]">Busy</p>
				</div>
			</div>
		</Card>
		<Card>
			<div class="flex items-center gap-3">
				<div class="flex h-10 w-10 items-center justify-center rounded-lg bg-secondary-100 dark:bg-secondary-800">
					<Server class="h-5 w-5 text-secondary-600 dark:text-secondary-400" />
				</div>
				<div>
					<p class="text-2xl font-bold text-[var(--text-primary)]">{stats.offline}</p>
					<p class="text-sm text-[var(--text-secondary)]">Offline</p>
				</div>
			</div>
		</Card>
		<Card>
			<div class="flex items-center gap-3">
				<div class="flex h-10 w-10 items-center justify-center rounded-lg bg-primary-100 dark:bg-primary-900/30">
					<Server class="h-5 w-5 text-primary-600 dark:text-primary-400" />
				</div>
				<div>
					<p class="text-2xl font-bold text-[var(--text-primary)]">
						{stats.usedCapacity}/{stats.totalCapacity}
					</p>
					<p class="text-sm text-[var(--text-secondary)]">Capacity</p>
				</div>
			</div>
		</Card>
	</div>

	<div class="flex flex-wrap gap-4">
		<div class="relative flex-1 max-w-md">
			<Search class="absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-[var(--text-tertiary)]" />
			<Input
				type="search"
				placeholder="Search agents or tags..."
				class="pl-10"
				bind:value={searchQuery}
			/>
		</div>
		<div class="w-40">
			<Select
				options={statusOptions}
				bind:value={statusFilter}
				onchange={loadAgents}
			/>
		</div>
		{#if pools.length > 0}
			<div class="w-40">
				<Select
					options={poolOptions}
					bind:value={poolFilter}
					onchange={loadAgents}
				/>
			</div>
		{/if}
	</div>

	{#if error}
		<Alert variant="error" title="Error" dismissible ondismiss={() => (error = null)}>
			{error}
		</Alert>
	{/if}

	{#if loading}
		<Card>
			<div class="space-y-4">
				{#each Array(5) as _, i (i)}
					<div class="flex items-center gap-4">
						<Skeleton class="h-10 w-10 rounded-full" />
						<div class="flex-1 space-y-2">
							<Skeleton class="h-4 w-48" />
							<Skeleton class="h-3 w-32" />
						</div>
						<Skeleton class="h-6 w-20 rounded-full" />
						<Skeleton class="h-8 w-24" />
					</div>
				{/each}
			</div>
		</Card>
	{:else if filteredAgents.length === 0}
		<Card>
			<EmptyState
				title="No agents found"
				description={searchQuery || statusFilter ? 'Try adjusting your filters.' : 'No agents have registered yet.'}
			/>
		</Card>
	{:else}
		<div class="overflow-hidden rounded-lg border border-[var(--border-primary)]">
			<table class="w-full text-sm">
				<thead class="bg-[var(--bg-tertiary)]">
					<tr>
						<th class="px-4 py-3 text-left font-medium text-[var(--text-secondary)]">Agent</th>
						<th class="px-4 py-3 text-left font-medium text-[var(--text-secondary)]">Status</th>
						<th class="px-4 py-3 text-left font-medium text-[var(--text-secondary)]">Pool</th>
						<th class="px-4 py-3 text-left font-medium text-[var(--text-secondary)]">Capacity</th>
						<th class="px-4 py-3 text-left font-medium text-[var(--text-secondary)]">Tags</th>
						<th class="px-4 py-3 text-left font-medium text-[var(--text-secondary)]">Last Seen</th>
						<th class="px-4 py-3 text-right font-medium text-[var(--text-secondary)]">Actions</th>
					</tr>
				</thead>
				<tbody class="divide-y divide-[var(--border-secondary)]">
					{#each filteredAgents as agent (agent.id)}
						<tr class="bg-[var(--bg-secondary)] transition-colors hover:bg-[var(--bg-hover)]">
							<td class="px-4 py-3">
								<button
									type="button"
									class="text-left"
									onclick={() => showDetails(agent)}
								>
									<div class="font-medium text-[var(--text-primary)] hover:text-primary-600">
										{agent.name}
									</div>
									<div class="text-xs text-[var(--text-tertiary)]">
										{agent.os}/{agent.arch} • v{agent.version}
									</div>
								</button>
							</td>
							<td class="px-4 py-3">
								<StatusBadge status={agent.status} size="sm" />
							</td>
							<td class="px-4 py-3">
								{#if agent.pool}
									<Badge variant="secondary" size="sm">{agent.pool}</Badge>
								{:else}
									<span class="text-[var(--text-tertiary)]">—</span>
								{/if}
							</td>
							<td class="px-4 py-3">
								<span class="text-sm">
									{agent.running_jobs}/{agent.max_jobs}
								</span>
							</td>
							<td class="px-4 py-3">
								<div class="flex flex-wrap gap-1">
									{#each agent.tags.slice(0, 3) as tag (tag)}
										<Badge variant="outline" size="sm">{tag}</Badge>
									{/each}
									{#if agent.tags.length > 3}
										<Badge variant="secondary" size="sm">+{agent.tags.length - 3}</Badge>
									{/if}
								</div>
							</td>
							<td class="px-4 py-3 text-sm text-[var(--text-secondary)]">
								{#if agent.last_heartbeat_at}
									{formatRelativeTime(agent.last_heartbeat_at)}
								{:else}
									<span class="text-[var(--text-tertiary)]">—</span>
								{/if}
							</td>
							<td class="px-4 py-3 text-right">
								<div class="flex items-center justify-end gap-1">
									{#if agent.status === 'draining'}
										<Button
											variant="ghost"
											size="sm"
											onclick={() => resumeAgent(agent)}
											loading={actionLoading}
										>
											<Play class="h-4 w-4" />
											Resume
										</Button>
									{:else if agent.status === 'online' || agent.status === 'busy'}
										<Button
											variant="ghost"
											size="sm"
											onclick={() => drainAgent(agent)}
											loading={actionLoading}
										>
											<Power class="h-4 w-4" />
											Drain
										</Button>
									{/if}
									<Button
										variant="ghost"
										size="sm"
										onclick={() => showDetails(agent)}
									>
										<Info class="h-4 w-4" />
									</Button>
								</div>
							</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
	{/if}
</div>

<Dialog bind:open={showDetailsDialog} title="Agent Details">
	{#if selectedAgent}
		<div class="space-y-4">
			<div class="flex items-center gap-4">
				<div class="flex h-12 w-12 items-center justify-center rounded-full bg-primary-100 dark:bg-primary-900/30">
					<Server class="h-6 w-6 text-primary-600 dark:text-primary-400" />
				</div>
				<div>
					<h3 class="font-semibold text-[var(--text-primary)]">{selectedAgent.name}</h3>
					<div class="flex items-center gap-2">
						<StatusBadge status={selectedAgent.status} size="sm" />
					</div>
				</div>
			</div>

			<div class="grid grid-cols-2 gap-4 text-sm">
				<div>
					<p class="text-[var(--text-tertiary)]">OS / Architecture</p>
					<p class="font-medium text-[var(--text-primary)]">{selectedAgent.os} / {selectedAgent.arch}</p>
				</div>
				<div>
					<p class="text-[var(--text-tertiary)]">Version</p>
					<p class="font-medium text-[var(--text-primary)]">{selectedAgent.version}</p>
				</div>
				<div>
					<p class="text-[var(--text-tertiary)]">Pool</p>
					<p class="font-medium text-[var(--text-primary)]">{selectedAgent.pool ?? '—'}</p>
				</div>
				<div>
					<p class="text-[var(--text-tertiary)]">Capacity</p>
					<p class="font-medium text-[var(--text-primary)]">
						{selectedAgent.running_jobs} / {selectedAgent.max_jobs} jobs
					</p>
				</div>
				<div>
					<p class="text-[var(--text-tertiary)]">Last Heartbeat</p>
					<p class="font-medium text-[var(--text-primary)]">
						{selectedAgent.last_heartbeat_at ? formatRelativeTime(selectedAgent.last_heartbeat_at) : '—'}
					</p>
				</div>
				<div>
					<p class="text-[var(--text-tertiary)]">Registered</p>
					<p class="font-medium text-[var(--text-primary)]">
						{formatRelativeTime(selectedAgent.created_at)}
					</p>
				</div>
			</div>

			{#if selectedAgent.tags.length > 0}
				<div>
					<p class="mb-2 text-sm text-[var(--text-tertiary)]">Tags</p>
					<div class="flex flex-wrap gap-2">
						{#each selectedAgent.tags as tag (tag)}
							<Badge variant="outline">{tag}</Badge>
						{/each}
					</div>
				</div>
			{/if}

			<div class="flex justify-end gap-2 pt-4">
				<Button variant="outline" onclick={() => (showDetailsDialog = false)}>
					Close
				</Button>
				{#if selectedAgent.status === 'draining'}
					<Button variant="primary" onclick={() => { resumeAgent(selectedAgent!); showDetailsDialog = false; }}>
						Resume Agent
					</Button>
				{:else if selectedAgent.status === 'online' || selectedAgent.status === 'busy'}
					<Button variant="outline" onclick={() => { drainAgent(selectedAgent!); showDetailsDialog = false; }}>
						Drain Agent
					</Button>
				{/if}
			</div>
		</div>
	{/if}
</Dialog>
