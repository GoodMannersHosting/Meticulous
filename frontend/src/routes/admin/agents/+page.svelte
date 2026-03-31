<script lang="ts">
	import {
		Server,
		Plus,
		Key,
		Copy,
		Trash2,
		CheckCircle,
		XCircle,
		Pencil,
		History,
		Ban
	} from 'lucide-svelte';
	import { api } from '$lib/api';

	interface JoinTokenAgent {
		id: string;
		name: string;
		status: string;
		registered_at: string;
	}

	interface JoinTokenDescriptionHistoryEntry {
		description: string;
		changed_at: string;
		changed_by?: string;
		changed_by_name?: string;
	}

	interface JoinToken {
		id: string;
		prefix: string;
		description: string;
		scope: string;
		scope_id?: string;
		max_uses: number;
		current_uses: number;
		labels: string[];
		pool_tags: string[];
		expires_at?: string;
		revoked: boolean;
		created_by: string;
		created_by_name?: string;
		created_at: string;
		consumed_at?: string;
		consumed_by_agent_id?: string;
		description_history?: JoinTokenDescriptionHistoryEntry[];
		agents: JoinTokenAgent[];
	}

	interface JoinTokenListResponse {
		data: JoinToken[];
		pagination: {
			page: number;
			per_page: number;
			total: number;
			has_more: boolean;
		};
	}

	const PER_PAGE_OPTIONS = [20, 50, 100, 200] as const;

	let tokens = $state<JoinToken[]>([]);
	let listPagination = $state<JoinTokenListResponse['pagination'] | null>(null);
	let page = $state(1);
	let perPage = $state(20);
	let searchDraft = $state('');
	let searchQ = $state('');
	let loading = $state(true);
	let error = $state<string | null>(null);
	let showCreateModal = $state(false);
	let creating = $state(false);
	let newTokenValue = $state<string | null>(null);
	let tokenToRevoke = $state<JoinToken | null>(null);
	let revoking = $state(false);
	let tokenToDelete = $state<JoinToken | null>(null);
	let deleting = $state(false);
	let editModalToken = $state<JoinToken | null>(null);
	let editDescriptionDraft = $state('');
	let savingDescription = $state(false);
	let historyModalId = $state<string | null>(null);
	let historyDetail = $state<JoinToken | null>(null);
	let historyLoading = $state(false);

	let newToken = $state({
		description: '',
		scope: 'tenant',
		expiresInDays: '30',
		labels: '',
		poolTags: ''
	});

	let searchDebounceTimer: ReturnType<typeof setTimeout> | undefined;

	$effect(() => {
		const d = searchDraft;
		clearTimeout(searchDebounceTimer);
		searchDebounceTimer = setTimeout(() => {
			const next = d.trim();
			if (next !== searchQ) {
				searchQ = next;
				page = 1;
			}
		}, 400);
		return () => clearTimeout(searchDebounceTimer);
	});

	$effect(() => {
		void page;
		void perPage;
		void searchQ;
		loadTokens();
	});

	async function loadTokens() {
		loading = true;
		error = null;
		try {
			const params: Record<string, string | number> = {
				page,
				limit: perPage
			};
			if (searchQ) params.q = searchQ;
			const response = await api.get<JoinTokenListResponse>('/admin/join-tokens', { params });
			tokens = response.data ?? [];
			listPagination = response.pagination;
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load join tokens';
			console.error('Failed to load join tokens:', e);
		} finally {
			loading = false;
		}
	}

	async function createToken() {
		const desc = newToken.description.trim();
		if (!desc) {
			error = 'Description is required';
			return;
		}
		creating = true;
		error = null;
		try {
			const response = await api.post<{ token: JoinToken; plain_token: string }>('/admin/join-tokens', {
				description: desc,
				scope: newToken.scope,
				expires_in_days: newToken.expiresInDays === 'never' ? null : parseInt(newToken.expiresInDays),
				labels: newToken.labels ? newToken.labels.split(',').map(l => l.trim()).filter(Boolean) : [],
				pool_tags: newToken.poolTags ? newToken.poolTags.split(',').map(t => t.trim()).filter(Boolean) : []
			});

			showCreateModal = false;
			newTokenValue = response.plain_token;
			page = 1;
			await loadTokens();
			newToken = { description: '', scope: 'tenant', expiresInDays: '30', labels: '', poolTags: '' };
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to create join token';
			console.error('Failed to create join token:', e);
		} finally {
			creating = false;
		}
	}

	function openRevokeConfirm(token: JoinToken) {
		tokenToRevoke = token;
	}

	function closeRevokeConfirm() {
		if (!revoking) tokenToRevoke = null;
	}

	async function confirmRevoke() {
		if (!tokenToRevoke) return;
		const id = tokenToRevoke.id;
		revoking = true;
		error = null;
		try {
			await api.post(`/admin/join-tokens/${id}/revoke`);
			tokens = tokens.map(t => (t.id === id ? { ...t, revoked: true } : t));
			tokenToRevoke = null;
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to revoke token';
			console.error('Failed to revoke token:', e);
		} finally {
			revoking = false;
		}
	}

	function openDeleteConfirm(token: JoinToken) {
		tokenToDelete = token;
	}

	function closeDeleteConfirm() {
		if (!deleting) tokenToDelete = null;
	}

	async function confirmDelete() {
		if (!tokenToDelete) return;
		const id = tokenToDelete.id;
		deleting = true;
		error = null;
		try {
			await api.delete(`/admin/join-tokens/${id}`);
			tokenToDelete = null;
			await loadTokens();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to delete token';
			console.error('Failed to delete join token:', e);
		} finally {
			deleting = false;
		}
	}

	function openEditDescription(token: JoinToken) {
		editModalToken = token;
		editDescriptionDraft = token.description;
	}

	function closeEditDescription() {
		if (!savingDescription) {
			editModalToken = null;
			editDescriptionDraft = '';
		}
	}

	async function saveDescription() {
		const desc = editDescriptionDraft.trim();
		if (!desc) {
			error = 'Description is required';
			return;
		}
		if (!editModalToken) return;
		savingDescription = true;
		error = null;
		try {
			const updated = await api.patch<JoinToken>(`/admin/join-tokens/${editModalToken.id}`, {
				description: desc
			});
			tokens = tokens.map(t => (t.id === updated.id ? updated : t));
			editModalToken = null;
			editDescriptionDraft = '';
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to update description';
			console.error('Failed to update join token description:', e);
		} finally {
			savingDescription = false;
		}
	}

	async function openHistoryModal(tokenId: string) {
		historyModalId = tokenId;
		historyDetail = null;
		historyLoading = true;
		error = null;
		try {
			historyDetail = await api.get<JoinToken>(`/admin/join-tokens/${tokenId}`);
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load token history';
			console.error('Failed to load join token:', e);
			historyModalId = null;
		} finally {
			historyLoading = false;
		}
	}

	function closeHistoryModal() {
		historyModalId = null;
		historyDetail = null;
	}

	function isExpired(expiresAt?: string): boolean {
		if (!expiresAt) return false;
		return new Date(expiresAt) < new Date();
	}

	function copyToken(tokenValue: string) {
		navigator.clipboard.writeText(tokenValue);
	}
</script>

<div class="space-y-8">
	<div>
		<h2 class="text-lg font-semibold text-[var(--text-primary)]">Agent Management</h2>
		<p class="text-sm text-[var(--text-secondary)]">Manage join tokens for agent registration</p>
	</div>

	{#if error}
		<div class="rounded-lg border border-red-200 bg-red-50 p-4 text-sm text-red-700 dark:border-red-800 dark:bg-red-950/50 dark:text-red-400">
			{error}
		</div>
	{/if}

	<div>
		<div class="flex flex-col gap-4 sm:flex-row sm:items-start sm:justify-between">
			<div>
				<h3 class="text-base font-medium text-[var(--text-primary)]">Join Tokens</h3>
				<p class="text-sm text-[var(--text-secondary)]">
					One-time enrollment tokens — each token allows a single successful agent registration
				</p>
			</div>
			<button
				type="button"
				onclick={() => (showCreateModal = true)}
				class="inline-flex shrink-0 items-center gap-2 rounded-lg bg-primary-600 px-4 py-2 text-sm font-medium text-white hover:bg-primary-700"
			>
				<Plus class="h-4 w-4" />
				Create Token
			</button>
		</div>

		<div class="mt-4 flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
			<div class="w-full sm:max-w-md">
				<label for="join-token-search" class="block text-xs font-medium text-[var(--text-secondary)]">
					Search description or token
				</label>
				<input
					id="join-token-search"
					type="search"
					placeholder="Filter by description or hash / prefix…"
					bind:value={searchDraft}
					class="mt-1 w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm text-[var(--text-primary)] placeholder:text-[var(--text-tertiary)] focus:border-primary-500 focus:outline-none focus:ring-1 focus:ring-primary-500"
				/>
			</div>
			<div class="flex items-center gap-2">
				<label for="join-token-per-page" class="text-sm text-[var(--text-secondary)]">Per page</label>
				<select
					id="join-token-per-page"
					value={perPage}
					onchange={(e) => {
						perPage = Number((e.currentTarget as HTMLSelectElement).value);
						page = 1;
					}}
					class="rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm text-[var(--text-primary)] focus:border-primary-500 focus:outline-none focus:ring-1 focus:ring-primary-500"
				>
					{#each PER_PAGE_OPTIONS as n}
						<option value={n}>{n}</option>
					{/each}
				</select>
			</div>
		</div>

		{#if loading}
			<div class="mt-4 flex items-center justify-center py-12">
				<div class="h-8 w-8 animate-spin rounded-full border-2 border-primary-500 border-t-transparent"></div>
			</div>
		{:else if tokens.length === 0}
			<div class="mt-4 flex flex-col items-center justify-center rounded-lg border border-dashed border-[var(--border-primary)] py-12">
				<Key class="h-12 w-12 text-[var(--text-tertiary)]" />
				<h3 class="mt-4 text-sm font-medium text-[var(--text-primary)]">No matching join tokens</h3>
				<p class="mt-1 text-sm text-[var(--text-secondary)]">
					{searchQ ? 'Try a different search or clear the filter.' : 'Create a join token to allow agents to register'}
				</p>
			</div>
		{:else}
			<div class="mt-4 overflow-x-auto overflow-hidden rounded-lg border border-[var(--border-primary)]">
				<table class="min-w-full divide-y divide-[var(--border-primary)]">
				<thead class="bg-[var(--bg-secondary)]">
					<tr>
						<th class="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-[var(--text-secondary)]">
							Description
						</th>
						<th class="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-[var(--text-secondary)]">
							Token
						</th>
						<th class="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-[var(--text-secondary)]">
							Scope
						</th>
						<th class="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-[var(--text-secondary)]">
							Created By
						</th>
						<th class="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-[var(--text-secondary)]">
							Created
						</th>
						<th class="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-[var(--text-secondary)]">
							Consumed
						</th>
						<th class="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-[var(--text-secondary)]">
							Agents
						</th>
						<th class="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-[var(--text-secondary)]">
							Status
						</th>
						<th class="px-4 py-3 text-right text-xs font-medium uppercase tracking-wider text-[var(--text-secondary)]">
							Actions
						</th>
					</tr>
				</thead>
					<tbody class="divide-y divide-[var(--border-primary)] bg-[var(--bg-primary)]">
						{#each tokens as token (token.id)}
							{@const expired = isExpired(token.expires_at)}
							{@const exhausted = token.current_uses >= token.max_uses}
							<tr class="hover:bg-[var(--bg-hover)]">
								<td class="max-w-xs px-4 py-3 text-sm text-[var(--text-primary)]">
									<div class="flex items-start gap-2">
										<span class="min-w-0 flex-1 break-words">{token.description}</span>
										<div class="flex shrink-0 gap-0.5">
											<button
												type="button"
												onclick={() => openEditDescription(token)}
												class="rounded p-1 text-[var(--text-tertiary)] hover:bg-[var(--bg-secondary)] hover:text-[var(--text-primary)]"
												title="Edit description"
											>
												<Pencil class="h-3.5 w-3.5" />
											</button>
											<button
												type="button"
												onclick={() => openHistoryModal(token.id)}
												class="rounded p-1 text-[var(--text-tertiary)] hover:bg-[var(--bg-secondary)] hover:text-[var(--text-primary)]"
												title="Description history"
											>
												<History class="h-3.5 w-3.5" />
											</button>
										</div>
									</div>
								</td>
								<td class="whitespace-nowrap px-4 py-3">
									<div class="flex items-center gap-3">
										<div class="flex h-8 w-8 items-center justify-center rounded bg-[var(--bg-secondary)]">
											<Key class="h-4 w-4 text-[var(--text-secondary)]" />
										</div>
										<div>
											<div class="font-mono text-sm text-[var(--text-primary)]">
												{token.prefix}
											</div>
											{#if token.labels.length > 0}
												<div class="text-xs text-[var(--text-tertiary)]">
													{token.labels.join(', ')}
												</div>
											{/if}
										</div>
									</div>
								</td>
							<td class="whitespace-nowrap px-4 py-3">
								<span class="rounded-full bg-gray-100 px-2 py-0.5 text-xs font-medium capitalize text-gray-700 dark:bg-gray-800 dark:text-gray-300">
									{token.scope}
								</span>
							</td>
							<td class="whitespace-nowrap px-4 py-3 text-sm text-[var(--text-secondary)]">
								{token.created_by_name || token.created_by}
							</td>
							<td class="whitespace-nowrap px-4 py-3 text-sm text-[var(--text-secondary)]">
								{new Date(token.created_at).toLocaleDateString()}
							</td>
							<td class="px-4 py-3 text-sm text-[var(--text-secondary)]">
								{#if token.consumed_at}
									<div title={token.consumed_by_agent_id ?? ''}>
										{new Date(token.consumed_at).toLocaleString()}
									</div>
									<div class="text-xs text-[var(--text-tertiary)]">
										{token.current_uses} / {token.max_uses} use
									</div>
								{:else}
									<span class="text-[var(--text-tertiary)]">—</span>
									<div class="mt-0.5 text-xs text-[var(--text-tertiary)]">
										{token.current_uses} / {token.max_uses} use
									</div>
								{/if}
							</td>
							<td class="px-4 py-3">
								{#if token.agents.length === 0}
									<span class="text-sm text-[var(--text-tertiary)]">None</span>
								{:else}
									<div class="flex flex-col gap-1">
										{#each token.agents as agent}
											<div class="flex items-center gap-1.5">
												<Server class="h-3 w-3 text-[var(--text-tertiary)]" />
												<span class="text-sm text-[var(--text-primary)]">{agent.name}</span>
												<span class="rounded-full px-1.5 py-0.5 text-[10px] font-medium capitalize {agent.status === 'online' ? 'bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-400' : 'bg-gray-100 text-gray-600 dark:bg-gray-800 dark:text-gray-400'}">
													{agent.status}
												</span>
											</div>
										{/each}
									</div>
								{/if}
							</td>
								<td class="whitespace-nowrap px-4 py-3">
									{#if token.revoked}
										<span class="inline-flex items-center gap-1.5 text-sm text-gray-500">
											<XCircle class="h-4 w-4" />
											Revoked
										</span>
									{:else if expired}
										<span class="inline-flex items-center gap-1.5 text-sm text-red-600 dark:text-red-400">
											<XCircle class="h-4 w-4" />
											Expired
										</span>
									{:else if exhausted}
										<span class="inline-flex items-center gap-1.5 text-sm text-gray-500">
											<XCircle class="h-4 w-4" />
											Used
										</span>
									{:else}
										<span class="inline-flex items-center gap-1.5 text-sm text-green-600 dark:text-green-400">
											<CheckCircle class="h-4 w-4" />
											Active
										</span>
									{/if}
								</td>
								<td class="whitespace-nowrap px-4 py-3 text-right">
									<div class="flex items-center justify-end gap-1">
										{#if !token.revoked}
											<button
												type="button"
												onclick={() => openRevokeConfirm(token)}
												class="rounded p-1.5 text-[var(--text-secondary)] hover:bg-amber-50 hover:text-amber-700 dark:hover:bg-amber-950/50"
												title="Revoke — token cannot be used to register"
											>
												<Ban class="h-4 w-4" />
											</button>
										{/if}
										<button
											type="button"
											onclick={() => openDeleteConfirm(token)}
											class="rounded p-1.5 text-[var(--text-secondary)] hover:bg-red-50 hover:text-red-600 dark:hover:bg-red-950/50"
											title="Delete — permanently remove this record"
										>
											<Trash2 class="h-4 w-4" />
										</button>
									</div>
								</td>
							</tr>
						{/each}
					</tbody>
				</table>
			</div>
			{#if listPagination && listPagination.total > 0}
				<div
					class="mt-0 flex flex-col gap-3 border-t border-[var(--border-primary)] bg-[var(--bg-secondary)] px-4 py-3 sm:flex-row sm:items-center sm:justify-between"
				>
					<p class="text-sm text-[var(--text-secondary)]">
						Showing {(page - 1) * perPage + 1}–{Math.min(page * perPage, listPagination.total)} of {listPagination.total}
					</p>
					<div class="flex items-center gap-2">
						<button
							type="button"
							disabled={page <= 1}
							onclick={() => {
								page -= 1;
							}}
							class="rounded-lg border border-[var(--border-primary)] px-3 py-1.5 text-sm font-medium text-[var(--text-primary)] hover:bg-[var(--bg-primary)] disabled:cursor-not-allowed disabled:opacity-40"
						>
							Previous
						</button>
						<span class="text-sm text-[var(--text-tertiary)]">Page {page}</span>
						<button
							type="button"
							disabled={!listPagination.has_more}
							onclick={() => {
								page += 1;
							}}
							class="rounded-lg border border-[var(--border-primary)] px-3 py-1.5 text-sm font-medium text-[var(--text-primary)] hover:bg-[var(--bg-primary)] disabled:cursor-not-allowed disabled:opacity-40"
						>
							Next
						</button>
					</div>
				</div>
			{/if}
		{/if}
	</div>

	<div class="rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] p-4">
		<h3 class="font-medium text-[var(--text-primary)]">Agent Registration</h3>
		<p class="mt-1 text-sm text-[var(--text-secondary)]">
			To register an agent, run it with a join token:
		</p>
		<pre class="mt-3 overflow-x-auto rounded-lg bg-[var(--bg-primary)] p-3 text-sm text-[var(--text-primary)]"><code>podman run -e MET_CONTROLLER_URL=http://your-server:9090 \
           -e MET_JOIN_TOKEN=met_join_xxx \
           meticulous/agent:latest</code></pre>
	</div>
</div>

{#if showCreateModal}
	<div class="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
		<div class="w-full max-w-md rounded-lg bg-[var(--bg-primary)] p-6 shadow-xl">
			<h3 class="text-lg font-semibold text-[var(--text-primary)]">Create Join Token</h3>
			<p class="mt-1 text-sm text-[var(--text-secondary)]">
				Tokens are single-use: one successful registration consumes the token.
			</p>

			<form onsubmit={(e) => { e.preventDefault(); createToken(); }} class="mt-6 space-y-4">
				<div>
					<label for="description" class="block text-sm font-medium text-[var(--text-primary)]">
						Description <span class="text-red-500">*</span>
					</label>
					<input
						id="description"
						type="text"
						required
						bind:value={newToken.description}
						placeholder="e.g. CI runners — prod cluster"
						class="mt-1 w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm focus:border-primary-500 focus:outline-none focus:ring-1 focus:ring-primary-500"
					/>
				</div>

				<div>
					<label for="scope" class="block text-sm font-medium text-[var(--text-primary)]">
						Scope
					</label>
					<select
						id="scope"
						bind:value={newToken.scope}
						class="mt-1 w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm focus:border-primary-500 focus:outline-none focus:ring-1 focus:ring-primary-500"
					>
						<option value="tenant">Tenant (All projects in organization)</option>
						<option value="platform">Platform (All organizations)</option>
					</select>
				</div>

				<div>
					<label for="expiresInDays" class="block text-sm font-medium text-[var(--text-primary)]">
						Expiration
					</label>
					<select
						id="expiresInDays"
						bind:value={newToken.expiresInDays}
						class="mt-1 w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm focus:border-primary-500 focus:outline-none focus:ring-1 focus:ring-primary-500"
					>
						<option value="7">7 days</option>
						<option value="30">30 days</option>
						<option value="90">90 days</option>
						<option value="365">1 year</option>
						<option value="never">Never</option>
					</select>
				</div>

				<div>
					<label for="labels" class="block text-sm font-medium text-[var(--text-primary)]">
						Labels
					</label>
					<input
						id="labels"
						type="text"
						bind:value={newToken.labels}
						placeholder="e.g., environment:prod, team:platform"
						class="mt-1 w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm focus:border-primary-500 focus:outline-none focus:ring-1 focus:ring-primary-500"
					/>
					<p class="mt-1 text-xs text-[var(--text-tertiary)]">Comma-separated labels to apply to agents</p>
				</div>

				<div>
					<label for="poolTags" class="block text-sm font-medium text-[var(--text-primary)]">
						Pool tags
					</label>
					<input
						id="poolTags"
						type="text"
						bind:value={newToken.poolTags}
						placeholder="e.g. docker, gpu"
						class="mt-1 w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm focus:border-primary-500 focus:outline-none focus:ring-1 focus:ring-primary-500"
					/>
				</div>

				<div class="flex justify-end gap-3 pt-4">
					<button
						type="button"
						onclick={() => (showCreateModal = false)}
						disabled={creating}
						class="rounded-lg border border-[var(--border-primary)] px-4 py-2 text-sm font-medium text-[var(--text-primary)] hover:bg-[var(--bg-secondary)] disabled:opacity-50"
					>
						Cancel
					</button>
					<button
						type="submit"
						disabled={creating}
						class="rounded-lg bg-primary-600 px-4 py-2 text-sm font-medium text-white hover:bg-primary-700 disabled:opacity-50"
					>
						{creating ? 'Creating...' : 'Create Token'}
					</button>
				</div>
			</form>
		</div>
	</div>
{/if}

{#if tokenToDelete}
	<div class="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
		<div class="w-full max-w-md rounded-lg bg-[var(--bg-primary)] p-6 shadow-xl">
			<h3 class="text-lg font-semibold text-[var(--text-primary)]">Delete join token?</h3>
			<p class="mt-2 text-sm text-[var(--text-secondary)]">
				This permanently removes the token record. Agents that enrolled with it will no longer be linked to this
				token. This cannot be undone.
			</p>
			{#if tokenToDelete}
				<p class="mt-3 rounded-lg bg-[var(--bg-secondary)] px-3 py-2 font-mono text-sm text-[var(--text-primary)]">
					{tokenToDelete.prefix}
				</p>
				<p class="mt-2 text-sm text-[var(--text-secondary)]">{tokenToDelete.description}</p>
			{/if}
			<div class="mt-6 flex justify-end gap-3">
				<button
					type="button"
					onclick={closeDeleteConfirm}
					disabled={deleting}
					class="rounded-lg border border-[var(--border-primary)] px-4 py-2 text-sm font-medium text-[var(--text-primary)] hover:bg-[var(--bg-secondary)] disabled:opacity-50"
				>
					Cancel
				</button>
				<button
					type="button"
					onclick={confirmDelete}
					disabled={deleting}
					class="rounded-lg bg-red-600 px-4 py-2 text-sm font-medium text-white hover:bg-red-700 disabled:opacity-50"
				>
					{deleting ? 'Deleting…' : 'Delete permanently'}
				</button>
			</div>
		</div>
	</div>
{/if}

{#if tokenToRevoke}
	<div class="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
		<div class="w-full max-w-md rounded-lg bg-[var(--bg-primary)] p-6 shadow-xl">
			<h3 class="text-lg font-semibold text-[var(--text-primary)]">Revoke join token?</h3>
			<p class="mt-2 text-sm text-[var(--text-secondary)]">
				The token will be marked revoked and cannot be used for new registrations. You can still see it in this
				list until you delete it.
			</p>
			{#if tokenToRevoke}
				<p class="mt-3 rounded-lg bg-[var(--bg-secondary)] px-3 py-2 font-mono text-sm text-[var(--text-primary)]">
					{tokenToRevoke.prefix}
				</p>
				<p class="mt-2 text-sm text-[var(--text-secondary)]">{tokenToRevoke.description}</p>
			{/if}
			<div class="mt-6 flex justify-end gap-3">
				<button
					type="button"
					onclick={closeRevokeConfirm}
					disabled={revoking}
					class="rounded-lg border border-[var(--border-primary)] px-4 py-2 text-sm font-medium text-[var(--text-primary)] hover:bg-[var(--bg-secondary)] disabled:opacity-50"
				>
					Cancel
				</button>
				<button
					type="button"
					onclick={confirmRevoke}
					disabled={revoking}
					class="rounded-lg bg-red-600 px-4 py-2 text-sm font-medium text-white hover:bg-red-700 disabled:opacity-50"
				>
					{revoking ? 'Revoking…' : 'Revoke token'}
				</button>
			</div>
		</div>
	</div>
{/if}

{#if editModalToken}
	<div class="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
		<div class="w-full max-w-md rounded-lg bg-[var(--bg-primary)] p-6 shadow-xl">
			<h3 class="text-lg font-semibold text-[var(--text-primary)]">Edit description</h3>
			<p class="mt-1 text-sm text-[var(--text-secondary)]">
				Description is required. Changes are recorded in the history.
			</p>
			<form
				onsubmit={(e) => {
					e.preventDefault();
					saveDescription();
				}}
				class="mt-4 space-y-4"
			>
				<div>
					<label for="edit-description" class="block text-sm font-medium text-[var(--text-primary)]">
						Description <span class="text-red-500">*</span>
					</label>
					<textarea
						id="edit-description"
						rows="3"
						required
						bind:value={editDescriptionDraft}
						class="mt-1 w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm focus:border-primary-500 focus:outline-none focus:ring-1 focus:ring-primary-500"
					></textarea>
				</div>
				<div class="flex justify-end gap-3 pt-2">
					<button
						type="button"
						onclick={closeEditDescription}
						disabled={savingDescription}
						class="rounded-lg border border-[var(--border-primary)] px-4 py-2 text-sm font-medium text-[var(--text-primary)] hover:bg-[var(--bg-secondary)] disabled:opacity-50"
					>
						Cancel
					</button>
					<button
						type="submit"
						disabled={savingDescription}
						class="rounded-lg bg-primary-600 px-4 py-2 text-sm font-medium text-white hover:bg-primary-700 disabled:opacity-50"
					>
						{savingDescription ? 'Saving…' : 'Save'}
					</button>
				</div>
			</form>
		</div>
	</div>
{/if}

{#if historyModalId}
	<div class="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
		<div class="flex max-h-[85vh] w-full max-w-lg flex-col rounded-lg bg-[var(--bg-primary)] shadow-xl">
			<div class="border-b border-[var(--border-primary)] p-6 pb-4">
				<h3 class="text-lg font-semibold text-[var(--text-primary)]">Description history</h3>
				{#if historyDetail}
					<p class="mt-1 font-mono text-sm text-[var(--text-secondary)]">{historyDetail.prefix}</p>
				{/if}
			</div>
			<div class="min-h-0 flex-1 overflow-y-auto px-6 pb-6">
				{#if historyLoading}
					<div class="flex justify-center py-10">
						<div class="h-8 w-8 animate-spin rounded-full border-2 border-primary-500 border-t-transparent"></div>
					</div>
				{:else if historyDetail}
					{#if (historyDetail.description_history?.length ?? 0) === 0}
						<p class="py-4 text-sm text-[var(--text-secondary)]">No history entries yet.</p>
					{:else}
						<ol class="space-y-4 border-l border-[var(--border-primary)] pl-4">
							{#each historyDetail.description_history ?? [] as entry}
								<li class="relative">
									<span
										class="absolute -left-[21px] top-1.5 h-2.5 w-2.5 rounded-full bg-primary-500 ring-4 ring-[var(--bg-primary)]"
									></span>
									<p class="text-sm text-[var(--text-primary)]">{entry.description}</p>
									<p class="mt-1 text-xs text-[var(--text-tertiary)]">
										{new Date(entry.changed_at).toLocaleString()}
										{#if entry.changed_by_name || entry.changed_by}
											<span class="text-[var(--text-secondary)]">
												· {entry.changed_by_name ?? entry.changed_by}
											</span>
										{/if}
									</p>
								</li>
							{/each}
						</ol>
					{/if}
				{/if}
			</div>
			<div class="border-t border-[var(--border-primary)] p-4">
				<div class="flex justify-end">
					<button
						type="button"
						onclick={closeHistoryModal}
						class="rounded-lg bg-primary-600 px-4 py-2 text-sm font-medium text-white hover:bg-primary-700"
					>
						Close
					</button>
				</div>
			</div>
		</div>
	</div>
{/if}

{#if newTokenValue}
	<div class="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
		<div class="w-full max-w-lg rounded-lg bg-[var(--bg-primary)] p-6 shadow-xl">
			<h3 class="text-lg font-semibold text-[var(--text-primary)]">Token Created</h3>

			<div class="mt-4 rounded-lg border border-amber-200 bg-amber-50 p-4 dark:border-amber-800 dark:bg-amber-950/50">
				<p class="text-sm font-medium text-amber-800 dark:text-amber-200">
					Copy this token now. You won't be able to see it again!
				</p>
			</div>

			<div class="mt-4 flex items-center gap-2 rounded-lg bg-[var(--bg-tertiary)] p-3">
				<code class="flex-1 break-all font-mono text-sm text-[var(--text-primary)]">{newTokenValue}</code>
				<button
					type="button"
					onclick={() => copyToken(newTokenValue!)}
					class="rounded p-2 text-[var(--text-secondary)] hover:bg-[var(--bg-secondary)] hover:text-[var(--text-primary)]"
					title="Copy token"
				>
					<Copy class="h-4 w-4" />
				</button>
			</div>

			<div class="mt-6 flex justify-end">
				<button
					type="button"
					onclick={() => (newTokenValue = null)}
					class="rounded-lg bg-primary-600 px-4 py-2 text-sm font-medium text-white hover:bg-primary-700"
				>
					Done
				</button>
			</div>
		</div>
	</div>
{/if}
