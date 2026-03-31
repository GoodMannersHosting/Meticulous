<script lang="ts">
	import { Server, Plus, Key, Copy, Trash2, Clock, CheckCircle, XCircle } from 'lucide-svelte';

	interface JoinToken {
		id: string;
		name: string;
		prefix: string;
		scope: 'platform' | 'project' | 'pipeline';
		scopeId?: string;
		maxUses?: number;
		usedCount: number;
		expiresAt?: string;
		createdAt: string;
	}

	let tokens = $state<JoinToken[]>([]);
	let loading = $state(true);
	let showCreateModal = $state(false);

	$effect(() => {
		loading = false;
	});

	function isExpired(expiresAt?: string): boolean {
		if (!expiresAt) return false;
		return new Date(expiresAt) < new Date();
	}

	function copyToken(token: JoinToken) {
		navigator.clipboard.writeText(`met_join_${token.prefix}...`);
	}
</script>

<div class="space-y-8">
	<div>
		<h2 class="text-lg font-semibold text-[var(--text-primary)]">Agent Management</h2>
		<p class="text-sm text-[var(--text-secondary)]">Manage join tokens for agent registration</p>
	</div>

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
							{@const expired = isExpired(token.expiresAt)}
							<tr class="hover:bg-[var(--bg-hover)]">
								<td class="whitespace-nowrap px-4 py-3">
									<div class="flex items-center gap-3">
										<div class="flex h-8 w-8 items-center justify-center rounded bg-[var(--bg-secondary)]">
											<Key class="h-4 w-4 text-[var(--text-secondary)]" />
										</div>
										<div>
											<div class="font-medium text-[var(--text-primary)]">{token.name}</div>
											<div class="font-mono text-xs text-[var(--text-secondary)]">
												met_join_{token.prefix}...
											</div>
										</div>
									</div>
								</td>
								<td class="whitespace-nowrap px-4 py-3">
									<span class="rounded-full bg-gray-100 px-2 py-0.5 text-xs font-medium capitalize text-gray-700 dark:bg-gray-800 dark:text-gray-300">
										{token.scope}
									</span>
								</td>
								<td class="whitespace-nowrap px-4 py-3 text-sm text-[var(--text-secondary)]">
									{token.usedCount}{token.maxUses ? ` / ${token.maxUses}` : ''} uses
								</td>
								<td class="whitespace-nowrap px-4 py-3">
									{#if expired}
										<span class="inline-flex items-center gap-1.5 text-sm text-red-600 dark:text-red-400">
											<XCircle class="h-4 w-4" />
											Expired
										</span>
									{:else if token.maxUses && token.usedCount >= token.maxUses}
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
											onclick={() => copyToken(token)}
											class="rounded p-1.5 text-[var(--text-secondary)] hover:bg-[var(--bg-secondary)] hover:text-[var(--text-primary)]"
											title="Copy token"
										>
											<Copy class="h-4 w-4" />
										</button>
										<button
											type="button"
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
