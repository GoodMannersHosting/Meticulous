<script lang="ts">
	import { Users, Plus, Search, MoreVertical, UserCog, Trash2, Edit, UserPlus, X } from 'lucide-svelte';
	import { apiMethods } from '$lib/api';
	import type { AdminGroup, GroupMember, AdminUser } from '$lib/api/types';

	let searchQuery = $state('');
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

	// Members modal
	let showMembersModal = $state(false);
	let membersGroupId = $state<string | null>(null);
	let membersGroupName = $state('');
	let members = $state<GroupMember[]>([]);
	let membersLoading = $state(false);
	let membersError = $state<string | null>(null);

	// Add member modal
	let showAddMemberModal = $state(false);
	let availableUsers = $state<AdminUser[]>([]);
	let selectedUserId = $state<string | null>(null);
	let selectedRole = $state<'member' | 'maintainer' | 'owner'>('member');
	let addMemberLoading = $state(false);
	let addMemberError = $state<string | null>(null);

	async function loadGroups() {
		loading = true;
		error = null;
		try {
			const response = await apiMethods.admin.groups.list({ limit: 100 });
			groups = response.items;
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

	async function openMembersModal(group: AdminGroup) {
		closeActionMenu();
		membersGroupId = group.id;
		membersGroupName = group.name;
		members = [];
		membersError = null;
		showMembersModal = true;
		membersLoading = true;

		try {
			members = await apiMethods.admin.groups.listMembers(group.id);
		} catch (e) {
			membersError = e instanceof Error ? e.message : 'Failed to load members';
		} finally {
			membersLoading = false;
		}
	}

	async function openAddMemberModal() {
		selectedUserId = null;
		selectedRole = 'member';
		addMemberError = null;
		showAddMemberModal = true;

		try {
			const response = await apiMethods.admin.users.list({ limit: 100 });
			const existingMemberIds = new Set(members.map(m => m.user_id));
			availableUsers = response.items.filter(u => !existingMemberIds.has(u.id));
		} catch (e) {
			addMemberError = e instanceof Error ? e.message : 'Failed to load users';
		}
	}

	async function addMember() {
		if (!membersGroupId || !selectedUserId) {
			addMemberError = 'Please select a user';
			return;
		}

		addMemberLoading = true;
		addMemberError = null;
		try {
			await apiMethods.admin.groups.addMember(membersGroupId, selectedUserId, selectedRole);
			showAddMemberModal = false;
			members = await apiMethods.admin.groups.listMembers(membersGroupId);
			await loadGroups();
		} catch (e) {
			addMemberError = e instanceof Error ? e.message : 'Failed to add member';
		} finally {
			addMemberLoading = false;
		}
	}

	async function removeMember(userId: string, username: string) {
		if (!membersGroupId) return;
		if (!confirm(`Remove ${username} from this group?`)) return;

		try {
			await apiMethods.admin.groups.removeMember(membersGroupId, userId);
			members = await apiMethods.admin.groups.listMembers(membersGroupId);
			await loadGroups();
		} catch (e) {
			alert(e instanceof Error ? e.message : 'Failed to remove member');
		}
	}

	async function updateMemberRole(userId: string, newRole: string) {
		if (!membersGroupId) return;

		try {
			await apiMethods.admin.groups.updateMember(membersGroupId, userId, newRole);
			members = await apiMethods.admin.groups.listMembers(membersGroupId);
		} catch (e) {
			alert(e instanceof Error ? e.message : 'Failed to update role');
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
			{#each filteredGroups as group (group.id)}
				<div class="rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] p-4">
					<div class="flex items-start justify-between">
						<div class="flex items-center gap-3">
							<div class="flex h-10 w-10 items-center justify-center rounded-lg bg-[var(--bg-primary)]">
								<Users class="h-5 w-5 text-[var(--text-secondary)]" />
							</div>
							<div>
								<h3 class="font-medium text-[var(--text-primary)]">{group.name}</h3>
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
										onclick={() => openMembersModal(group)}
										class="flex w-full items-center gap-2 px-4 py-2 text-left text-sm text-[var(--text-primary)] hover:bg-[var(--bg-hover)]"
									>
										<Users class="h-4 w-4" />
										Manage Members
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

<!-- Members Modal -->
{#if showMembersModal}
	<div class="fixed inset-0 z-50 flex items-center justify-center bg-black/50" onclick={() => showMembersModal = false}>
		<div class="w-full max-w-2xl rounded-lg bg-[var(--bg-primary)] p-6 shadow-xl" onclick={(e) => e.stopPropagation()}>
			<div class="flex items-center justify-between">
				<h3 class="text-lg font-semibold text-[var(--text-primary)]">Members of {membersGroupName}</h3>
				<button
					type="button"
					onclick={() => showMembersModal = false}
					class="rounded p-1 text-[var(--text-secondary)] hover:bg-[var(--bg-secondary)]"
				>
					<X class="h-5 w-5" />
				</button>
			</div>

			{#if membersError}
				<div class="mt-4 rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-700 dark:border-red-900 dark:bg-red-950/50 dark:text-red-400">
					{membersError}
				</div>
			{/if}

			<div class="mt-4">
				<button
					type="button"
					onclick={openAddMemberModal}
					class="inline-flex items-center gap-2 rounded-lg bg-primary-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-primary-700"
				>
					<UserPlus class="h-4 w-4" />
					Add Member
				</button>
			</div>

			<div class="mt-4 max-h-96 overflow-auto">
				{#if membersLoading}
					<div class="flex items-center justify-center py-8">
						<div class="h-6 w-6 animate-spin rounded-full border-2 border-primary-500 border-t-transparent"></div>
					</div>
				{:else if members.length === 0}
					<div class="py-8 text-center text-sm text-[var(--text-secondary)]">
						No members yet. Add some users to this group.
					</div>
				{:else}
					<table class="min-w-full divide-y divide-[var(--border-primary)]">
						<thead>
							<tr>
								<th class="px-3 py-2 text-left text-xs font-medium uppercase text-[var(--text-secondary)]">User</th>
								<th class="px-3 py-2 text-left text-xs font-medium uppercase text-[var(--text-secondary)]">Role</th>
								<th class="px-3 py-2 text-left text-xs font-medium uppercase text-[var(--text-secondary)]">Joined</th>
								<th class="px-3 py-2 text-right text-xs font-medium uppercase text-[var(--text-secondary)]">Actions</th>
							</tr>
						</thead>
						<tbody class="divide-y divide-[var(--border-primary)]">
							{#each members as member (member.user_id)}
								<tr>
									<td class="px-3 py-2">
										<div class="text-sm font-medium text-[var(--text-primary)]">{member.display_name || member.username}</div>
										<div class="text-xs text-[var(--text-secondary)]">{member.email}</div>
									</td>
									<td class="px-3 py-2">
										<select
											value={member.role}
											onchange={(e) => updateMemberRole(member.user_id, (e.target as HTMLSelectElement).value)}
											class="rounded border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-2 py-1 text-sm text-[var(--text-primary)]"
										>
											<option value="member">Member</option>
											<option value="maintainer">Maintainer</option>
											<option value="owner">Owner</option>
										</select>
									</td>
									<td class="px-3 py-2 text-sm text-[var(--text-secondary)]">
										{new Date(member.joined_at).toLocaleDateString()}
									</td>
									<td class="px-3 py-2 text-right">
										<button
											type="button"
											onclick={() => removeMember(member.user_id, member.username)}
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
					onclick={() => showMembersModal = false}
					class="rounded-lg border border-[var(--border-primary)] px-4 py-2 text-sm font-medium text-[var(--text-primary)] hover:bg-[var(--bg-secondary)]"
				>
					Close
				</button>
			</div>
		</div>
	</div>
{/if}

<!-- Add Member Modal -->
{#if showAddMemberModal}
	<div class="fixed inset-0 z-50 flex items-center justify-center bg-black/50" onclick={() => showAddMemberModal = false}>
		<div class="w-full max-w-md rounded-lg bg-[var(--bg-primary)] p-6 shadow-xl" onclick={(e) => e.stopPropagation()}>
			<div class="flex items-center justify-between">
				<h3 class="text-lg font-semibold text-[var(--text-primary)]">Add Member</h3>
				<button
					type="button"
					onclick={() => showAddMemberModal = false}
					class="rounded p-1 text-[var(--text-secondary)] hover:bg-[var(--bg-secondary)]"
				>
					<X class="h-5 w-5" />
				</button>
			</div>
			{#if addMemberError}
				<div class="mt-4 rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-700 dark:border-red-900 dark:bg-red-950/50 dark:text-red-400">
					{addMemberError}
				</div>
			{/if}
			<div class="mt-4 space-y-4">
				<div>
					<label for="add-user" class="block text-sm font-medium text-[var(--text-primary)]">User</label>
					<select
						id="add-user"
						bind:value={selectedUserId}
						class="mt-1 w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm text-[var(--text-primary)] focus:border-primary-500 focus:outline-none focus:ring-1 focus:ring-primary-500"
					>
						<option value={null}>Select a user...</option>
						{#each availableUsers as user (user.id)}
							<option value={user.id}>{user.display_name || user.username} ({user.email})</option>
						{/each}
					</select>
				</div>
				<div>
					<label for="add-role" class="block text-sm font-medium text-[var(--text-primary)]">Role</label>
					<select
						id="add-role"
						bind:value={selectedRole}
						class="mt-1 w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm text-[var(--text-primary)] focus:border-primary-500 focus:outline-none focus:ring-1 focus:ring-primary-500"
					>
						<option value="member">Member</option>
						<option value="maintainer">Maintainer</option>
						<option value="owner">Owner</option>
					</select>
				</div>
			</div>
			<div class="mt-6 flex justify-end gap-3">
				<button
					type="button"
					onclick={() => showAddMemberModal = false}
					class="rounded-lg border border-[var(--border-primary)] px-4 py-2 text-sm font-medium text-[var(--text-primary)] hover:bg-[var(--bg-secondary)]"
				>
					Cancel
				</button>
				<button
					type="button"
					onclick={addMember}
					disabled={addMemberLoading || !selectedUserId}
					class="rounded-lg bg-primary-600 px-4 py-2 text-sm font-medium text-white hover:bg-primary-700 disabled:opacity-50"
				>
					{addMemberLoading ? 'Adding...' : 'Add Member'}
				</button>
			</div>
		</div>
	</div>
{/if}
