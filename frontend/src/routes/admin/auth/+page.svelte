<script lang="ts">
	import { Key, Plus, Settings, Github, Globe, ToggleLeft, ToggleRight, CheckCircle, XCircle, X, Trash2 } from 'lucide-svelte';

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

	$effect(() => {
		loading = false;
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

		// TODO: Call API to create provider once backend endpoint is wired
		try {
			// Simulate API call - replace with actual API call when ready
			await new Promise(resolve => setTimeout(resolve, 500));
			
			// Add to local state for demo
			const newProvider: AuthProvider = {
				id: crypto.randomUUID(),
				name: providerName,
				type: providerType,
				enabled: false,
				issuerUrl: providerType === 'oidc' ? issuerUrl : undefined,
				clientId: clientId,
				createdAt: new Date().toISOString()
			};
			providers = [...providers, newProvider];
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
		showSettingsModal = true;
	}

	async function toggleProvider() {
		if (!selectedProvider) return;
		
		settingsLoading = true;
		settingsError = null;

		try {
			// TODO: Call API to toggle provider
			await new Promise(resolve => setTimeout(resolve, 300));
			
			providers = providers.map(p => 
				p.id === selectedProvider!.id 
					? { ...p, enabled: !p.enabled }
					: p
			);
			selectedProvider = { ...selectedProvider, enabled: !selectedProvider.enabled };
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
			// TODO: Call API to update provider
			await new Promise(resolve => setTimeout(resolve, 500));
			
			providers = providers.map(p => 
				p.id === selectedProvider!.id 
					? { 
						...p, 
						clientId: editClientId,
						issuerUrl: selectedProvider!.type === 'oidc' ? editIssuerUrl : undefined
					}
					: p
			);
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
			// TODO: Call API to delete provider
			await new Promise(resolve => setTimeout(resolve, 300));
			
			providers = providers.filter(p => p.id !== selectedProvider!.id);
			showSettingsModal = false;
		} catch (e) {
			settingsError = e instanceof Error ? e.message : 'Failed to delete provider';
		} finally {
			settingsLoading = false;
		}
	}
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
							{window.location.origin}/auth/oauth/{selectedProvider.id}/callback
						</code>
					</div>
				</div>
			</div>

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
