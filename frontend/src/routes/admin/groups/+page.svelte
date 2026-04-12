<script lang="ts">
	import { goto } from '$app/navigation';
	import { Users, Plus, Search, MoreVertical, UserCog, Trash2, Edit, X, Link2 } from 'lucide-svelte';
	import { apiMethods } from '$lib/api';
	import type { AdminGroup, AuthProviderResponse, GroupMappingResponse } from '$lib/api/types';
	import Pagination from '$lib/components/data/Pagination.svelte';

	let searchQuery = $state('');
	let listPage = $state(1);
	let listPerPage = $state(20);
	let groups = $state<AdminGroup[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let actionMenuOpen = $state<string | null>(null);

	// Create group modal
	let showCreateModal = $state(false);
	let createName = $state('');
	let createDescription = $state('');
	let createLoading = $state(false);
	let createError = $state<string | null>(null);

	// Edit group modal
	let showEditModal = $state(false);
	let editGroupId = $state<string | null>(null);
	let editName = $state('');
	let editDescription = $state('');
	let editLoading = $state(false);
	let editError = $state<string | null>(null);

	// OIDC Mappings modal
	let showOidcMappingsModal = $state(false);
	let oidcMappingsGroupId = $state<string | null>(null);
	let oidcMappingsGroupName = $state('');
	let oidcMappings = $state<(GroupMappingResponse & { provider_name: string })[]>([]);
	let oidcMappingsLoading = $state(false);
	let oidcMappingsError = $state<string | null>(null);

	// Add OIDC mapping
	let showAddOidcMappingModal = $state(false);
	let authProviders = $state<AuthProviderResponse[]>([]);
	let newMappingProviderId = $state<string | null>(null);
	let newMappingOidcGroup = $state('');
	let newMappingRole = $state<'member' | 'maintainer' | 'owner'>('member');
	let addOidcMappingLoading = $state(false);
	let addOidcMappingError = $state<string | null>(null);

	async function loadGroups() {
		loading = true;
		error = null;
		try {
			const response = await apiMethods.admin.groups.list({ limit: 100 });
			groups = response.data;
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load groups';
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		loadGroups();
	});

	const filteredGroups = $derived(
		groups.filter(
			(g) =>
				g.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
				(g.description?.toLowerCase().includes(searchQuery.toLowerCase()) ?? false)
		)
	);

	const pagedGroups = $derived(
		filteredGroups.slice((listPage - 1) * listPerPage, listPage * listPerPage)
	);

	$effect(() => {
		searchQuery;
		listPage = 1;
	});

	$effect(() => {
		const n = filteredGroups.length;
		const maxPage = Math.max(1, Math.ceil(n / listPerPage) || 1);
		if (listPage > maxPage) listPage = maxPage;
	});

	function toggleActionMenu(groupId: string) {
		actionMenuOpen = actionMenuOpen === groupId ? null : groupId;
	}

	function closeActionMenu() {
		actionMenuOpen = null;
	}

	function openCreateModal() {
		createName = '';
		createDescription = '';
		createError = null;
		showCreateModal = true;
	}

	async function createGroup() {
		if (!createName.trim()) {
			createError = 'Name is required';
			return;
		}

		createLoading = true;
		createError = null;
		try {
			await apiMethods.admin.groups.create({
				name: createName.trim(),
				description: createDescription.trim() || undefined
			});
			showCreateModal = false;
			await loadGroups();
		} catch (e) {
			createError = e instanceof Error ? e.message : 'Failed to create group';
		} finally {
			createLoading = false;
		}
	}

	function openEditModal(group: AdminGroup) {
		closeActionMenu();
		editGroupId = group.id;
		editName = group.name;
		editDescription = group.description ?? '';
		editError = null;
		showEditModal = true;
	}

	async function updateGroup() {
		if (!editGroupId || !editName.trim()) {
			editError = 'Name is required';
			return;
		}

		editLoading = true;
		editError = null;
		try {
			await apiMethods.admin.groups.update(editGroupId, {
				name: editName.trim(),
				description: editDescription.trim() || undefined
			});
			showEditModal = false;
			await loadGroups();
		} catch (e) {
			editError = e instanceof Error ? e.message : 'Failed to update group';
		} finally {
			editLoading = false;
		}
	}

	async function deleteGroup(groupId: string, groupName: string) {
		closeActionMenu();
		if (!confirm(`Are you sure you want to delete the group "${groupName}"? This action cannot be undone.`)) {
			return;
		}
		try {
			await apiMethods.admin.groups.delete(groupId);
			await loadGroups();
		} catch (e) {
			alert(e instanceof Error ? e.message : 'Failed to delete group');
		}
	}

	async function openOidcMappingsModal(group: AdminGroup) {
		closeActionMenu();
		oidcMappingsGroupId = group.id;
		oidcMappingsGroupName = group.name;
		oidcMappings = [];
		oidcMappingsError = null;
		showOidcMappingsModal = true;
		oidcMappingsLoading = true;

		try {
			const providers = await apiMethods.admin.authProviders.list();
			const allMappings: (GroupMappingResponse & { provider_name: string })[] = [];
			
			for (const provider of providers) {
				if (provider.provider_type === 'oidc') {
					try {
						const mappings = await apiMethods.admin.authProviders.groupMappings.list(provider.id);
						for (const mapping of mappings) {
							if (mapping.meticulous_group_id === group.id) {
								allMappings.push({ ...mapping, provider_name: provider.name });
							}
						}
					} catch {
						// Skip providers we can't access mappings for
					}
				}
			}
			
			oidcMappings = allMappings;
		} catch (e) {
			oidcMappingsError = e instanceof Error ? e.message : 'Failed to load OIDC mappings';
		} finally {
			oidcMappingsLoading = false;
		}
	}

	async function openAddOidcMappingModal() {
		newMappingProviderId = null;
		newMappingOidcGroup = '';
		newMappingRole = 'member';
		addOidcMappingError = null;
		showAddOidcMappingModal = true;

		try {
			const providers = await apiMethods.admin.authProviders.list();
			authProviders = providers.filter(p => p.provider_type === 'oidc' && p.enabled);
		} catch (e) {
			addOidcMappingError = e instanceof Error ? e.message : 'Failed to load auth providers';
		}
	}

	async function createOidcMapping() {
		if (!oidcMappingsGroupId || !newMappingProviderId || !newMappingOidcGroup.trim()) {
			addOidcMappingError = 'Please fill in all fields';
			return;
		}

		addOidcMappingLoading = true;
		addOidcMappingError = null;
		try {
			await apiMethods.admin.authProviders.groupMappings.create(newMappingProviderId, {
				oidc_group_claim: newMappingOidcGroup.trim(),
				meticulous_group_id: oidcMappingsGroupId,
				role: newMappingRole
			});
			showAddOidcMappingModal = false;
			
			// Reload mappings
			const group = groups.find(g => g.id === oidcMappingsGroupId);
			if (group) {
				await openOidcMappingsModal(group);
			}
		} catch (e) {
			addOidcMappingError = e instanceof Error ? e.message : 'Failed to create mapping';
		} finally {
			addOidcMappingLoading = false;
		}
	}

	async function deleteOidcMapping(providerId: string, mappingId: string) {
		if (!confirm('Remove this OIDC group mapping?')) return;

		try {
			await apiMethods.admin.authProviders.groupMappings.delete(providerId, mappingId);
			oidcMappings = oidcMappings.filter(m => m.id !== mappingId);
		} catch (e) {
			alert(e instanceof Error ? e.message : 'Failed to delete mapping');
		}
	}
</script>

<div class="space-y-6">
	<div class="flex items-center justify-between gap-4">
		<div>
			<h2 class="text-lg font-semibold text-[var(--text-primary)]">Groups</h2>
			<p class="text-sm text-[var(--text-secondary)]">Organize users into groups for easier permission management</p>
		</div>
		<button
			type="button"
			onclick={openCreateModal}
			class="inline-flex items-center gap-2 rounded-lg bg-primary-600 px-4 py-2 text-sm font-medium text-white hover:bg-primary-700"
		>
			<Plus class="h-4 w-4" />
			Create Group
		</button>
	</div>

	<div class="relative">
		<Search class="absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-[var(--text-tertiary)]" />
		<input
			type="text"
			bind:value={searchQuery}
			placeholder="Search groups..."
			class="w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] py-2 pl-10 pr-4 text-sm text-[var(--text-primary)] placeholder-[var(--text-tertiary)] focus:border-primary-500 focus:outline-none focus:ring-1 focus:ring-primary-500"
		/>
	</div>

	{#if loading}
		<div class="flex items-center justify-center py-12">
			<div class="h-8 w-8 animate-spin rounded-full border-2 border-primary-500 border-t-transparent"></div>
		</div>
	{:else if error}
		<div class="rounded-lg border border-red-200 bg-red-50 p-4 text-sm text-red-700 dark:border-red-900 dark:bg-red-950/50 dark:text-red-400">
			{error}
		</div>
	{:else if filteredGroups.length === 0}
		<div class="flex flex-col items-center justify-center rounded-lg border border-dashed border-[var(--border-primary)] py-12">
			<UserCog class="h-12 w-12 text-[var(--text-tertiary)]" />
			<h3 class="mt-4 text-sm font-medium text-[var(--text-primary)]">
				{searchQuery ? 'No groups found' : 'No groups yet'}
			</h3>
			<p class="mt-1 text-sm text-[var(--text-secondary)]">
				{searchQuery ? 'Try adjusting your search' : 'Create your first group to get started'}
			</p>
		</div>
	{:else}
		<div class="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
			{#each pagedGroups as group (group.id)}
				<div class="rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] p-4">
					<div class="flex items-start justify-between">
						<div class="flex items-center gap-3">
							<div class="flex h-10 w-10 items-center justify-center rounded-lg bg-[var(--bg-primary)]">
								<Users class="h-5 w-5 text-[var(--text-secondary)]" />
							</div>
							<div>
								<h3 class="font-medium text-[var(--text-primary)]">
									<a
										href="/admin/groups/{group.id}"
										class="hover:text-primary-600 hover:underline dark:hover:text-primary-400"
									>
										{group.name}
									</a>
								</h3>
								<p class="text-sm text-[var(--text-secondary)]">
									{group.member_count} {group.member_count === 1 ? 'member' : 'members'}
								</p>
							</div>
						</div>
						<div class="relative">
							<button
								type="button"
								onclick={() => toggleActionMenu(group.id)}
								class="rounded p-1 text-[var(--text-secondary)] hover:bg-[var(--bg-primary)] hover:text-[var(--text-primary)]"
							>
								<MoreVertical class="h-4 w-4" />
							</button>
							{#if actionMenuOpen === group.id}
								<div class="absolute right-0 z-10 mt-1 w-48 rounded-lg border border-[var(--border-primary)] bg-[var(--bg-primary)] py-1 shadow-lg">
								<button
									type="button"
									onclick={() => goto(`/admin/groups/${group.id}`)}
									class="flex w-full items-center gap-2 px-4 py-2 text-left text-sm text-[var(--text-primary)] hover:bg-[var(--bg-hover)]"
								>
									<Users class="h-4 w-4" />
									Open group
								</button>
								<button
									type="button"
									onclick={() => openOidcMappingsModal(group)}
									class="flex w-full items-center gap-2 px-4 py-2 text-left text-sm text-[var(--text-primary)] hover:bg-[var(--bg-hover)]"
								>
									<Link2 class="h-4 w-4" />
									OIDC Mappings
								</button>
								<button
									type="button"
									onclick={() => openEditModal(group)}
									class="flex w-full items-center gap-2 px-4 py-2 text-left text-sm text-[var(--text-primary)] hover:bg-[var(--bg-hover)]"
								>
									<Edit class="h-4 w-4" />
									Edit Group
								</button>
								<button
									type="button"
									onclick={() => deleteGroup(group.id, group.name)}
									class="flex w-full items-center gap-2 px-4 py-2 text-left text-sm text-red-600 hover:bg-[var(--bg-hover)] dark:text-red-400"
								>
									<Trash2 class="h-4 w-4" />
									Delete Group
								</button>
								</div>
							{/if}
						</div>
					</div>
					{#if group.description}
						<p class="mt-3 text-sm text-[var(--text-secondary)]">{group.description}</p>
					{/if}
				</div>
			{/each}
		</div>
		{#if filteredGroups.length > listPerPage}
			<div class="mt-4">
				<Pagination bind:page={listPage} bind:perPage={listPerPage} total={filteredGroups.length} />
			</div>
		{/if}
	{/if}
</div>

<!-- Create Group Modal -->
{#if showCreateModal}
	<div class="fixed inset-0 z-50 flex items-center justify-center bg-black/50" onclick={() => showCreateModal = false}>
		<div class="w-full max-w-md rounded-lg bg-[var(--bg-primary)] p-6 shadow-xl" onclick={(e) => e.stopPropagation()}>
			<div class="flex items-center justify-between">
				<h3 class="text-lg font-semibold text-[var(--text-primary)]">Create Group</h3>
				<button
					type="button"
					onclick={() => showCreateModal = false}
					class="rounded p-1 text-[var(--text-secondary)] hover:bg-[var(--bg-secondary)]"
				>
					<X class="h-5 w-5" />
				</button>
			</div>
			{#if createError}
				<div class="mt-4 rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-700 dark:border-red-900 dark:bg-red-950/50 dark:text-red-400">
					{createError}
				</div>
			{/if}
			<div class="mt-4 space-y-4">
				<div>
					<label for="create-name" class="block text-sm font-medium text-[var(--text-primary)]">Name</label>
					<input
						type="text"
						id="create-name"
						bind:value={createName}
						placeholder="Enter group name"
						class="mt-1 w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm text-[var(--text-primary)] focus:border-primary-500 focus:outline-none focus:ring-1 focus:ring-primary-500"
					/>
				</div>
				<div>
					<label for="create-description" class="block text-sm font-medium text-[var(--text-primary)]">Description</label>
					<textarea
						id="create-description"
						bind:value={createDescription}
						placeholder="Enter description (optional)"
						rows="3"
						class="mt-1 w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm text-[var(--text-primary)] focus:border-primary-500 focus:outline-none focus:ring-1 focus:ring-primary-500"
					></textarea>
				</div>
			</div>
			<div class="mt-6 flex justify-end gap-3">
				<button
					type="button"
					onclick={() => showCreateModal = false}
					class="rounded-lg border border-[var(--border-primary)] px-4 py-2 text-sm font-medium text-[var(--text-primary)] hover:bg-[var(--bg-secondary)]"
				>
					Cancel
				</button>
				<button
					type="button"
					onclick={createGroup}
					disabled={createLoading || !createName.trim()}
					class="rounded-lg bg-primary-600 px-4 py-2 text-sm font-medium text-white hover:bg-primary-700 disabled:opacity-50"
				>
					{createLoading ? 'Creating...' : 'Create Group'}
				</button>
			</div>
		</div>
	</div>
{/if}

<!-- Edit Group Modal -->
{#if showEditModal}
	<div class="fixed inset-0 z-50 flex items-center justify-center bg-black/50" onclick={() => showEditModal = false}>
		<div class="w-full max-w-md rounded-lg bg-[var(--bg-primary)] p-6 shadow-xl" onclick={(e) => e.stopPropagation()}>
			<div class="flex items-center justify-between">
				<h3 class="text-lg font-semibold text-[var(--text-primary)]">Edit Group</h3>
				<button
					type="button"
					onclick={() => showEditModal = false}
					class="rounded p-1 text-[var(--text-secondary)] hover:bg-[var(--bg-secondary)]"
				>
					<X class="h-5 w-5" />
				</button>
			</div>
			{#if editError}
				<div class="mt-4 rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-700 dark:border-red-900 dark:bg-red-950/50 dark:text-red-400">
					{editError}
				</div>
			{/if}
			<div class="mt-4 space-y-4">
				<div>
					<label for="edit-name" class="block text-sm font-medium text-[var(--text-primary)]">Name</label>
					<input
						type="text"
						id="edit-name"
						bind:value={editName}
						class="mt-1 w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm text-[var(--text-primary)] focus:border-primary-500 focus:outline-none focus:ring-1 focus:ring-primary-500"
					/>
				</div>
				<div>
					<label for="edit-description" class="block text-sm font-medium text-[var(--text-primary)]">Description</label>
					<textarea
						id="edit-description"
						bind:value={editDescription}
						rows="3"
						class="mt-1 w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm text-[var(--text-primary)] focus:border-primary-500 focus:outline-none focus:ring-1 focus:ring-primary-500"
					></textarea>
				</div>
			</div>
			<div class="mt-6 flex justify-end gap-3">
				<button
					type="button"
					onclick={() => showEditModal = false}
					class="rounded-lg border border-[var(--border-primary)] px-4 py-2 text-sm font-medium text-[var(--text-primary)] hover:bg-[var(--bg-secondary)]"
				>
					Cancel
				</button>
				<button
					type="button"
					onclick={updateGroup}
					disabled={editLoading || !editName.trim()}
					class="rounded-lg bg-primary-600 px-4 py-2 text-sm font-medium text-white hover:bg-primary-700 disabled:opacity-50"
				>
					{editLoading ? 'Saving...' : 'Save Changes'}
				</button>
			</div>
		</div>
	</div>
{/if}

<!-- OIDC Mappings Modal -->
{#if showOidcMappingsModal}
	<div class="fixed inset-0 z-50 flex items-center justify-center bg-black/50" onclick={() => showOidcMappingsModal = false}>
		<div class="w-full max-w-2xl rounded-lg bg-[var(--bg-primary)] p-6 shadow-xl" onclick={(e) => e.stopPropagation()}>
			<div class="flex items-center justify-between">
				<div>
					<h3 class="text-lg font-semibold text-[var(--text-primary)]">OIDC Group Mappings</h3>
					<p class="mt-1 text-sm text-[var(--text-secondary)]">
						Map OIDC groups to <strong>{oidcMappingsGroupName}</strong> for automatic membership
					</p>
				</div>
				<button
					type="button"
					onclick={() => showOidcMappingsModal = false}
					class="rounded p-1 text-[var(--text-secondary)] hover:bg-[var(--bg-secondary)]"
				>
					<X class="h-5 w-5" />
				</button>
			</div>

			{#if oidcMappingsError}
				<div class="mt-4 rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-700 dark:border-red-900 dark:bg-red-950/50 dark:text-red-400">
					{oidcMappingsError}
				</div>
			{/if}

			<div class="mt-4">
				<button
					type="button"
					onclick={openAddOidcMappingModal}
					class="inline-flex items-center gap-2 rounded-lg bg-primary-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-primary-700"
				>
					<Plus class="h-4 w-4" />
					Add Mapping
				</button>
			</div>

			<div class="mt-4 max-h-96 overflow-auto">
				{#if oidcMappingsLoading}
					<div class="flex items-center justify-center py-8">
						<div class="h-6 w-6 animate-spin rounded-full border-2 border-primary-500 border-t-transparent"></div>
					</div>
				{:else if oidcMappings.length === 0}
					<div class="rounded-lg border border-dashed border-[var(--border-primary)] py-8 text-center">
						<Link2 class="mx-auto h-8 w-8 text-[var(--text-tertiary)]" />
						<p class="mt-2 text-sm text-[var(--text-secondary)]">
							No OIDC mappings yet. Add a mapping to auto-assign users from OIDC groups.
						</p>
					</div>
				{:else}
					<table class="min-w-full divide-y divide-[var(--border-primary)]">
						<thead>
							<tr>
								<th class="px-3 py-2 text-left text-xs font-medium uppercase text-[var(--text-secondary)]">Provider</th>
								<th class="px-3 py-2 text-left text-xs font-medium uppercase text-[var(--text-secondary)]">OIDC Group</th>
								<th class="px-3 py-2 text-left text-xs font-medium uppercase text-[var(--text-secondary)]">Role</th>
								<th class="px-3 py-2 text-right text-xs font-medium uppercase text-[var(--text-secondary)]">Actions</th>
							</tr>
						</thead>
						<tbody class="divide-y divide-[var(--border-primary)]">
							{#each oidcMappings as mapping (mapping.id)}
								<tr>
									<td class="px-3 py-2 text-sm text-[var(--text-primary)]">{mapping.provider_name}</td>
									<td class="px-3 py-2">
										<code class="rounded bg-[var(--bg-secondary)] px-1.5 py-0.5 text-sm text-[var(--text-primary)]">
											{mapping.oidc_group_claim}
										</code>
									</td>
									<td class="px-3 py-2 text-sm capitalize text-[var(--text-secondary)]">{mapping.role}</td>
									<td class="px-3 py-2 text-right">
										<button
											type="button"
											onclick={() => deleteOidcMapping(mapping.provider_id, mapping.id)}
											class="rounded p-1 text-red-600 hover:bg-red-50 dark:text-red-400 dark:hover:bg-red-900/20"
										>
											<Trash2 class="h-4 w-4" />
										</button>
									</td>
								</tr>
							{/each}
						</tbody>
					</table>
				{/if}
			</div>

			<div class="mt-6 flex justify-end">
				<button
					type="button"
					onclick={() => showOidcMappingsModal = false}
					class="rounded-lg border border-[var(--border-primary)] px-4 py-2 text-sm font-medium text-[var(--text-primary)] hover:bg-[var(--bg-secondary)]"
				>
					Close
				</button>
			</div>
		</div>
	</div>
{/if}

<!-- Add OIDC Mapping Modal -->
{#if showAddOidcMappingModal}
	<div class="fixed inset-0 z-50 flex items-center justify-center bg-black/50" onclick={() => showAddOidcMappingModal = false}>
		<div class="w-full max-w-md rounded-lg bg-[var(--bg-primary)] p-6 shadow-xl" onclick={(e) => e.stopPropagation()}>
			<div class="flex items-center justify-between">
				<h3 class="text-lg font-semibold text-[var(--text-primary)]">Add OIDC Mapping</h3>
				<button
					type="button"
					onclick={() => showAddOidcMappingModal = false}
					class="rounded p-1 text-[var(--text-secondary)] hover:bg-[var(--bg-secondary)]"
				>
					<X class="h-5 w-5" />
				</button>
			</div>
			<p class="mt-2 text-sm text-[var(--text-secondary)]">
				When users log in via OIDC with this group, they'll be automatically added to <strong>{oidcMappingsGroupName}</strong>.
			</p>
			{#if addOidcMappingError}
				<div class="mt-4 rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-700 dark:border-red-900 dark:bg-red-950/50 dark:text-red-400">
					{addOidcMappingError}
				</div>
			{/if}
			<div class="mt-4 space-y-4">
				<div>
					<label for="oidc-provider" class="block text-sm font-medium text-[var(--text-primary)]">Identity Provider</label>
					<select
						id="oidc-provider"
						bind:value={newMappingProviderId}
						class="mt-1 w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm text-[var(--text-primary)] focus:border-primary-500 focus:outline-none focus:ring-1 focus:ring-primary-500"
					>
						<option value={null}>Select a provider...</option>
						{#each authProviders as provider (provider.id)}
							<option value={provider.id}>{provider.name}</option>
						{/each}
					</select>
					{#if authProviders.length === 0}
						<p class="mt-1 text-xs text-[var(--text-tertiary)]">No enabled OIDC providers found</p>
					{/if}
				</div>
				<div>
					<label for="oidc-group" class="block text-sm font-medium text-[var(--text-primary)]">OIDC Group Name</label>
					<input
						type="text"
						id="oidc-group"
						bind:value={newMappingOidcGroup}
						placeholder="e.g., /developers or admin-team"
						class="mt-1 w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm text-[var(--text-primary)] focus:border-primary-500 focus:outline-none focus:ring-1 focus:ring-primary-500"
					/>
					<p class="mt-1 text-xs text-[var(--text-tertiary)]">The exact group name from your OIDC provider's groups claim</p>
				</div>
				<div>
					<label for="oidc-role" class="block text-sm font-medium text-[var(--text-primary)]">Role</label>
					<select
						id="oidc-role"
						bind:value={newMappingRole}
						class="mt-1 w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm text-[var(--text-primary)] focus:border-primary-500 focus:outline-none focus:ring-1 focus:ring-primary-500"
					>
						<option value="member">Member</option>
						<option value="maintainer">Maintainer</option>
						<option value="owner">Owner</option>
					</select>
					<p class="mt-1 text-xs text-[var(--text-tertiary)]">The role users will be assigned in this group</p>
				</div>
			</div>
			<div class="mt-6 flex justify-end gap-3">
				<button
					type="button"
					onclick={() => showAddOidcMappingModal = false}
					class="rounded-lg border border-[var(--border-primary)] px-4 py-2 text-sm font-medium text-[var(--text-primary)] hover:bg-[var(--bg-secondary)]"
				>
					Cancel
				</button>
				<button
					type="button"
					onclick={createOidcMapping}
					disabled={addOidcMappingLoading || !newMappingProviderId || !newMappingOidcGroup.trim()}
					class="rounded-lg bg-primary-600 px-4 py-2 text-sm font-medium text-white hover:bg-primary-700 disabled:opacity-50"
				>
					{addOidcMappingLoading ? 'Creating...' : 'Create Mapping'}
				</button>
			</div>
		</div>
	</div>
{/if}
