<script lang="ts">
	import { Button, Card, Input, Badge, Dialog, Alert, CopyButton } from '$components/ui';
	import { Skeleton, EmptyState } from '$components/data';
	import { Key, Plus, Trash2, Shield, Eye, EyeOff, Copy } from 'lucide-svelte';

	interface ApiToken {
		id: string;
		name: string;
		prefix: string;
		scopes: string[];
		created_at: string;
		last_used_at?: string;
		expires_at?: string;
	}

	let tokens = $state<ApiToken[]>([]);
	let loading = $state(true);
	let showCreateDialog = $state(false);
	let showNewTokenDialog = $state(false);
	let newTokenValue = $state<string | null>(null);

	let newToken = $state({
		name: '',
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
		try {
			await new Promise((resolve) => setTimeout(resolve, 500));
			tokens = [
				{
					id: '1',
					name: 'CI/CD Token',
					prefix: 'met_live_abc1',
					scopes: ['read', 'write'],
					created_at: new Date(Date.now() - 7 * 24 * 60 * 60 * 1000).toISOString(),
					last_used_at: new Date(Date.now() - 60 * 60 * 1000).toISOString()
				},
				{
					id: '2',
					name: 'Deploy Key',
					prefix: 'met_live_xyz9',
					scopes: ['read'],
					created_at: new Date(Date.now() - 30 * 24 * 60 * 60 * 1000).toISOString(),
					expires_at: new Date(Date.now() + 60 * 24 * 60 * 60 * 1000).toISOString()
				}
			];
		} finally {
			loading = false;
		}
	}

	async function createToken() {
		showCreateDialog = false;
		newTokenValue = 'met_live_' + crypto.randomUUID().replace(/-/g, '').slice(0, 32);
		showNewTokenDialog = true;
		
		const token: ApiToken = {
			id: crypto.randomUUID(),
			name: newToken.name,
			prefix: newTokenValue.slice(0, 13),
			scopes: newToken.scopes,
			created_at: new Date().toISOString()
		};
		tokens = [token, ...tokens];
		
		newToken = { name: '', scopes: ['read'], expiresIn: '90' };
	}

	async function deleteToken(id: string) {
		tokens = tokens.filter((t) => t.id !== id);
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
							onclick={() => deleteToken(token.id)}
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
			<Button variant="outline" onclick={() => (showCreateDialog = false)}>
				Cancel
			</Button>
			<Button variant="primary" type="submit" disabled={!newToken.name || newToken.scopes.length === 0}>
				Create Token
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
