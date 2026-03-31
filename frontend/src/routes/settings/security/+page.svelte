<script lang="ts">
	import { Button, Card, Input, Badge, Dialog, Alert, CopyButton } from '$components/ui';
	import { Skeleton, EmptyState } from '$components/data';
	import { Key, Plus, Trash2, Shield, Eye, EyeOff, Copy } from 'lucide-svelte';
	import { api } from '$lib/api';

	interface ApiToken {
		id: string;
		name: string;
		description?: string;
		prefix: string;
		scopes: string[];
		created_at: string;
		last_used_at?: string;
		expires_at?: string;
	}

	let tokens = $state<ApiToken[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let showCreateDialog = $state(false);
	let showNewTokenDialog = $state(false);
	let showDeleteDialog = $state(false);
	let tokenToDelete = $state<ApiToken | null>(null);
	let deleting = $state(false);
	let newTokenValue = $state<string | null>(null);
	let creating = $state(false);

	let newToken = $state({
		name: '',
		description: '',
		scopes: ['read'] as string[],
		expiresIn: '90'
	});

	const scopeOptions = [
		{ value: 'read', label: 'Read', description: 'Read access to resources' },
		{ value: 'write', label: 'Write', description: 'Write access to resources' },
		{ value: 'admin', label: 'Admin', description: 'Full administrative access' }
	];

	const expirationOptions = [
		{ value: '30', label: '30 days' },
		{ value: '90', label: '90 days' },
		{ value: '365', label: '1 year' },
		{ value: 'never', label: 'Never' }
	];

	$effect(() => {
		loadTokens();
	});

	async function loadTokens() {
		loading = true;
		error = null;
		try {
			const response = await api.get<{ items: ApiToken[] }>('/api/v1/tokens');
			tokens = response.items || [];
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load tokens';
			console.error('Failed to load tokens:', e);
		} finally {
			loading = false;
		}
	}

	async function createToken() {
		if (!newToken.name.trim()) return;
		
		creating = true;
		error = null;
		try {
			const expiresInDays = newToken.expiresIn === 'never' ? null : parseInt(newToken.expiresIn);
			const description = newToken.description.trim() || undefined;
			
			const response = await api.post<{ token: ApiToken; plain_token: string }>('/api/v1/tokens', {
				name: newToken.name.trim(),
				description,
				scopes: newToken.scopes,
				expires_in_days: expiresInDays
			});
			
			showCreateDialog = false;
			newTokenValue = response.plain_token;
			showNewTokenDialog = true;
			tokens = [response.token, ...tokens];
			newToken = { name: '', description: '', scopes: ['read'], expiresIn: '90' };
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to create token';
			console.error('Failed to create token:', e);
		} finally {
			creating = false;
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

	function toggleScope(scope: string) {
		if (newToken.scopes.includes(scope)) {
			newToken.scopes = newToken.scopes.filter((s) => s !== scope);
		} else {
			newToken.scopes = [...newToken.scopes, scope];
		}
	}
</script>

<svelte:head>
	<title>Security | Meticulous</title>
</svelte:head>

<div class="space-y-6">
	<div class="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
		<div>
			<h1 class="text-2xl font-bold text-[var(--text-primary)]">Security</h1>
			<p class="mt-1 text-[var(--text-secondary)]">
				Manage API tokens and security settings.
			</p>
		</div>

		<Button variant="primary" onclick={() => (showCreateDialog = true)}>
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
					Tokens for authenticating with the API
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
				<Button variant="primary" onclick={() => (showCreateDialog = true)}>
					<Plus class="h-4 w-4" />
					Create Token
				</Button>
			</EmptyState>
		{:else}
			<div class="space-y-3">
				{#each tokens as token (token.id)}
					<div class="flex items-center gap-4 rounded-lg border border-[var(--border-primary)] p-4">
						<div class="flex-1">
							<div class="flex items-center gap-2">
								<span class="font-medium text-[var(--text-primary)]">{token.name}</span>
								<code class="text-xs text-[var(--text-tertiary)]">{token.prefix}...</code>
							</div>
							{#if token.description}
								<p class="mt-0.5 text-sm text-[var(--text-secondary)]">{token.description}</p>
							{/if}
							<div class="mt-1 flex flex-wrap gap-1">
								{#each token.scopes as scope (scope)}
									<Badge variant="outline" size="sm">{scope}</Badge>
								{/each}
							</div>
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
						<Button
							variant="ghost"
							size="sm"
							onclick={() => confirmDeleteToken(token)}
						>
							<Trash2 class="h-4 w-4 text-error-500" />
						</Button>
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
</div>

<Dialog bind:open={showCreateDialog} title="Create API Token">
	<form onsubmit={(e) => { e.preventDefault(); createToken(); }} class="space-y-4">
		<div>
			<label for="token-name" class="block text-sm font-medium text-[var(--text-primary)]">
				Token Name
			</label>
			<Input
				id="token-name"
				placeholder="e.g., CI/CD Token"
				bind:value={newToken.name}
				class="mt-1"
				required
			/>
		</div>

		<div>
			<label for="token-description" class="block text-sm font-medium text-[var(--text-primary)]">
				Description
				<span class="font-normal text-[var(--text-tertiary)]">(optional)</span>
			</label>
			<Input
				id="token-description"
				placeholder="e.g., Used by GitHub Actions to deploy staging"
				bind:value={newToken.description}
				class="mt-1"
			/>
		</div>

		<div>
			<span class="block text-sm font-medium text-[var(--text-primary)]">
				Scopes
			</span>
			<div class="mt-2 space-y-2">
				{#each scopeOptions as option (option.value)}
					<label class="flex items-center gap-3 rounded-lg border border-[var(--border-primary)] p-3">
						<input
							type="checkbox"
							checked={newToken.scopes.includes(option.value)}
							onchange={() => toggleScope(option.value)}
							class="h-4 w-4 rounded border-secondary-300"
						/>
						<div>
							<p class="font-medium text-[var(--text-primary)]">{option.label}</p>
							<p class="text-sm text-[var(--text-secondary)]">{option.description}</p>
						</div>
					</label>
				{/each}
			</div>
		</div>

		<div>
			<label for="expiration" class="block text-sm font-medium text-[var(--text-primary)]">
				Expiration
			</label>
			<select
				id="expiration"
				bind:value={newToken.expiresIn}
				class="
					mt-1 w-full rounded-lg border border-[var(--border-primary)]
					bg-[var(--bg-secondary)] px-3 py-2 text-sm
					focus:outline-none focus:ring-2 focus:ring-primary-500
				"
			>
				{#each expirationOptions as option (option.value)}
					<option value={option.value}>{option.label}</option>
				{/each}
			</select>
		</div>

		<div class="flex justify-end gap-3 pt-4">
			<Button variant="outline" onclick={() => (showCreateDialog = false)} disabled={creating}>
				Cancel
			</Button>
			<Button variant="primary" type="submit" disabled={!newToken.name || newToken.scopes.length === 0 || creating}>
				{creating ? 'Creating...' : 'Create Token'}
			</Button>
		</div>
	</form>
</Dialog>

<Dialog bind:open={showNewTokenDialog} title="Token Created">
	<div class="space-y-4">
		<Alert variant="warning" title="Copy your token now">
			This is the only time you'll see this token. Make sure to save it somewhere safe.
		</Alert>

		{#if newTokenValue}
			<div class="flex items-center gap-2 rounded-lg bg-[var(--bg-tertiary)] p-3">
				<code class="flex-1 break-all font-mono text-sm">{newTokenValue}</code>
				<CopyButton text={newTokenValue} />
			</div>
		{/if}

		<div class="flex justify-end">
			<Button variant="primary" onclick={() => { showNewTokenDialog = false; newTokenValue = null; }}>
				Done
			</Button>
		</div>
	</div>
</Dialog>

<Dialog bind:open={showDeleteDialog} title="Delete API Token">
	<div class="space-y-4">
		<p class="text-sm text-[var(--text-secondary)]">
			Are you sure you want to delete this token? Any applications or scripts using this token
			will no longer be able to authenticate.
		</p>

		{#if tokenToDelete}
			<div class="rounded-lg border border-[var(--border-primary)] bg-[var(--bg-tertiary)] p-3">
				<p class="font-medium text-[var(--text-primary)]">{tokenToDelete.name}</p>
				{#if tokenToDelete.description}
					<p class="mt-0.5 text-sm text-[var(--text-secondary)]">{tokenToDelete.description}</p>
				{/if}
				<code class="mt-1 block text-xs text-[var(--text-tertiary)]">{tokenToDelete.prefix}...</code>
			</div>
		{/if}

		<Alert variant="error" title="This action cannot be undone">
			The token will be permanently revoked and cannot be recovered.
		</Alert>

		<div class="flex justify-end gap-3">
			<Button variant="outline" onclick={() => { showDeleteDialog = false; tokenToDelete = null; }} disabled={deleting}>
				Cancel
			</Button>
			<Button variant="destructive" onclick={deleteToken} disabled={deleting}>
				{deleting ? 'Deleting...' : 'Delete Token'}
			</Button>
		</div>
	</div>
</Dialog>
