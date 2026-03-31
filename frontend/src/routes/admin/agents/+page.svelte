<script lang="ts">
	import { Server, Plus, Key, Copy, Trash2, Clock, CheckCircle, XCircle } from 'lucide-svelte';
	import { api } from '$lib/api';

	interface JoinTokenAgent {
		id: string;
		name: string;
		status: string;
		registered_at: string;
	}

	interface JoinToken {
		id: string;
		prefix: string;
		scope: string;
		scope_id?: string;
		max_uses?: number;
		current_uses: number;
		labels: string[];
		pool_tags: string[];
		expires_at?: string;
		revoked: boolean;
		created_by: string;
		created_by_name?: string;
		created_at: string;
		agents: JoinTokenAgent[];
	}

	let tokens = $state<JoinToken[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let showCreateModal = $state(false);
	let creating = $state(false);
	let newTokenValue = $state<string | null>(null);

	let newToken = $state({
		scope: 'tenant',
		maxUses: '' as string | number,
		expiresInDays: '30',
		labels: '',
		poolTags: ''
	});

	$effect(() => {
		loadTokens();
	});

	async function loadTokens() {
		loading = true;
		error = null;
		try {
			const response = await api.get<{ items: JoinToken[] }>('/admin/join-tokens');
			tokens = response.items || [];
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load join tokens';
			console.error('Failed to load join tokens:', e);
		} finally {
			loading = false;
		}
	}

	async function createToken() {
		creating = true;
		error = null;
		try {
			const response = await api.post<{ token: JoinToken; plain_token: string }>('/admin/join-tokens', {
				scope: newToken.scope,
				max_uses: newToken.maxUses ? parseInt(newToken.maxUses.toString()) : null,
				expires_in_days: newToken.expiresInDays === 'never' ? null : parseInt(newToken.expiresInDays),
				labels: newToken.labels ? newToken.labels.split(',').map(l => l.trim()).filter(Boolean) : [],
				pool_tags: newToken.poolTags ? newToken.poolTags.split(',').map(t => t.trim()).filter(Boolean) : []
			});
			
			showCreateModal = false;
			newTokenValue = response.plain_token;
			tokens = [response.token, ...tokens];
			newToken = { scope: 'tenant', maxUses: '', expiresInDays: '30', labels: '', poolTags: '' };
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to create join token';
			console.error('Failed to create join token:', e);
		} finally {
			creating = false;
		}
	}

	async function revokeToken(id: string) {
		try {
			await api.delete(`/admin/join-tokens/${id}`);
			tokens = tokens.filter(t => t.id !== id);
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to revoke token';
			console.error('Failed to revoke token:', e);
		}
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
		<div class="flex items-center justify-between">
			<div>
				<h3 class="text-base font-medium text-[var(--text-primary)]">Join Tokens</h3>
				<p class="text-sm text-[var(--text-secondary)]">Create tokens for agents to register with the platform</p>
			</div>
			<button
				type="button"
				onclick={() => (showCreateModal = true)}
				class="inline-flex items-center gap-2 rounded-lg bg-primary-600 px-4 py-2 text-sm font-medium text-white hover:bg-primary-700"
			>
				<Plus class="h-4 w-4" />
				Create Token
			</button>
		</div>

		{#if loading}
			<div class="mt-4 flex items-center justify-center py-12">
				<div class="h-8 w-8 animate-spin rounded-full border-2 border-primary-500 border-t-transparent"></div>
			</div>
		{:else if tokens.length === 0}
			<div class="mt-4 flex flex-col items-center justify-center rounded-lg border border-dashed border-[var(--border-primary)] py-12">
				<Key class="h-12 w-12 text-[var(--text-tertiary)]" />
				<h3 class="mt-4 text-sm font-medium text-[var(--text-primary)]">No join tokens</h3>
				<p class="mt-1 text-sm text-[var(--text-secondary)]">
					Create a join token to allow agents to register
				</p>
			</div>
		{:else}
			<div class="mt-4 overflow-hidden rounded-lg border border-[var(--border-primary)]">
				<table class="min-w-full divide-y divide-[var(--border-primary)]">
				<thead class="bg-[var(--bg-secondary)]">
					<tr>
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
							Agents
						</th>
						<th class="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-[var(--text-secondary)]">
							Usage
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
							{@const exhausted = token.max_uses !== null && token.current_uses >= (token.max_uses || 0)}
							<tr class="hover:bg-[var(--bg-hover)]">
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
							<td class="whitespace-nowrap px-4 py-3 text-sm text-[var(--text-secondary)]">
								{token.current_uses}{token.max_uses ? ` / ${token.max_uses}` : ''} uses
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
											Exhausted
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
										<button
											type="button"
											onclick={() => revokeToken(token.id)}
											class="rounded p-1.5 text-[var(--text-secondary)] hover:bg-red-50 hover:text-red-600 dark:hover:bg-red-950/50"
											title="Revoke token"
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

<!-- Create Token Modal -->
{#if showCreateModal}
	<div class="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
		<div class="w-full max-w-md rounded-lg bg-[var(--bg-primary)] p-6 shadow-xl">
			<h3 class="text-lg font-semibold text-[var(--text-primary)]">Create Join Token</h3>
			<p class="mt-1 text-sm text-[var(--text-secondary)]">
				Create a new token for agents to register with the platform.
			</p>

			<form onsubmit={(e) => { e.preventDefault(); createToken(); }} class="mt-6 space-y-4">
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
					<label for="maxUses" class="block text-sm font-medium text-[var(--text-primary)]">
						Maximum Uses
					</label>
					<input
						id="maxUses"
						type="number"
						min="1"
						bind:value={newToken.maxUses}
						placeholder="Unlimited"
						class="mt-1 w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm focus:border-primary-500 focus:outline-none focus:ring-1 focus:ring-primary-500"
					/>
					<p class="mt-1 text-xs text-[var(--text-tertiary)]">Leave empty for unlimited uses</p>
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

<!-- New Token Display Modal -->
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
