<script lang="ts">
	import { Button, Card, Badge, Dialog, Alert } from '$components/ui';
	import { Skeleton, EmptyState } from '$components/data';
	import { Key, Plus, Trash2, Shield, Ban, RotateCcw } from 'lucide-svelte';
	import { api } from '$lib/api';
	import { auth } from '$stores';

	interface ApiToken {
		id: string;
		name: string;
		description?: string;
		prefix: string;
		scopes: string[];
		project_ids?: string[];
		pipeline_ids?: string[];
		created_at: string;
		last_used_at?: string;
		expires_at?: string;
		deactivated_at?: string;
		revoked_at?: string;
	}

	let tokens = $state<ApiToken[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let showDeleteDialog = $state(false);
	let tokenToDelete = $state<ApiToken | null>(null);
	let deleting = $state(false);
	let actionTokenId = $state<string | null>(null);

	const createTokenHref = '/settings/api-token/new';

	$effect(() => {
		loadTokens();
	});

	function formatScopeLabel(scope: string): string {
		if (scope === 'write') return 'Operator';
		return scope.charAt(0).toUpperCase() + scope.slice(1);
	}

	async function loadTokens() {
		loading = true;
		error = null;
		try {
			const response = await api.get<{ data: ApiToken[] }>('/api/v1/tokens');
			tokens = response.data ?? [];
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load tokens';
			console.error('Failed to load tokens:', e);
		} finally {
			loading = false;
		}
	}

	function tokenState(t: ApiToken): 'revoked' | 'expired' | 'deactivated' | 'active' {
		if (t.revoked_at) return 'revoked';
		if (t.expires_at && new Date(t.expires_at) < new Date()) return 'expired';
		if (t.deactivated_at) return 'deactivated';
		return 'active';
	}

	async function deactivateToken(t: ApiToken) {
		actionTokenId = t.id;
		error = null;
		try {
			const updated = await api.post<ApiToken>(`/api/v1/tokens/${t.id}/deactivate`, {});
			tokens = tokens.map((x) => (x.id === updated.id ? { ...x, ...updated } : x));
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to deactivate';
		} finally {
			actionTokenId = null;
		}
	}

	async function reactivateToken(t: ApiToken) {
		actionTokenId = t.id;
		error = null;
		try {
			const updated = await api.post<ApiToken>(`/api/v1/tokens/${t.id}/reactivate`, {});
			tokens = tokens.map((x) => (x.id === updated.id ? { ...x, ...updated } : x));
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to reactivate';
		} finally {
			actionTokenId = null;
		}
	}

	async function revokeToken(t: ApiToken) {
		if (!confirm(`Permanently revoke “${t.name}”? It will stop working immediately.`)) return;
		actionTokenId = t.id;
		error = null;
		try {
			await api.post(`/api/v1/tokens/${t.id}/revoke`, {});
			await loadTokens();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to revoke';
		} finally {
			actionTokenId = null;
		}
	}

	function confirmDeleteToken(token: ApiToken) {
		tokenToDelete = token;
		showDeleteDialog = true;
	}

	async function deleteToken() {
		if (!tokenToDelete) return;

		deleting = true;
		error = null;
		try {
			await api.delete(`/api/v1/tokens/${tokenToDelete.id}`);
			tokens = tokens.filter((t) => t.id !== tokenToDelete!.id);
			showDeleteDialog = false;
			tokenToDelete = null;
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to delete token';
			console.error('Failed to delete token:', e);
		} finally {
			deleting = false;
		}
	}

	function formatDate(date: string): string {
		return new Date(date).toLocaleDateString('en-US', {
			year: 'numeric',
			month: 'short',
			day: 'numeric'
		});
	}
</script>

<div class="space-y-6">
	<div class="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
		<div>
			<h3 class="text-lg font-medium text-[var(--text-primary)]">Security</h3>
			<p class="mt-1 text-sm text-[var(--text-secondary)]">
				Manage API tokens. At most two may be active at once. Deactivate a token before deleting it.
			</p>
			{#if auth.user?.role === 'admin'}
				<p class="mt-2 text-sm">
					<a href="/admin/policy" class="text-primary-600 hover:underline dark:text-primary-400">
						Organization token policy & admin token list
					</a>
				</p>
			{/if}
		</div>

		<Button variant="primary" href={createTokenHref}>
			<Plus class="h-4 w-4" />
			New API Token
		</Button>
	</div>

	{#if error}
		<Alert variant="error" title="Error">
			{error}
		</Alert>
	{/if}

	<Card>
		<div class="mb-4 flex items-center gap-3">
			<div class="flex h-10 w-10 items-center justify-center rounded-lg bg-[var(--bg-tertiary)]">
				<Key class="h-5 w-5 text-[var(--text-secondary)]" />
			</div>
			<div>
				<h3 class="font-medium text-[var(--text-primary)]">API Tokens</h3>
				<p class="text-sm text-[var(--text-secondary)]">
					Tokens for authenticating with the API (
					<code class="text-xs">Authorization: Token met_…</code>
					)
				</p>
			</div>
		</div>

		{#if loading}
			<div class="space-y-4">
				{#each Array(3) as _, i (i)}
					<div class="flex items-center gap-4 rounded-lg border border-[var(--border-primary)] p-4">
						<Skeleton class="h-5 w-32" />
						<Skeleton class="h-5 w-24 rounded-full" />
						<div class="flex-1"></div>
						<Skeleton class="h-5 w-24" />
						<Skeleton class="h-8 w-8" />
					</div>
				{/each}
			</div>
		{:else if tokens.length === 0}
			<EmptyState
				title="No API tokens"
				description="Create an API token to authenticate with the Meticulous API."
			>
				<Button variant="primary" href={createTokenHref}>
					<Plus class="h-4 w-4" />
					Create Token
				</Button>
			</EmptyState>
		{:else}
			<div class="space-y-3">
				{#each tokens as token (token.id)}
					{@const st = tokenState(token)}
					<div class="flex flex-col gap-3 rounded-lg border border-[var(--border-primary)] p-4 sm:flex-row sm:items-center">
						<div class="flex-1">
							<div class="flex flex-wrap items-center gap-2">
								<span class="font-medium text-[var(--text-primary)]">{token.name}</span>
								<code class="text-xs text-[var(--text-tertiary)]">{token.prefix}…</code>
								{#if st === 'active'}
									<Badge variant="success" size="sm">Active</Badge>
								{:else if st === 'deactivated'}
									<Badge variant="secondary" size="sm">Deactivated</Badge>
								{:else if st === 'expired'}
									<Badge variant="outline" size="sm">Expired</Badge>
								{:else}
									<Badge variant="error" size="sm">Revoked</Badge>
								{/if}
							</div>
							{#if token.description}
								<p class="mt-0.5 text-sm text-[var(--text-secondary)]">{token.description}</p>
							{/if}
							<div class="mt-1 flex flex-wrap gap-1">
								{#each token.scopes as scope (scope)}
									<Badge variant="outline" size="sm">{formatScopeLabel(scope)}</Badge>
								{/each}
							</div>
							{#if token.project_ids && token.project_ids.length > 0}
								<p class="mt-2 text-xs text-[var(--text-tertiary)]">
									Projects:
									{#each token.project_ids as pid, i (pid)}
										<code class="mx-0.5 rounded bg-[var(--bg-tertiary)] px-1">{pid}</code>{#if i < token.project_ids!.length - 1}, {/if}
									{/each}
								</p>
							{/if}
							{#if token.pipeline_ids && token.pipeline_ids.length > 0}
								<p class="mt-1 text-xs text-[var(--text-tertiary)]">
									Pipelines:
									{#each token.pipeline_ids as pid, i (pid)}
										<code class="mx-0.5 rounded bg-[var(--bg-tertiary)] px-1">{pid}</code>{#if i < token.pipeline_ids!.length - 1}, {/if}
									{/each}
								</p>
							{/if}
						</div>
						<div class="text-right text-sm text-[var(--text-secondary)]">
							{#if token.last_used_at}
								<p>Last used {formatDate(token.last_used_at)}</p>
							{:else}
								<p>Never used</p>
							{/if}
							<p class="text-xs text-[var(--text-tertiary)]">
								Created {formatDate(token.created_at)}
							</p>
						</div>
						<div class="flex flex-wrap justify-end gap-1">
							{#if st === 'active'}
								<Button
									variant="outline"
									size="sm"
									disabled={actionTokenId === token.id}
									onclick={() => deactivateToken(token)}
								>
									<Ban class="h-4 w-4" />
									Deactivate
								</Button>
								<Button
									variant="ghost"
									size="sm"
									class="text-amber-700 dark:text-amber-400"
									disabled={actionTokenId === token.id}
									onclick={() => revokeToken(token)}
								>
									Revoke
								</Button>
							{:else if st === 'deactivated' && !token.revoked_at}
								<Button
									variant="outline"
									size="sm"
									disabled={actionTokenId === token.id}
									onclick={() => reactivateToken(token)}
								>
									<RotateCcw class="h-4 w-4" />
									Activate
								</Button>
								<Button
									variant="ghost"
									size="sm"
									onclick={() => confirmDeleteToken(token)}
									title="Remove metadata after deactivating"
								>
									<Trash2 class="h-4 w-4 text-error-500" />
								</Button>
							{/if}
						</div>
					</div>
				{/each}
			</div>
		{/if}
	</Card>

	<Card>
		<div class="flex items-center gap-3">
			<div class="flex h-10 w-10 items-center justify-center rounded-lg bg-[var(--bg-tertiary)]">
				<Shield class="h-5 w-5 text-[var(--text-secondary)]" />
			</div>
			<div class="flex-1">
				<h3 class="font-medium text-[var(--text-primary)]">Two-Factor Authentication</h3>
				<p class="text-sm text-[var(--text-secondary)]">
					Add an extra layer of security to your account
				</p>
			</div>
			<Badge variant="secondary">Not enabled</Badge>
			<Button variant="outline" size="sm">
				Enable
			</Button>
		</div>
	</Card>

	<Card>
		<div class="space-y-4">
			<div class="flex items-center gap-3">
				<div class="flex h-10 w-10 items-center justify-center rounded-lg bg-[var(--bg-tertiary)]">
					<Key class="h-5 w-5 text-[var(--text-secondary)]" />
				</div>
				<div class="flex-1">
					<h3 class="font-medium text-[var(--text-primary)]">OIDC Workload Identity</h3>
					<p class="text-sm text-[var(--text-secondary)]">
						Machine-to-machine authentication for pipeline jobs using short-lived OIDC tokens
					</p>
				</div>
				<Badge variant="outline">ES256 / P-256</Badge>
			</div>
			<div class="rounded-lg border border-[var(--border-primary)] bg-[var(--bg-tertiary)] p-4 space-y-3">
				<div class="grid gap-4 text-sm sm:grid-cols-2">
					<div>
						<p class="text-xs text-[var(--text-tertiary)]">Discovery endpoint</p>
						<code class="text-xs text-[var(--text-primary)]">/.well-known/openid-configuration</code>
					</div>
					<div>
						<p class="text-xs text-[var(--text-tertiary)]">JWKS endpoint</p>
						<code class="text-xs text-[var(--text-primary)]">/.well-known/jwks.json</code>
					</div>
					<div>
						<p class="text-xs text-[var(--text-tertiary)]">Token lifetime</p>
						<p class="text-[var(--text-primary)]">5 minutes (max 15 min)</p>
					</div>
					<div>
						<p class="text-xs text-[var(--text-tertiary)]">Key rotation</p>
						<p class="text-[var(--text-primary)]">Every 90 days (automatic)</p>
					</div>
					<div>
						<p class="text-xs text-[var(--text-tertiary)]">Signing algorithm</p>
						<p class="text-[var(--text-primary)]">ES256 (ECDSA P-256 + SHA-256)</p>
					</div>
					<div>
						<p class="text-xs text-[var(--text-tertiary)]">Agent command</p>
						<code class="text-xs text-[var(--text-primary)]">met id-token --audience &lt;aud&gt;</code>
					</div>
				</div>
				<p class="text-xs text-[var(--text-secondary)]">
					Configure external services (AWS, GCP, Azure, Vault) to trust your Meticulous issuer URL and map
					the <code class="rounded bg-[var(--bg-secondary)] px-1 text-[10px]">sub</code> claim to IAM roles.
					Token claims include org, project, pipeline, ref, and environment.
				</p>
			</div>
		</div>
	</Card>
</div>

<Dialog bind:open={showDeleteDialog} title="Delete API Token">
	<div class="space-y-4">
		<p class="text-sm text-[var(--text-secondary)]">
			Remove this deactivated token from your account. This only deletes metadata; the token already cannot
			authenticate.
		</p>

		{#if tokenToDelete}
			<div class="rounded-lg border border-[var(--border-primary)] bg-[var(--bg-tertiary)] p-3">
				<p class="font-medium text-[var(--text-primary)]">{tokenToDelete.name}</p>
				<code class="mt-1 block text-xs text-[var(--text-tertiary)]">{tokenToDelete.prefix}…</code>
			</div>
		{/if}

		<div class="flex justify-end gap-3">
			<Button
				variant="outline"
				onclick={() => {
					showDeleteDialog = false;
					tokenToDelete = null;
				}}
				disabled={deleting}
			>
				Cancel
			</Button>
			<Button variant="destructive" onclick={deleteToken} disabled={deleting}>
				{deleting ? 'Deleting...' : 'Delete'}
			</Button>
		</div>
	</div>
</Dialog>
