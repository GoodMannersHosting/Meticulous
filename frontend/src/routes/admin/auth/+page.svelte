<script lang="ts">
	import { Key, Plus, Settings, Github, Globe, ToggleLeft, ToggleRight, CheckCircle, XCircle, X, Trash2, Link2 } from 'lucide-svelte';
	import { onMount } from 'svelte';
	import { apiMethods } from '$api';
	import type { AuthProviderResponse, GroupMappingResponse, AdminGroup } from '$api/types';

	interface AuthProvider {
		id: string;
		name: string;
		type: 'oidc' | 'github';
		enabled: boolean;
		issuerUrl?: string;
		clientId?: string;
		createdAt: string;
	}

	let passwordAuthEnabled = $state(true);
	let providers = $state<AuthProvider[]>([]);
	let loading = $state(true);
	let loadError = $state<string | null>(null);

	// Add Provider modal state
	let showAddProviderModal = $state(false);
	let providerType = $state<'oidc' | 'github'>('oidc');
	let providerName = $state('');
	let clientId = $state('');
	let clientSecret = $state('');
	let issuerUrl = $state('');
	let addProviderLoading = $state(false);
	let addProviderError = $state<string | null>(null);

	// Settings modal state
	let showSettingsModal = $state(false);
	let selectedProvider = $state<AuthProvider | null>(null);
	let settingsLoading = $state(false);
	let settingsError = $state<string | null>(null);
	let editClientId = $state('');
	let editClientSecret = $state('');
	let editIssuerUrl = $state('');

	// Group mappings state
	let groupMappings = $state<(GroupMappingResponse & { group_name: string })[]>([]);
	let mappingsLoading = $state(false);
	let allGroups = $state<AdminGroup[]>([]);
	let showAddMappingModal = $state(false);
	let newMappingGroupId = $state<string | null>(null);
	let newMappingOidcGroup = $state('');
	let newMappingRole = $state<'member' | 'maintainer' | 'owner'>('member');
	let addMappingLoading = $state(false);
	let addMappingError = $state<string | null>(null);

	function mapApiProvider(p: AuthProviderResponse): AuthProvider {
		return {
			id: p.id,
			name: p.name,
			type: p.provider_type as 'oidc' | 'github',
			enabled: p.enabled,
			issuerUrl: p.issuer_url,
			clientId: p.client_id,
			createdAt: p.created_at
		};
	}

	async function loadProviders() {
		loading = true;
		loadError = null;
		try {
			const result = await apiMethods.admin.authProviders.list();
			providers = result.map(mapApiProvider);
		} catch (e) {
			loadError = e instanceof Error ? e.message : 'Failed to load providers';
		} finally {
			loading = false;
		}
	}

	onMount(() => {
		loadProviders();
	});

	function getProviderIcon(type: string) {
		switch (type) {
			case 'github':
				return Github;
			case 'oidc':
				return Globe;
			default:
				return Key;
		}
	}

	function openAddProviderModal() {
		providerType = 'oidc';
		providerName = '';
		clientId = '';
		clientSecret = '';
		issuerUrl = '';
		addProviderError = null;
		showAddProviderModal = true;
	}

	async function addProvider() {
		if (!providerName.trim()) {
			addProviderError = 'Provider name is required';
			return;
		}
		if (!clientId.trim()) {
			addProviderError = 'Client ID is required';
			return;
		}
		if (!clientSecret.trim()) {
			addProviderError = 'Client Secret is required';
			return;
		}
		if (providerType === 'oidc' && !issuerUrl.trim()) {
			addProviderError = 'Issuer URL is required for OIDC providers';
			return;
		}

		addProviderLoading = true;
		addProviderError = null;

		try {
			const result = await apiMethods.admin.authProviders.create({
				name: providerName,
				provider_type: providerType,
				client_id: clientId,
				client_secret: clientSecret,
				issuer_url: providerType === 'oidc' ? issuerUrl : undefined
			});
			providers = [...providers, mapApiProvider(result)];
			showAddProviderModal = false;
		} catch (e) {
			addProviderError = e instanceof Error ? e.message : 'Failed to add provider';
		} finally {
			addProviderLoading = false;
		}
	}

	function openSettingsModal(provider: AuthProvider) {
		selectedProvider = provider;
		editClientId = provider.clientId || '';
		editClientSecret = '';
		editIssuerUrl = provider.issuerUrl || '';
		settingsError = null;
		groupMappings = [];
		showSettingsModal = true;
		
		if (provider.type === 'oidc') {
			loadGroupMappings();
		}
	}

	async function toggleProvider() {
		if (!selectedProvider) return;
		
		settingsLoading = true;
		settingsError = null;

		try {
			const result = selectedProvider.enabled
				? await apiMethods.admin.authProviders.disable(selectedProvider.id)
				: await apiMethods.admin.authProviders.enable(selectedProvider.id);
			
			const updated = mapApiProvider(result);
			providers = providers.map(p => p.id === updated.id ? updated : p);
			selectedProvider = updated;
		} catch (e) {
			settingsError = e instanceof Error ? e.message : 'Failed to update provider';
		} finally {
			settingsLoading = false;
		}
	}

	async function saveProviderSettings() {
		if (!selectedProvider) return;

		if (!editClientId.trim()) {
			settingsError = 'Client ID is required';
			return;
		}
		if (selectedProvider.type === 'oidc' && !editIssuerUrl.trim()) {
			settingsError = 'Issuer URL is required for OIDC providers';
			return;
		}

		settingsLoading = true;
		settingsError = null;

		try {
			const result = await apiMethods.admin.authProviders.update(selectedProvider.id, {
				client_id: editClientId,
				client_secret: editClientSecret || undefined,
				issuer_url: selectedProvider.type === 'oidc' ? editIssuerUrl : undefined
			});
			
			const updated = mapApiProvider(result);
			providers = providers.map(p => p.id === updated.id ? updated : p);
			showSettingsModal = false;
		} catch (e) {
			settingsError = e instanceof Error ? e.message : 'Failed to save settings';
		} finally {
			settingsLoading = false;
		}
	}

	async function deleteProvider() {
		if (!selectedProvider) return;
		
		if (!confirm(`Are you sure you want to delete "${selectedProvider.name}"? This action cannot be undone.`)) {
			return;
		}

		settingsLoading = true;
		settingsError = null;

		try {
			await apiMethods.admin.authProviders.delete(selectedProvider.id);
			providers = providers.filter(p => p.id !== selectedProvider!.id);
			showSettingsModal = false;
		} catch (e) {
			settingsError = e instanceof Error ? e.message : 'Failed to delete provider';
		} finally {
			settingsLoading = false;
		}
	}

	async function loadGroupMappings() {
		if (!selectedProvider || selectedProvider.type !== 'oidc') return;
		
		mappingsLoading = true;
		try {
			const [mappings, groups] = await Promise.all([
				apiMethods.admin.authProviders.groupMappings.list(selectedProvider.id),
				apiMethods.admin.groups.list({ limit: 100 })
			]);
			
			allGroups = groups.data;
			
			groupMappings = mappings.map(m => {
				const group = allGroups.find(g => g.id === m.meticulous_group_id);
				return {
					...m,
					group_name: group?.name || 'Unknown Group'
				};
			});
		} catch (e) {
			console.error('Failed to load group mappings:', e);
		} finally {
			mappingsLoading = false;
		}
	}

	function openAddMappingModal() {
		newMappingGroupId = null;
		newMappingOidcGroup = '';
		newMappingRole = 'member';
		addMappingError = null;
		showAddMappingModal = true;
	}

	async function createMapping() {
		if (!selectedProvider || !newMappingGroupId || !newMappingOidcGroup.trim()) {
			addMappingError = 'Please fill in all fields';
			return;
		}

		addMappingLoading = true;
		addMappingError = null;

		try {
			await apiMethods.admin.authProviders.groupMappings.create(selectedProvider.id, {
				oidc_group_claim: newMappingOidcGroup.trim(),
				meticulous_group_id: newMappingGroupId,
				role: newMappingRole
			});
			showAddMappingModal = false;
			await loadGroupMappings();
		} catch (e) {
			addMappingError = e instanceof Error ? e.message : 'Failed to create mapping';
		} finally {
			addMappingLoading = false;
		}
	}

	async function deleteMapping(mappingId: string) {
		if (!selectedProvider) return;
		if (!confirm('Remove this group mapping?')) return;

		try {
			await apiMethods.admin.authProviders.groupMappings.delete(selectedProvider.id, mappingId);
			groupMappings = groupMappings.filter(m => m.id !== mappingId);
		} catch (e) {
			alert(e instanceof Error ? e.message : 'Failed to delete mapping');
		}
	}

	const availableGroups = $derived(
		allGroups.filter(g => !groupMappings.some(m => m.meticulous_group_id === g.id && m.oidc_group_claim === newMappingOidcGroup))
	);
</script>

<div class="space-y-8">
	<div>
		<h2 class="text-lg font-semibold text-[var(--text-primary)]">Authentication Settings</h2>
		<p class="text-sm text-[var(--text-secondary)]">Configure how users can sign in to the platform</p>
	</div>

	<div class="rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)]">
		<div class="flex items-center justify-between border-b border-[var(--border-primary)] p-4">
			<div class="flex items-center gap-3">
				<div class="flex h-10 w-10 items-center justify-center rounded-lg bg-[var(--bg-primary)]">
					<Key class="h-5 w-5 text-[var(--text-secondary)]" />
				</div>
				<div>
					<h3 class="font-medium text-[var(--text-primary)]">Password Authentication</h3>
					<p class="text-sm text-[var(--text-secondary)]">Allow users to sign in with username and password</p>
				</div>
			</div>
			<button
				type="button"
				onclick={() => (passwordAuthEnabled = !passwordAuthEnabled)}
				class="text-[var(--text-secondary)] hover:text-[var(--text-primary)]"
			>
				{#if passwordAuthEnabled}
					<ToggleRight class="h-8 w-8 text-primary-500" />
				{:else}
					<ToggleLeft class="h-8 w-8" />
				{/if}
			</button>
		</div>
		<div class="p-4">
			<div class="flex items-center gap-2 text-sm">
				{#if passwordAuthEnabled}
					<CheckCircle class="h-4 w-4 text-green-500" />
					<span class="text-[var(--text-secondary)]">Password login is enabled</span>
				{:else}
					<XCircle class="h-4 w-4 text-gray-400" />
					<span class="text-[var(--text-secondary)]">Password login is disabled</span>
				{/if}
			</div>
		</div>
	</div>

	<div>
		<div class="flex items-center justify-between">
			<div>
				<h3 class="text-base font-medium text-[var(--text-primary)]">Identity Providers</h3>
				<p class="text-sm text-[var(--text-secondary)]">Configure external authentication providers</p>
			</div>
		<button
			type="button"
			onclick={openAddProviderModal}
			class="inline-flex items-center gap-2 rounded-lg bg-primary-600 px-4 py-2 text-sm font-medium text-white hover:bg-primary-700"
		>
			<Plus class="h-4 w-4" />
			Add Provider
		</button>
		</div>

		{#if loading}
			<div class="mt-4 flex items-center justify-center py-12">
				<div class="h-8 w-8 animate-spin rounded-full border-2 border-primary-500 border-t-transparent"></div>
			</div>
		{:else if providers.length === 0}
			<div class="mt-4 flex flex-col items-center justify-center rounded-lg border border-dashed border-[var(--border-primary)] py-12">
				<Globe class="h-12 w-12 text-[var(--text-tertiary)]" />
				<h3 class="mt-4 text-sm font-medium text-[var(--text-primary)]">No providers configured</h3>
				<p class="mt-1 text-sm text-[var(--text-secondary)]">
					Add an OIDC or GitHub provider to enable SSO
				</p>
			</div>
		{:else}
			<div class="mt-4 space-y-3">
				{#each providers as provider (provider.id)}
					{@const Icon = getProviderIcon(provider.type)}
					<div class="rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] p-4">
						<div class="flex items-center justify-between">
							<div class="flex items-center gap-3">
								<div class="flex h-10 w-10 items-center justify-center rounded-lg bg-[var(--bg-primary)]">
									<Icon class="h-5 w-5 text-[var(--text-secondary)]" />
								</div>
								<div>
									<div class="flex items-center gap-2">
										<h4 class="font-medium text-[var(--text-primary)]">{provider.name}</h4>
										<span class="rounded-full bg-gray-100 px-2 py-0.5 text-xs font-medium uppercase text-gray-600 dark:bg-gray-800 dark:text-gray-400">
											{provider.type}
										</span>
									</div>
									{#if provider.issuerUrl}
										<p class="text-sm text-[var(--text-secondary)]">{provider.issuerUrl}</p>
									{/if}
								</div>
							</div>
							<div class="flex items-center gap-3">
								{#if provider.enabled}
									<span class="inline-flex items-center gap-1.5 text-sm text-green-600 dark:text-green-400">
										<span class="h-1.5 w-1.5 rounded-full bg-green-500"></span>
										Enabled
									</span>
								{:else}
									<span class="inline-flex items-center gap-1.5 text-sm text-gray-500">
										<span class="h-1.5 w-1.5 rounded-full bg-gray-400"></span>
										Disabled
									</span>
								{/if}
								<button
									type="button"
									onclick={() => openSettingsModal(provider)}
									class="rounded p-1 text-[var(--text-secondary)] hover:bg-[var(--bg-primary)] hover:text-[var(--text-primary)]"
									title="Provider settings"
								>
									<Settings class="h-4 w-4" />
								</button>
							</div>
						</div>
					</div>
				{/each}
			</div>
		{/if}
	</div>
</div>

<!-- Add Provider Modal -->
{#if showAddProviderModal}
	<div class="fixed inset-0 z-50 flex items-center justify-center bg-black/50" onclick={() => showAddProviderModal = false}>
		<div class="w-full max-w-lg rounded-lg bg-[var(--bg-primary)] p-6 shadow-xl" onclick={(e) => e.stopPropagation()}>
			<div class="flex items-center justify-between">
				<h3 class="text-lg font-semibold text-[var(--text-primary)]">Add Identity Provider</h3>
				<button
					type="button"
					onclick={() => showAddProviderModal = false}
					class="rounded p-1 text-[var(--text-secondary)] hover:bg-[var(--bg-secondary)]"
				>
					<X class="h-5 w-5" />
				</button>
			</div>
			
			{#if addProviderError}
				<div class="mt-4 rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-700 dark:border-red-900 dark:bg-red-950/50 dark:text-red-400">
					{addProviderError}
				</div>
			{/if}

			<div class="mt-4 space-y-4">
				<div>
					<label class="block text-sm font-medium text-[var(--text-primary)]">Provider Type</label>
					<div class="mt-2 flex gap-4">
						<label class="flex items-center gap-2 cursor-pointer">
							<input
								type="radio"
								name="providerType"
								value="oidc"
								checked={providerType === 'oidc'}
								onchange={() => providerType = 'oidc'}
								class="h-4 w-4 text-primary-600 focus:ring-primary-500"
							/>
							<Globe class="h-4 w-4 text-[var(--text-secondary)]" />
							<span class="text-sm text-[var(--text-primary)]">OIDC</span>
						</label>
						<label class="flex items-center gap-2 cursor-pointer">
							<input
								type="radio"
								name="providerType"
								value="github"
								checked={providerType === 'github'}
								onchange={() => providerType = 'github'}
								class="h-4 w-4 text-primary-600 focus:ring-primary-500"
							/>
							<Github class="h-4 w-4 text-[var(--text-secondary)]" />
							<span class="text-sm text-[var(--text-primary)]">GitHub</span>
						</label>
					</div>
				</div>

				<div>
					<label for="provider-name" class="block text-sm font-medium text-[var(--text-primary)]">Provider Name</label>
					<input
						type="text"
						id="provider-name"
						bind:value={providerName}
						placeholder={providerType === 'github' ? 'e.g., GitHub Enterprise' : 'e.g., Okta, Auth0'}
						class="mt-1 w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm text-[var(--text-primary)] focus:border-primary-500 focus:outline-none focus:ring-1 focus:ring-primary-500"
					/>
				</div>

				<div>
					<label for="client-id" class="block text-sm font-medium text-[var(--text-primary)]">Client ID</label>
					<input
						type="text"
						id="client-id"
						bind:value={clientId}
						placeholder="OAuth client ID"
						class="mt-1 w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm text-[var(--text-primary)] focus:border-primary-500 focus:outline-none focus:ring-1 focus:ring-primary-500"
					/>
				</div>

				<div>
					<label for="client-secret" class="block text-sm font-medium text-[var(--text-primary)]">Client Secret</label>
					<input
						type="password"
						id="client-secret"
						bind:value={clientSecret}
						placeholder="OAuth client secret"
						class="mt-1 w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm text-[var(--text-primary)] focus:border-primary-500 focus:outline-none focus:ring-1 focus:ring-primary-500"
					/>
				</div>

				{#if providerType === 'oidc'}
					<div>
						<label for="issuer-url" class="block text-sm font-medium text-[var(--text-primary)]">Issuer URL</label>
						<input
							type="url"
							id="issuer-url"
							bind:value={issuerUrl}
							placeholder="https://your-idp.example.com"
							class="mt-1 w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm text-[var(--text-primary)] focus:border-primary-500 focus:outline-none focus:ring-1 focus:ring-primary-500"
						/>
						<p class="mt-1 text-xs text-[var(--text-tertiary)]">The OIDC discovery endpoint will be derived from this URL</p>
					</div>
				{/if}
			</div>

			<div class="mt-6 flex justify-end gap-3">
				<button
					type="button"
					onclick={() => showAddProviderModal = false}
					class="rounded-lg border border-[var(--border-primary)] px-4 py-2 text-sm font-medium text-[var(--text-primary)] hover:bg-[var(--bg-secondary)]"
				>
					Cancel
				</button>
				<button
					type="button"
					onclick={addProvider}
					disabled={addProviderLoading}
					class="rounded-lg bg-primary-600 px-4 py-2 text-sm font-medium text-white hover:bg-primary-700 disabled:opacity-50"
				>
					{addProviderLoading ? 'Adding...' : 'Add Provider'}
				</button>
			</div>
		</div>
	</div>
{/if}

<!-- Provider Settings Modal -->
{#if showSettingsModal && selectedProvider}
	<div class="fixed inset-0 z-50 flex items-center justify-center bg-black/50" onclick={() => showSettingsModal = false}>
		<div class="w-full max-w-lg rounded-lg bg-[var(--bg-primary)] p-6 shadow-xl" onclick={(e) => e.stopPropagation()}>
			<div class="flex items-center justify-between">
				<div class="flex items-center gap-3">
					{#if selectedProvider.type === 'github'}
						<Github class="h-5 w-5 text-[var(--text-secondary)]" />
					{:else}
						<Globe class="h-5 w-5 text-[var(--text-secondary)]" />
					{/if}
					<h3 class="text-lg font-semibold text-[var(--text-primary)]">{selectedProvider.name}</h3>
					<span class="rounded-full bg-gray-100 px-2 py-0.5 text-xs font-medium uppercase text-gray-600 dark:bg-gray-800 dark:text-gray-400">
						{selectedProvider.type}
					</span>
				</div>
				<button
					type="button"
					onclick={() => showSettingsModal = false}
					class="rounded p-1 text-[var(--text-secondary)] hover:bg-[var(--bg-secondary)]"
				>
					<X class="h-5 w-5" />
				</button>
			</div>

			{#if settingsError}
				<div class="mt-4 rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-700 dark:border-red-900 dark:bg-red-950/50 dark:text-red-400">
					{settingsError}
				</div>
			{/if}

			<!-- Enable/Disable Toggle -->
			<div class="mt-6 flex items-center justify-between rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] p-4">
				<div>
					<h4 class="font-medium text-[var(--text-primary)]">Provider Status</h4>
					<p class="text-sm text-[var(--text-secondary)]">
						{selectedProvider.enabled ? 'Users can sign in with this provider' : 'This provider is disabled'}
					</p>
				</div>
				<button
					type="button"
					onclick={toggleProvider}
					disabled={settingsLoading}
					class="text-[var(--text-secondary)] hover:text-[var(--text-primary)] disabled:opacity-50"
				>
					{#if selectedProvider.enabled}
						<ToggleRight class="h-8 w-8 text-primary-500" />
					{:else}
						<ToggleLeft class="h-8 w-8" />
					{/if}
				</button>
			</div>

			<!-- OAuth Settings -->
			<div class="mt-6 space-y-4">
				<h4 class="font-medium text-[var(--text-primary)]">OAuth Configuration</h4>
				
				<div>
					<label for="edit-client-id" class="block text-sm font-medium text-[var(--text-primary)]">Client ID</label>
					<input
						type="text"
						id="edit-client-id"
						bind:value={editClientId}
						class="mt-1 w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm text-[var(--text-primary)] focus:border-primary-500 focus:outline-none focus:ring-1 focus:ring-primary-500"
					/>
				</div>

				<div>
					<label for="edit-client-secret" class="block text-sm font-medium text-[var(--text-primary)]">Client Secret</label>
					<input
						type="password"
						id="edit-client-secret"
						bind:value={editClientSecret}
						placeholder="Leave blank to keep current secret"
						class="mt-1 w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm text-[var(--text-primary)] focus:border-primary-500 focus:outline-none focus:ring-1 focus:ring-primary-500"
					/>
				</div>

				{#if selectedProvider.type === 'oidc'}
					<div>
						<label for="edit-issuer-url" class="block text-sm font-medium text-[var(--text-primary)]">Issuer URL</label>
						<input
							type="url"
							id="edit-issuer-url"
							bind:value={editIssuerUrl}
							class="mt-1 w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm text-[var(--text-primary)] focus:border-primary-500 focus:outline-none focus:ring-1 focus:ring-primary-500"
						/>
					</div>
				{/if}
			</div>

			<!-- Callback URLs Info -->
			<div class="mt-6 rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] p-4">
				<h4 class="font-medium text-[var(--text-primary)]">Callback URLs</h4>
				<p class="mt-1 text-xs text-[var(--text-tertiary)]">Configure these URLs in your identity provider</p>
				<div class="mt-3 space-y-2">
					<div>
						<span class="text-xs font-medium text-[var(--text-secondary)]">Redirect URI:</span>
						<code class="ml-2 rounded bg-[var(--bg-primary)] px-2 py-0.5 text-xs text-[var(--text-primary)]">
							{window.location.origin}/auth/oauth/callback
						</code>
					</div>
				</div>
			</div>

			<!-- OIDC Group Mappings -->
			{#if selectedProvider.type === 'oidc'}
				<div class="mt-6">
					<div class="flex items-center justify-between">
						<div>
							<h4 class="font-medium text-[var(--text-primary)]">Group Mappings</h4>
							<p class="text-xs text-[var(--text-tertiary)]">Map OIDC groups to Meticulous groups for auto-assignment</p>
						</div>
						<button
							type="button"
							onclick={openAddMappingModal}
							class="inline-flex items-center gap-1.5 rounded-lg bg-primary-600 px-2.5 py-1 text-xs font-medium text-white hover:bg-primary-700"
						>
							<Plus class="h-3 w-3" />
							Add
						</button>
					</div>
					
					<div class="mt-3 max-h-48 overflow-auto rounded-lg border border-[var(--border-primary)]">
						{#if mappingsLoading}
							<div class="flex items-center justify-center py-6">
								<div class="h-5 w-5 animate-spin rounded-full border-2 border-primary-500 border-t-transparent"></div>
							</div>
						{:else if groupMappings.length === 0}
							<div class="py-6 text-center">
								<Link2 class="mx-auto h-6 w-6 text-[var(--text-tertiary)]" />
								<p class="mt-2 text-xs text-[var(--text-secondary)]">No group mappings configured</p>
							</div>
						{:else}
							<table class="min-w-full divide-y divide-[var(--border-primary)]">
								<thead class="bg-[var(--bg-secondary)]">
									<tr>
										<th class="px-3 py-2 text-left text-xs font-medium uppercase text-[var(--text-secondary)]">OIDC Group</th>
										<th class="px-3 py-2 text-left text-xs font-medium uppercase text-[var(--text-secondary)]">Meticulous Group</th>
										<th class="px-3 py-2 text-left text-xs font-medium uppercase text-[var(--text-secondary)]">Role</th>
										<th class="px-3 py-2 w-10"></th>
									</tr>
								</thead>
								<tbody class="divide-y divide-[var(--border-primary)] bg-[var(--bg-primary)]">
									{#each groupMappings as mapping (mapping.id)}
										<tr>
											<td class="px-3 py-2">
												<code class="rounded bg-[var(--bg-secondary)] px-1.5 py-0.5 text-xs text-[var(--text-primary)]">
													{mapping.oidc_group_claim}
												</code>
											</td>
											<td class="px-3 py-2 text-xs text-[var(--text-primary)]">{mapping.group_name}</td>
											<td class="px-3 py-2 text-xs capitalize text-[var(--text-secondary)]">{mapping.role}</td>
											<td class="px-3 py-2">
												<button
													type="button"
													onclick={() => deleteMapping(mapping.id)}
													class="rounded p-1 text-red-600 hover:bg-red-50 dark:text-red-400 dark:hover:bg-red-900/20"
												>
													<Trash2 class="h-3 w-3" />
												</button>
											</td>
										</tr>
									{/each}
								</tbody>
							</table>
						{/if}
					</div>
				</div>
			{/if}

			<div class="mt-6 flex items-center justify-between">
				<button
					type="button"
					onclick={deleteProvider}
					disabled={settingsLoading}
					class="inline-flex items-center gap-2 rounded-lg border border-red-300 px-3 py-2 text-sm font-medium text-red-600 hover:bg-red-50 disabled:opacity-50 dark:border-red-800 dark:text-red-400 dark:hover:bg-red-950/50"
				>
					<Trash2 class="h-4 w-4" />
					Delete Provider
				</button>
				<div class="flex gap-3">
					<button
						type="button"
						onclick={() => showSettingsModal = false}
						class="rounded-lg border border-[var(--border-primary)] px-4 py-2 text-sm font-medium text-[var(--text-primary)] hover:bg-[var(--bg-secondary)]"
					>
						Cancel
					</button>
					<button
						type="button"
						onclick={saveProviderSettings}
						disabled={settingsLoading}
						class="rounded-lg bg-primary-600 px-4 py-2 text-sm font-medium text-white hover:bg-primary-700 disabled:opacity-50"
					>
						{settingsLoading ? 'Saving...' : 'Save Changes'}
					</button>
				</div>
			</div>
		</div>
	</div>
{/if}

<!-- Add Group Mapping Modal -->
{#if showAddMappingModal && selectedProvider}
	<div class="fixed inset-0 z-[60] flex items-center justify-center bg-black/50" onclick={() => showAddMappingModal = false}>
		<div class="w-full max-w-md rounded-lg bg-[var(--bg-primary)] p-6 shadow-xl" onclick={(e) => e.stopPropagation()}>
			<div class="flex items-center justify-between">
				<h3 class="text-lg font-semibold text-[var(--text-primary)]">Add Group Mapping</h3>
				<button
					type="button"
					onclick={() => showAddMappingModal = false}
					class="rounded p-1 text-[var(--text-secondary)] hover:bg-[var(--bg-secondary)]"
				>
					<X class="h-5 w-5" />
				</button>
			</div>
			<p class="mt-2 text-sm text-[var(--text-secondary)]">
				When users log in via <strong>{selectedProvider.name}</strong> with this OIDC group, they'll be automatically added to the selected Meticulous group.
			</p>
			{#if addMappingError}
				<div class="mt-4 rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-700 dark:border-red-900 dark:bg-red-950/50 dark:text-red-400">
					{addMappingError}
				</div>
			{/if}
			<div class="mt-4 space-y-4">
				<div>
					<label for="mapping-oidc-group" class="block text-sm font-medium text-[var(--text-primary)]">OIDC Group Name</label>
					<input
						type="text"
						id="mapping-oidc-group"
						bind:value={newMappingOidcGroup}
						placeholder="e.g., /developers or admin-team"
						class="mt-1 w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm text-[var(--text-primary)] focus:border-primary-500 focus:outline-none focus:ring-1 focus:ring-primary-500"
					/>
					<p class="mt-1 text-xs text-[var(--text-tertiary)]">The exact group name from your OIDC provider's groups claim</p>
				</div>
				<div>
					<label for="mapping-group" class="block text-sm font-medium text-[var(--text-primary)]">Meticulous Group</label>
					<select
						id="mapping-group"
						bind:value={newMappingGroupId}
						class="mt-1 w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm text-[var(--text-primary)] focus:border-primary-500 focus:outline-none focus:ring-1 focus:ring-primary-500"
					>
						<option value={null}>Select a group...</option>
						{#each allGroups as group (group.id)}
							<option value={group.id}>{group.name}</option>
						{/each}
					</select>
				</div>
				<div>
					<label for="mapping-role" class="block text-sm font-medium text-[var(--text-primary)]">Role</label>
					<select
						id="mapping-role"
						bind:value={newMappingRole}
						class="mt-1 w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm text-[var(--text-primary)] focus:border-primary-500 focus:outline-none focus:ring-1 focus:ring-primary-500"
					>
						<option value="member">Member</option>
						<option value="maintainer">Maintainer</option>
						<option value="owner">Owner</option>
					</select>
					<p class="mt-1 text-xs text-[var(--text-tertiary)]">The role users will be assigned in the Meticulous group</p>
				</div>
			</div>
			<div class="mt-6 flex justify-end gap-3">
				<button
					type="button"
					onclick={() => showAddMappingModal = false}
					class="rounded-lg border border-[var(--border-primary)] px-4 py-2 text-sm font-medium text-[var(--text-primary)] hover:bg-[var(--bg-secondary)]"
				>
					Cancel
				</button>
				<button
					type="button"
					onclick={createMapping}
					disabled={addMappingLoading || !newMappingGroupId || !newMappingOidcGroup.trim()}
					class="rounded-lg bg-primary-600 px-4 py-2 text-sm font-medium text-white hover:bg-primary-700 disabled:opacity-50"
				>
					{addMappingLoading ? 'Creating...' : 'Create Mapping'}
				</button>
			</div>
		</div>
	</div>
{/if}
