<script lang="ts">
	import { Button, Card, Alert, Badge } from '$components/ui';
	import { EmptyState, Skeleton } from '$components/data';
	import { apiMethods, type AdminOrgTokenRow, type OrgPolicyApiResponse } from '$lib/api';
	import { Shield, Key, Save } from 'lucide-svelte';

	let policy = $state<OrgPolicyApiResponse | null>(null);
	let tokens = $state<AdminOrgTokenRow[]>([]);
	let loading = $state(true);
	let saving = $state(false);
	let error = $state<string | null>(null);
	let message = $state<string | null>(null);

	let form = $state({
		max_api_token_ttl_days: 365,
		user_rl_primary_period_secs: 3600,
		user_rl_primary_max: 15000,
		user_rl_secondary_period_secs: 10,
		user_rl_secondary_max: 60,
		app_rl_primary_period_secs: 3600,
		app_rl_primary_max: 15000,
		app_rl_secondary_period_secs: 10,
		app_rl_secondary_max: 60
	});

	$effect(() => {
		void load();
	});

	async function load() {
		loading = true;
		error = null;
		try {
			const [p, t] = await Promise.all([apiMethods.admin.policy.get(), apiMethods.admin.tokens.list()]);
			policy = p;
			tokens = t;
			form = {
				max_api_token_ttl_days: p.max_api_token_ttl_days,
				user_rl_primary_period_secs: p.user_rl_primary_period_secs,
				user_rl_primary_max: p.user_rl_primary_max,
				user_rl_secondary_period_secs: p.user_rl_secondary_period_secs,
				user_rl_secondary_max: p.user_rl_secondary_max,
				app_rl_primary_period_secs: p.app_rl_primary_period_secs,
				app_rl_primary_max: p.app_rl_primary_max,
				app_rl_secondary_period_secs: p.app_rl_secondary_period_secs,
				app_rl_secondary_max: p.app_rl_secondary_max
			};
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load policy';
		} finally {
			loading = false;
		}
	}

	async function save() {
		saving = true;
		message = null;
		error = null;
		try {
			policy = await apiMethods.admin.policy.patch({
				max_api_token_ttl_days: form.max_api_token_ttl_days,
				user_rl_primary_period_secs: form.user_rl_primary_period_secs,
				user_rl_primary_max: form.user_rl_primary_max,
				user_rl_secondary_period_secs: form.user_rl_secondary_period_secs,
				user_rl_secondary_max: form.user_rl_secondary_max,
				app_rl_primary_period_secs: form.app_rl_primary_period_secs,
				app_rl_primary_max: form.app_rl_primary_max,
				app_rl_secondary_period_secs: form.app_rl_secondary_period_secs,
				app_rl_secondary_max: form.app_rl_secondary_max
			});
			message = 'Policy saved.';
		} catch (e) {
			error = e instanceof Error ? e.message : 'Save failed';
		} finally {
			saving = false;
		}
	}

	function tokenBadge(row: AdminOrgTokenRow) {
		const t = row.token;
		if (t.revoked_at) return { label: 'Revoked', variant: 'error' as const };
		if (t.expires_at && new Date(t.expires_at) < new Date()) return { label: 'Expired', variant: 'secondary' as const };
		if (t.deactivated_at) return { label: 'Deactivated', variant: 'secondary' as const };
		return { label: 'Active', variant: 'success' as const };
	}
</script>

<svelte:head>
	<title>Organization policy | Admin</title>
</svelte:head>

<div class="space-y-6">
	<div class="flex items-center gap-3">
		<div class="flex h-10 w-10 items-center justify-center rounded-lg bg-[var(--bg-tertiary)]">
			<Shield class="h-5 w-5 text-[var(--text-secondary)]" />
		</div>
		<div>
			<h1 class="text-xl font-semibold text-[var(--text-primary)]">Organization policy</h1>
			<p class="text-sm text-[var(--text-secondary)]">
				API token TTL cap and per-credential rate limits (session / API token vs Meticulous App JWT).
			</p>
		</div>
	</div>

	{#if error}
		<Alert variant="error" title="Error">{error}</Alert>
	{/if}
	{#if message}
		<Alert variant="success" title="Saved" dismissible ondismiss={() => (message = null)}>{message}</Alert>
	{/if}

	{#if loading}
		<Card>
			<div class="space-y-4 p-4">
				<Skeleton class="h-10 w-full" />
				<Skeleton class="h-10 w-full" />
			</div>
		</Card>
	{:else}
		<Card>
			<form
				class="space-y-4 p-4"
				onsubmit={(e) => {
					e.preventDefault();
					void save();
				}}
			>
				<h2 class="font-medium text-[var(--text-primary)]">Token & rate limits</h2>
				<div class="grid gap-4 sm:grid-cols-2">
					<div>
						<label for="ttl" class="text-sm font-medium text-[var(--text-primary)]"
							>Max API token TTL (days)</label
						>
						<input
							id="ttl"
							type="number"
							class="mt-1 w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm"
							bind:value={form.max_api_token_ttl_days}
							min={1}
						/>
					</div>
				</div>
				<div>
					<h3 class="mb-2 text-sm font-medium text-[var(--text-primary)]">User session / API token</h3>
					<div class="grid gap-4 sm:grid-cols-2">
						<div>
							<label for="urp" class="text-xs text-[var(--text-secondary)]">Primary window (seconds)</label>
							<input
								id="urp"
								type="number"
								class="mt-1 w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm"
								bind:value={form.user_rl_primary_period_secs}
								min={1}
							/>
						</div>
						<div>
							<label for="urpm" class="text-xs text-[var(--text-secondary)]">Primary max requests / window</label>
							<input
								id="urpm"
								type="number"
								class="mt-1 w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm"
								bind:value={form.user_rl_primary_max}
								min={1}
							/>
						</div>
						<div>
							<label for="urs" class="text-xs text-[var(--text-secondary)]">Secondary window (seconds)</label>
							<input
								id="urs"
								type="number"
								class="mt-1 w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm"
								bind:value={form.user_rl_secondary_period_secs}
								min={1}
							/>
						</div>
						<div>
							<label for="ursm" class="text-xs text-[var(--text-secondary)]">Secondary max requests / window</label>
							<input
								id="ursm"
								type="number"
								class="mt-1 w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm"
								bind:value={form.user_rl_secondary_max}
								min={1}
							/>
						</div>
					</div>
				</div>
				<div>
					<h3 class="mb-2 text-sm font-medium text-[var(--text-primary)]">Meticulous App (installation JWT)</h3>
					<div class="grid gap-4 sm:grid-cols-2">
						<div>
							<label for="arp" class="text-xs text-[var(--text-secondary)]">Primary window (seconds)</label>
							<input
								id="arp"
								type="number"
								class="mt-1 w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm"
								bind:value={form.app_rl_primary_period_secs}
								min={1}
							/>
						</div>
						<div>
							<label for="arpm" class="text-xs text-[var(--text-secondary)]">Primary max requests / window</label>
							<input
								id="arpm"
								type="number"
								class="mt-1 w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm"
								bind:value={form.app_rl_primary_max}
								min={1}
							/>
						</div>
						<div>
							<label for="ars" class="text-xs text-[var(--text-secondary)]">Secondary window (seconds)</label>
							<input
								id="ars"
								type="number"
								class="mt-1 w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm"
								bind:value={form.app_rl_secondary_period_secs}
								min={1}
							/>
						</div>
						<div>
							<label for="arsm" class="text-xs text-[var(--text-secondary)]">Secondary max requests / window</label>
							<input
								id="arsm"
								type="number"
								class="mt-1 w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm"
								bind:value={form.app_rl_secondary_max}
								min={1}
							/>
						</div>
					</div>
				</div>
				<div class="flex justify-end">
					<Button variant="primary" type="submit" loading={saving}>
						<Save class="h-4 w-4" />
						Save policy
					</Button>
				</div>
			</form>
		</Card>

		<Card>
			<div class="mb-4 flex items-center gap-2 p-4 pb-0">
				<Key class="h-5 w-5 text-[var(--text-secondary)]" />
				<h2 class="font-medium text-[var(--text-primary)]">API tokens in this organization</h2>
			</div>
			<div class="p-4 pt-2">
				{#if tokens.length === 0}
					<EmptyState title="No tokens" description="No API tokens found for your organization." />
				{:else}
					<div class="overflow-x-auto">
						<table class="w-full text-left text-sm">
							<thead>
								<tr class="border-b border-[var(--border-primary)] text-[var(--text-tertiary)]">
									<th class="py-2 pr-4">Owner</th>
									<th class="py-2 pr-4">Name</th>
									<th class="py-2 pr-4">Prefix</th>
									<th class="py-2 pr-4">State</th>
									<th class="py-2">Scopes</th>
								</tr>
							</thead>
							<tbody>
								{#each tokens as row (row.token.id)}
									{@const b = tokenBadge(row)}
									<tr class="border-b border-[var(--border-secondary)]">
										<td class="py-2 pr-4 text-[var(--text-secondary)]">{row.owner_email}</td>
										<td class="py-2 pr-4 font-medium">{row.token.name}</td>
										<td class="py-2 pr-4 font-mono text-xs">{row.token.prefix}…</td>
										<td class="py-2 pr-4">
											<Badge variant={b.variant} size="sm">{b.label}</Badge>
										</td>
										<td class="py-2 text-xs">{row.token.scopes.join(', ')}</td>
									</tr>
								{/each}
							</tbody>
						</table>
					</div>
				{/if}
			</div>
		</Card>
	{/if}
</div>
