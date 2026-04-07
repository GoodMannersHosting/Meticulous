<script lang="ts">
	import { Button, Card, Input, Select, Badge, StatusBadge, Alert, Dialog } from '$components/ui';
	import { DataTable, Skeleton, EmptyState } from '$components/data';
	import { apiMethods } from '$api/client';
	import type { Agent } from '$api/types';
	import { formatRelativeTime } from '$utils/format';
	import { Server, RefreshCw, Search, Power, Play, Info, Trash2, ChevronLeft } from 'lucide-svelte';
	import { goto } from '$app/navigation';
	import type { Column } from '$components/data/DataTable.svelte';

	let agents = $state<Agent[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let searchQuery = $state('');
	let statusFilter = $state<string>('');
	let poolFilter = $state<string>('');
	let selectedAgent = $state<Agent | null>(null);
	let showDeleteDialog = $state(false);
	let agentToDelete = $state<Agent | null>(null);
	let actionLoading = $state(false);
	let hostMetadataFilter = $state('');

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
			agents = response.data;
			if (selectedAgent) {
				const updated = response.data.find((a) => a.id === selectedAgent!.id);
				selectedAgent = updated ?? null;
			}
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

	function confirmDelete(agent: Agent) {
		agentToDelete = agent;
		showDeleteDialog = true;
	}

	async function deleteAgent() {
		if (!agentToDelete) return;
		actionLoading = true;
		error = null;
		try {
			await apiMethods.agents.delete(agentToDelete.id);
			showDeleteDialog = false;
			agentToDelete = null;
			selectedAgent = null;
			await loadAgents();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to remove agent';
		} finally {
			actionLoading = false;
		}
	}

	function showDetails(agent: Agent) {
		selectedAgent = agent;
		hostMetadataFilter = '';
	}

	function clearAgentPanel() {
		selectedAgent = null;
		hostMetadataFilter = '';
	}

	/** API uses `draining` for the whole drain lifecycle; show Paused when idle and not pulling new work. */
	function agentBadgeStatus(agent: Agent): string {
		if (agent.status === 'draining' && agent.running_jobs === 0) return 'paused';
		return agent.status;
	}

	function hostBundleHaystack(agent: Agent): string {
		const b = agent.last_security_bundle;
		if (!b || typeof b !== 'object') return '';
		try {
			return JSON.stringify(b).toLowerCase();
		} catch {
			return '';
		}
	}

	/** UI labels for host metadata keys (API/JSON keys unchanged). Matches controller `security_bundle_to_json`. */
	const HOST_METADATA_KEY_LABELS: Record<string, string> = {
		hostname: 'Hostname',
		os: 'Operating system',
		arch: 'Architecture',
		kernel_version: 'Kernel version',
		public_ips: 'Public IP addresses',
		private_ips: 'Private IP addresses',
		ntp_synchronized: 'NTP synchronized',
		container_runtime: 'Container runtime',
		container_runtime_version: 'Container runtime version',
		environment_type: 'Environment',
		agent_x509_public_key_hex: 'Agent public key (hex)',
		machine_id: 'Machine Identifier',
		logical_cpus: 'CPU Cores',
		memory_total_bytes: 'Memory (GB)',
		egress_public_ip: 'Egress public IP',
		kubernetes_pod_uid: 'Kubernetes pod UID',
		kubernetes_namespace: 'Kubernetes namespace',
		kubernetes_node_name: 'Kubernetes node'
	};

	const ENVIRONMENT_TYPE_LABELS: Record<string, string> = {
		ENVIRONMENT_TYPE_UNSPECIFIED: 'Unspecified',
		ENVIRONMENT_TYPE_PHYSICAL: 'Physical',
		ENVIRONMENT_TYPE_VIRTUAL: 'Virtual',
		ENVIRONMENT_TYPE_CONTAINER: 'Container'
	};

	function humanizeMetadataKey(key: string): string {
		return key
			.split('_')
			.filter(Boolean)
			.map((w) => w.charAt(0).toUpperCase() + w.slice(1).toLowerCase())
			.join(' ');
	}

	function hostMetadataRowLabel(key: string): string {
		return HOST_METADATA_KEY_LABELS[key] ?? humanizeMetadataKey(key);
	}

	function hostMetadataValueCellClass(key: string): string {
		return key === 'agent_x509_public_key_hex'
			? 'whitespace-pre-wrap break-all px-3 py-2 text-[var(--text-primary)] align-top font-mono text-xs sm:text-sm'
			: 'whitespace-pre-wrap break-all px-3 py-2 text-[var(--text-primary)] align-top text-sm';
	}

	function hostMetadataDisplayValue(key: string, val: unknown): string {
		if (key === 'memory_total_bytes') {
			const n = typeof val === 'number' ? val : Number(val);
			if (Number.isFinite(n)) {
				return (n / 1024 ** 3).toFixed(2);
			}
		}
		if (key === 'ntp_synchronized') {
			if (val === true || val === 'true') return 'Yes';
			if (val === false || val === 'false') return 'No';
		}
		if (key === 'environment_type') {
			if (typeof val === 'number' && Number.isInteger(val) && val >= 0 && val <= 3) {
				return ['Unspecified', 'Physical', 'Virtual', 'Container'][val] ?? String(val);
			}
			const s = String(val);
			if (ENVIRONMENT_TYPE_LABELS[s]) return ENVIRONMENT_TYPE_LABELS[s];
			if (s.startsWith('ENVIRONMENT_TYPE_')) {
				return s
					.replace(/^ENVIRONMENT_TYPE_/, '')
					.split('_')
					.filter(Boolean)
					.map((w) => w.charAt(0).toUpperCase() + w.slice(1).toLowerCase())
					.join(' ');
			}
		}
		return Array.isArray(val)
			? val.join(', ')
			: val != null && typeof val === 'object'
				? JSON.stringify(val)
				: String(val);
	}

	const filteredAgents = $derived(
		searchQuery
			? agents.filter((a) => {
					const q = searchQuery.toLowerCase();
					return (
						a.name.toLowerCase().includes(q) ||
						a.tags.some((t) => t.toLowerCase().includes(q)) ||
						(a.pool_tags ?? []).some((t) => t.toLowerCase().includes(q)) ||
						hostBundleHaystack(a).includes(q)
					);
				})
			: agents
	);

	const selectedHostMetadataRows = $derived.by(() => {
		const a = selectedAgent;
		const b = a?.last_security_bundle;
		if (!b || typeof b !== 'object') return [];
		const q = hostMetadataFilter.trim().toLowerCase();
		return Object.entries(b).filter(([key, val]) => {
			if (!q) return true;
			const label = hostMetadataRowLabel(key).toLowerCase();
			const str = hostMetadataDisplayValue(key, val).toLowerCase();
			return (
				key.toLowerCase().includes(q) ||
				label.includes(q) ||
				str.includes(q)
			);
		});
	});

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

<div class="flex flex-col gap-8 lg:flex-row lg:items-start lg:gap-10">
	<div class="min-w-0 flex-1 space-y-6">
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
					placeholder="Search agents, tags, pools, or host metadata..."
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
						<tr
							class="bg-[var(--bg-secondary)] transition-colors hover:bg-[var(--bg-hover)] {selectedAgent?.id === agent.id ? 'ring-2 ring-inset ring-primary-500/40' : ''}"
						>
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
								<StatusBadge status={agentBadgeStatus(agent)} size="sm" />
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
									{#if agent.running_jobs === 0}
										<span title="Remove agent from organization">
											<Button
												variant="ghost"
												size="sm"
												class="text-red-600 hover:text-red-700 dark:text-red-400 dark:hover:text-red-300"
												onclick={() => confirmDelete(agent)}
												loading={actionLoading}
											>
												<Trash2 class="h-4 w-4" />
											</Button>
										</span>
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

	{#if selectedAgent}
		<aside
			class="w-full shrink-0 rounded-xl border border-[var(--border-primary)] bg-[var(--bg-secondary)] lg:sticky lg:top-6 lg:max-h-[calc(100vh-5rem)] lg:min-h-[min(100%,24rem)] lg:w-[min(100%,28rem)] xl:w-[32rem] lg:overflow-hidden lg:self-start flex flex-col"
			aria-label="Agent details"
		>
			<div class="flex flex-1 flex-col overflow-hidden">
				<div class="flex items-start gap-3 border-b border-[var(--border-secondary)] p-4">
					<Button variant="ghost" size="sm" class="shrink-0 -ml-1" onclick={clearAgentPanel} title="Close panel">
						<ChevronLeft class="h-5 w-5" />
					</Button>
					<div class="flex min-w-0 flex-1 items-center gap-3">
						<div
							class="flex h-11 w-11 shrink-0 items-center justify-center rounded-full bg-primary-100 dark:bg-primary-900/30"
						>
							<Server class="h-5 w-5 text-primary-600 dark:text-primary-400" />
						</div>
						<div class="min-w-0">
							<h2 class="truncate text-lg font-semibold text-[var(--text-primary)]">{selectedAgent.name}</h2>
							<StatusBadge status={agentBadgeStatus(selectedAgent)} size="sm" />
						</div>
					</div>
				</div>

				<div class="min-h-0 flex-1 space-y-4 overflow-y-auto p-4">
					<div class="grid grid-cols-1 gap-4 text-sm sm:grid-cols-2">
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
						{#if selectedAgent.pool_tags && selectedAgent.pool_tags.length > 0}
							<div class="sm:col-span-2">
								<p class="text-[var(--text-tertiary)]">Pool tags</p>
								<div class="mt-1 flex flex-wrap gap-1">
									{#each selectedAgent.pool_tags as pt (pt)}
										<Badge variant="secondary" size="sm">{pt}</Badge>
									{/each}
								</div>
							</div>
						{/if}
						<div>
							<p class="text-[var(--text-tertiary)]">Capacity</p>
							<p class="font-medium text-[var(--text-primary)]">
								{selectedAgent.running_jobs} / {selectedAgent.max_jobs} jobs
							</p>
						</div>
						<div>
							<p class="text-[var(--text-tertiary)]">Last heartbeat</p>
							<p class="font-medium text-[var(--text-primary)]">
								{selectedAgent.last_heartbeat_at
									? formatRelativeTime(selectedAgent.last_heartbeat_at)
									: '—'}
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
						{#if !selectedAgent.last_security_bundle || typeof selectedAgent.last_security_bundle !== 'object'}
							<p class="text-sm text-[var(--text-secondary)]">No host metadata snapshot stored for this agent.</p>
						{:else if selectedHostMetadataRows.length === 0}
							<p class="text-sm text-[var(--text-secondary)]">No fields match this filter.</p>
						{:else}
							<div class="max-h-[min(50vh,24rem)] overflow-auto rounded-md border border-[var(--border-secondary)] lg:max-h-none lg:flex-1">
								<table class="w-full text-left text-sm">
									<tbody class="divide-y divide-[var(--border-secondary)]">
										{#each selectedHostMetadataRows as [key, val] (key)}
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
				</div>

				<div
					class="flex flex-wrap gap-2 border-t border-[var(--border-secondary)] bg-[var(--bg-secondary)] p-4"
				>
					{#if selectedAgent.running_jobs === 0}
						<Button
							variant="outline"
							class="border-red-200 text-red-700 hover:bg-red-50 dark:border-red-900 dark:text-red-400 dark:hover:bg-red-950/40"
							onclick={() => {
								confirmDelete(selectedAgent!);
								clearAgentPanel();
							}}
						>
							Remove agent
						</Button>
					{/if}
					<div class="flex flex-1 flex-wrap justify-end gap-2">
						{#if selectedAgent.status === 'draining'}
							<Button variant="primary" onclick={() => resumeAgent(selectedAgent!)} loading={actionLoading}>
								Resume agent
							</Button>
						{:else if selectedAgent.status === 'online' || selectedAgent.status === 'busy'}
							<Button variant="outline" onclick={() => drainAgent(selectedAgent!)} loading={actionLoading}>
								Drain agent
							</Button>
						{/if}
					</div>
				</div>
			</div>
		</aside>
	{/if}
</div>

<Dialog bind:open={showDeleteDialog} title="Remove agent?">
	{#if agentToDelete}
		<p class="text-sm text-[var(--text-secondary)]">
			This removes <span class="font-medium text-[var(--text-primary)]">{agentToDelete.name}</span> from your
			organization. The agent can register again with a join token (it will get a new identity).
		</p>
		<div class="mt-6 flex justify-end gap-2">
			<Button variant="outline" onclick={() => { showDeleteDialog = false; agentToDelete = null; }}>
				Cancel
			</Button>
			<Button variant="primary" class="bg-red-600 hover:bg-red-700" onclick={deleteAgent} loading={actionLoading}>
				Remove
			</Button>
		</div>
	{/if}
</Dialog>
