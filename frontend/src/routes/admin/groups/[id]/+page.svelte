<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import {
		Users,
		Plus,
		Search,
		Trash2,
		Edit,
		UserPlus,
		X,
		ArrowLeft,
		FolderKanban
	} from 'lucide-svelte';
	import { apiMethods } from '$lib/api';
	import type {
		AdminGroup,
		GroupMember,
		AdminUser,
		AuthProviderResponse,
		GroupMappingResponse,
		AdminGroupResourceProjectRow,
		AdminGroupResourcePipelineRow
	} from '$lib/api/types';
	import Pagination from '$lib/components/data/Pagination.svelte';

	let { data } = $props();

	let group = $state<AdminGroup | null>(null);
	let loading = $state(true);
	let error = $state<string | null>(null);

	let members = $state<GroupMember[]>([]);
	let membersLoading = $state(false);
	let membersError = $state<string | null>(null);
	let memberSearch = $state('');
	let memPage = $state(1);
	let memPerPage = $state(20);

	let resourceProjects = $state<AdminGroupResourceProjectRow[]>([]);
	let resourcePipelines = $state<AdminGroupResourcePipelineRow[]>([]);
	let resourceLoading = $state(false);
	let resourceError = $state<string | null>(null);
	let projPage = $state(1);
	let projPerPage = $state(20);
	let pipePage = $state(1);
	let pipePerPage = $state(20);

	let oidcMappings = $state<(GroupMappingResponse & { provider_name: string })[]>([]);
	let oidcLoading = $state(false);
	let oidcError = $state<string | null>(null);
	let oidcPage = $state(1);
	let oidcPerPage = $state(20);

	let showEditModal = $state(false);
	let editName = $state('');
	let editDescription = $state('');
	let editLoading = $state(false);
	let editError = $state<string | null>(null);

	let showAddMemberModal = $state(false);
	let availableUsers = $state<AdminUser[]>([]);
	let addUserSearch = $state('');
	let selectedUserId = $state('');
	let selectedRole = $state<'member' | 'maintainer' | 'owner'>('member');
	let addMemberLoading = $state(false);
	let addMemberError = $state<string | null>(null);

	let showAddOidcModal = $state(false);
	let authProviders = $state<AuthProviderResponse[]>([]);
	let newMappingProviderId = $state<string | null>(null);
	let newMappingOidcGroup = $state('');
	let newMappingRole = $state<'member' | 'maintainer' | 'owner'>('member');
	let addOidcLoading = $state(false);
	let addOidcError = $state<string | null>(null);

	const filteredMembers = $derived(
		members.filter((m) => {
			const q = memberSearch.trim().toLowerCase();
			if (!q) return true;
			return (
				m.username.toLowerCase().includes(q) ||
				m.email.toLowerCase().includes(q) ||
				(m.display_name?.toLowerCase().includes(q) ?? false)
			);
		})
	);

	const pagedMembers = $derived(
		filteredMembers.slice((memPage - 1) * memPerPage, memPage * memPerPage)
	);

	const pagedResourceProjects = $derived(
		resourceProjects.slice((projPage - 1) * projPerPage, projPage * projPerPage)
	);

	const pagedResourcePipelines = $derived(
		resourcePipelines.slice((pipePage - 1) * pipePerPage, pipePage * pipePerPage)
	);

	const pagedOidc = $derived(
		oidcMappings.slice((oidcPage - 1) * oidcPerPage, oidcPage * oidcPerPage)
	);

	const filteredAddUsers = $derived(
		availableUsers.filter((u) => {
			const q = addUserSearch.trim().toLowerCase();
			if (!q) return true;
			return (
				u.username.toLowerCase().includes(q) ||
				u.email.toLowerCase().includes(q) ||
				(u.display_name?.toLowerCase().includes(q) ?? false)
			);
		})
	);

	$effect(() => {
		memberSearch;
		memPage = 1;
	});

	$effect(() => {
		const n = filteredMembers.length;
		const maxPage = Math.max(1, Math.ceil(n / memPerPage) || 1);
		if (memPage > maxPage) memPage = maxPage;
	});

	async function loadGroup() {
		loading = true;
		error = null;
		try {
			group = await apiMethods.admin.groups.get(data.groupId);
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load group';
			group = null;
		} finally {
			loading = false;
		}
	}

	async function loadMembers() {
		membersLoading = true;
		membersError = null;
		try {
			members = await apiMethods.admin.groups.listMembers(data.groupId);
		} catch (e) {
			membersError = e instanceof Error ? e.message : 'Failed to load members';
			members = [];
		} finally {
			membersLoading = false;
		}
	}

	async function loadResourceAccess() {
		resourceLoading = true;
		resourceError = null;
		try {
			const r = await apiMethods.admin.groups.resourceAccess(data.groupId);
			resourceProjects = r.projects;
			resourcePipelines = r.pipelines;
		} catch (e) {
			resourceError = e instanceof Error ? e.message : 'Failed to load resource access';
			resourceProjects = [];
			resourcePipelines = [];
		} finally {
			resourceLoading = false;
		}
	}

	async function loadOidcMappings() {
		oidcLoading = true;
		oidcError = null;
		try {
			const providers = await apiMethods.admin.authProviders.list();
			const allMappings: (GroupMappingResponse & { provider_name: string })[] = [];
			for (const provider of providers) {
				if (provider.provider_type !== 'oidc') continue;
				try {
					const mappings = await apiMethods.admin.authProviders.groupMappings.list(provider.id);
					for (const mapping of mappings) {
						if (mapping.meticulous_group_id === data.groupId) {
							allMappings.push({ ...mapping, provider_name: provider.name });
						}
					}
				} catch {
					/* skip */
				}
			}
			oidcMappings = allMappings;
		} catch (e) {
			oidcError = e instanceof Error ? e.message : 'Failed to load OIDC mappings';
			oidcMappings = [];
		} finally {
			oidcLoading = false;
		}
	}

	onMount(async () => {
		await loadGroup();
		if (group) {
			await Promise.all([loadMembers(), loadResourceAccess(), loadOidcMappings()]);
		}
	});

	function openEditModal() {
		if (!group) return;
		editName = group.name;
		editDescription = group.description ?? '';
		editError = null;
		showEditModal = true;
	}

	async function updateGroup() {
		if (!group || !editName.trim()) {
			editError = 'Name is required';
			return;
		}
		editLoading = true;
		editError = null;
		try {
			group = await apiMethods.admin.groups.update(data.groupId, {
				name: editName.trim(),
				description: editDescription.trim() || undefined
			});
			showEditModal = false;
		} catch (e) {
			editError = e instanceof Error ? e.message : 'Failed to update group';
		} finally {
			editLoading = false;
		}
	}

	async function deleteGroup() {
		if (!group) return;
		if (!confirm(`Delete group "${group.name}"? This cannot be undone.`)) return;
		try {
			await apiMethods.admin.groups.delete(data.groupId);
			goto('/admin/groups');
		} catch (e) {
			alert(e instanceof Error ? e.message : 'Failed to delete group');
		}
	}

	async function openAddMemberModal() {
		selectedUserId = '';
		selectedRole = 'member';
		addUserSearch = '';
		addMemberError = null;
		showAddMemberModal = true;
		try {
			const response = await apiMethods.admin.users.list({ limit: 500 });
			const existing = new Set(members.map((m) => m.user_id));
			availableUsers = response.data.filter((u) => !existing.has(u.id));
		} catch (e) {
			addMemberError = e instanceof Error ? e.message : 'Failed to load users';
		}
	}

	async function addMember() {
		if (!selectedUserId.trim()) {
			addMemberError = 'Please select a user';
			return;
		}
		addMemberLoading = true;
		addMemberError = null;
		try {
			await apiMethods.admin.groups.addMember(data.groupId, selectedUserId, selectedRole);
			showAddMemberModal = false;
			await loadMembers();
			await loadGroup();
		} catch (e) {
			addMemberError = e instanceof Error ? e.message : 'Failed to add member';
		} finally {
			addMemberLoading = false;
		}
	}

	async function removeMember(userId: string, username: string) {
		if (!confirm(`Remove ${username} from this group?`)) return;
		try {
			await apiMethods.admin.groups.removeMember(data.groupId, userId);
			await loadMembers();
			await loadGroup();
		} catch (e) {
			alert(e instanceof Error ? e.message : 'Failed to remove member');
		}
	}

	async function updateMemberRole(userId: string, newRole: string) {
		try {
			await apiMethods.admin.groups.updateMember(data.groupId, userId, newRole);
			members = await apiMethods.admin.groups.listMembers(data.groupId);
		} catch (e) {
			alert(e instanceof Error ? e.message : 'Failed to update role');
		}
	}

	async function openAddOidcModal() {
		newMappingProviderId = null;
		newMappingOidcGroup = '';
		newMappingRole = 'member';
		addOidcError = null;
		showAddOidcModal = true;
		try {
			const providers = await apiMethods.admin.authProviders.list();
			authProviders = providers.filter((p) => p.provider_type === 'oidc' && p.enabled);
		} catch (e) {
			addOidcError = e instanceof Error ? e.message : 'Failed to load auth providers';
		}
	}

	async function createOidcMapping() {
		if (!newMappingProviderId || !newMappingOidcGroup.trim()) {
			addOidcError = 'Please fill in all fields';
			return;
		}
		addOidcLoading = true;
		addOidcError = null;
		try {
			await apiMethods.admin.authProviders.groupMappings.create(newMappingProviderId, {
				oidc_group_claim: newMappingOidcGroup.trim(),
				meticulous_group_id: data.groupId,
				role: newMappingRole
			});
			showAddOidcModal = false;
			await loadOidcMappings();
		} catch (e) {
			addOidcError = e instanceof Error ? e.message : 'Failed to create mapping';
		} finally {
			addOidcLoading = false;
		}
	}

	async function deleteOidcMapping(providerId: string, mappingId: string) {
		if (!confirm('Remove this OIDC group mapping?')) return;
		try {
			await apiMethods.admin.authProviders.groupMappings.delete(providerId, mappingId);
			oidcMappings = oidcMappings.filter((m) => m.id !== mappingId);
		} catch (e) {
			alert(e instanceof Error ? e.message : 'Failed to delete mapping');
		}
	}
</script>

<div class="space-y-6">
	<div class="flex items-center gap-4">
		<a
			href="/admin/groups"
			class="rounded-lg p-2 text-[var(--text-secondary)] hover:bg-[var(--bg-secondary)] hover:text-[var(--text-primary)]"
		>
			<ArrowLeft class="h-5 w-5" />
		</a>
		<h2 class="text-lg font-semibold text-[var(--text-primary)]">Group</h2>
	</div>

	{#if loading}
		<div class="flex items-center justify-center py-12">
			<div class="h-8 w-8 animate-spin rounded-full border-2 border-primary-500 border-t-transparent"></div>
		</div>
	{:else if error}
		<div
			class="rounded-lg border border-red-200 bg-red-50 p-4 text-sm text-red-700 dark:border-red-900 dark:bg-red-950/50 dark:text-red-400"
		>
			{error}
		</div>
	{:else if group}
		<div class="grid gap-6 lg:grid-cols-3">
			<div class="lg:col-span-1">
				<div class="rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] p-6">
					<div class="flex flex-col items-center text-center">
						<div class="flex h-16 w-16 items-center justify-center rounded-xl bg-[var(--bg-primary)]">
							<Users class="h-8 w-8 text-[var(--text-secondary)]" />
						</div>
						<h3 class="mt-4 text-xl font-semibold text-[var(--text-primary)]">{group.name}</h3>
						<p class="text-sm text-[var(--text-secondary)]">
							{group.member_count} {group.member_count === 1 ? 'member' : 'members'}
						</p>
					</div>
					{#if group.description}
						<p class="mt-4 border-t border-[var(--border-primary)] pt-4 text-sm text-[var(--text-secondary)]">
							{group.description}
						</p>
					{/if}
					<div class="mt-6 space-y-2 border-t border-[var(--border-primary)] pt-6">
						<button
							type="button"
							onclick={openEditModal}
							class="flex w-full items-center gap-2 rounded-lg px-3 py-2 text-sm text-[var(--text-primary)] hover:bg-[var(--bg-primary)]"
						>
							<Edit class="h-4 w-4" />
							Edit group
						</button>
						<button
							type="button"
							onclick={deleteGroup}
							class="flex w-full items-center gap-2 rounded-lg px-3 py-2 text-sm text-red-600 hover:bg-red-50 dark:text-red-400 dark:hover:bg-red-900/20"
						>
							<Trash2 class="h-4 w-4" />
							Delete group
						</button>
					</div>
				</div>
			</div>

			<div class="space-y-6 lg:col-span-2">
				<div class="rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] p-6">
					<div class="flex flex-wrap items-center justify-between gap-3">
						<h3 class="text-base font-semibold text-[var(--text-primary)]">Members</h3>
						<button
							type="button"
							onclick={openAddMemberModal}
							class="inline-flex items-center gap-2 rounded-lg bg-primary-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-primary-700"
						>
							<UserPlus class="h-4 w-4" />
							Add member
						</button>
					</div>
					<div class="relative mt-4">
						<Search class="absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-[var(--text-tertiary)]" />
						<input
							type="text"
							bind:value={memberSearch}
							placeholder="Search members by name or email…"
							class="w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-primary)] py-2 pl-10 pr-4 text-sm"
						/>
					</div>
					{#if membersError}
						<div
							class="mt-4 rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-700 dark:border-red-900 dark:bg-red-950/50 dark:text-red-400"
						>
							{membersError}
						</div>
					{/if}
					{#if membersLoading}
						<div class="flex justify-center py-8">
							<div class="h-6 w-6 animate-spin rounded-full border-2 border-primary-500 border-t-transparent"></div>
						</div>
					{:else if filteredMembers.length === 0}
						<p class="mt-4 text-sm text-[var(--text-secondary)]">No members match this filter.</p>
					{:else}
						<div class="mt-4 overflow-x-auto rounded-lg border border-[var(--border-primary)]">
							<table class="min-w-full divide-y divide-[var(--border-primary)] text-sm">
								<thead class="bg-[var(--bg-primary)]">
									<tr>
										<th class="px-3 py-2 text-left text-xs font-medium text-[var(--text-secondary)]">User</th>
										<th class="px-3 py-2 text-left text-xs font-medium text-[var(--text-secondary)]">Role</th>
										<th class="px-3 py-2 text-left text-xs font-medium text-[var(--text-secondary)]">Joined</th>
										<th class="px-3 py-2 text-right text-xs font-medium text-[var(--text-secondary)]"></th>
									</tr>
								</thead>
								<tbody class="divide-y divide-[var(--border-primary)]">
									{#each pagedMembers as member (member.user_id)}
										<tr>
											<td class="px-3 py-2">
												<a
													href="/admin/users/{member.user_id}"
													class="font-medium text-primary-600 hover:underline dark:text-primary-400"
												>
													{member.display_name || member.username}
												</a>
												<div class="text-xs text-[var(--text-secondary)]">{member.email}</div>
											</td>
											<td class="px-3 py-2">
												<select
													value={member.role}
													onchange={(e) =>
														updateMemberRole(
															member.user_id,
															(e.target as HTMLSelectElement).value
														)}
													class="rounded border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-2 py-1 text-sm"
												>
													<option value="member">Member</option>
													<option value="maintainer">Maintainer</option>
													<option value="owner">Owner</option>
												</select>
											</td>
											<td class="px-3 py-2 text-[var(--text-secondary)]">
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
						</div>
						{#if filteredMembers.length > memPerPage}
							<div class="mt-4">
								<Pagination bind:page={memPage} bind:perPage={memPerPage} total={filteredMembers.length} />
							</div>
						{/if}
					{/if}
				</div>

				<div class="rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] p-6">
					<div class="flex items-center gap-2">
						<FolderKanban class="h-5 w-5 text-[var(--text-secondary)]" />
						<h3 class="text-base font-semibold text-[var(--text-primary)]">Project &amp; pipeline access</h3>
					</div>
					<p class="mt-1 text-sm text-[var(--text-secondary)]">
						Resources where this group is listed on the access control list.
					</p>
					{#if resourceError}
						<div
							class="mt-4 rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-700 dark:border-red-900 dark:bg-red-950/50 dark:text-red-400"
						>
							{resourceError}
						</div>
					{/if}
					{#if resourceLoading}
						<div class="mt-6 flex justify-center py-8">
							<div class="h-8 w-8 animate-spin rounded-full border-2 border-primary-500 border-t-transparent"></div>
						</div>
					{:else}
						<div class="mt-6 space-y-6">
							<div>
								<h4 class="text-sm font-medium text-[var(--text-primary)]">Projects</h4>
								{#if resourceProjects.length === 0}
									<p class="mt-2 text-sm text-[var(--text-secondary)]">Not used on any project ACL.</p>
								{:else}
									<div class="mt-2 overflow-x-auto rounded-lg border border-[var(--border-primary)]">
										<table class="min-w-full divide-y divide-[var(--border-primary)] text-sm">
											<thead class="bg-[var(--bg-primary)]">
												<tr>
													<th class="px-3 py-2 text-left text-xs font-medium text-[var(--text-secondary)]">Project</th>
													<th class="px-3 py-2 text-left text-xs font-medium text-[var(--text-secondary)]">Role</th>
												</tr>
											</thead>
											<tbody class="divide-y divide-[var(--border-primary)]">
												{#each pagedResourceProjects as row (row.project_id)}
													<tr>
														<td class="px-3 py-2">
															<a
																href="/projects/{row.project_id}"
																class="font-medium text-primary-600 hover:underline dark:text-primary-400"
															>
																{row.project_name}
															</a>
														</td>
														<td class="px-3 py-2 capitalize text-[var(--text-secondary)]">{row.role}</td>
													</tr>
												{/each}
											</tbody>
										</table>
									</div>
									{#if resourceProjects.length > projPerPage}
										<div class="mt-4">
											<Pagination bind:page={projPage} bind:perPage={projPerPage} total={resourceProjects.length} />
										</div>
									{/if}
								{/if}
							</div>
							<div>
								<h4 class="text-sm font-medium text-[var(--text-primary)]">Pipelines</h4>
								{#if resourcePipelines.length === 0}
									<p class="mt-2 text-sm text-[var(--text-secondary)]">Not used on any pipeline ACL.</p>
								{:else}
									<div class="mt-2 overflow-x-auto rounded-lg border border-[var(--border-primary)]">
										<table class="min-w-full divide-y divide-[var(--border-primary)] text-sm">
											<thead class="bg-[var(--bg-primary)]">
												<tr>
													<th class="px-3 py-2 text-left text-xs font-medium text-[var(--text-secondary)]">Pipeline</th>
													<th class="px-3 py-2 text-left text-xs font-medium text-[var(--text-secondary)]">Project</th>
													<th class="px-3 py-2 text-left text-xs font-medium text-[var(--text-secondary)]">Role</th>
													<th class="px-3 py-2 text-left text-xs font-medium text-[var(--text-secondary)]">Source</th>
												</tr>
											</thead>
											<tbody class="divide-y divide-[var(--border-primary)]">
												{#each pagedResourcePipelines as row (row.pipeline_id + String(row.inherited))}
													<tr>
														<td class="px-3 py-2">
															<a
																href="/pipelines/{row.pipeline_id}"
																class="font-medium text-primary-600 hover:underline dark:text-primary-400"
															>
																{row.pipeline_name}
															</a>
														</td>
														<td class="px-3 py-2 text-[var(--text-secondary)]">{row.project_name}</td>
														<td class="px-3 py-2 capitalize text-[var(--text-secondary)]">{row.role}</td>
														<td class="px-3 py-2 text-[var(--text-secondary)]">
															{row.inherited ? 'Inherited from project' : 'Direct'}
														</td>
													</tr>
												{/each}
											</tbody>
										</table>
									</div>
									{#if resourcePipelines.length > pipePerPage}
										<div class="mt-4">
											<Pagination bind:page={pipePage} bind:perPage={pipePerPage} total={resourcePipelines.length} />
										</div>
									{/if}
								{/if}
							</div>
						</div>
					{/if}
				</div>

				<div class="rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] p-6">
					<div class="flex flex-wrap items-center justify-between gap-3">
						<div>
							<h3 class="text-base font-semibold text-[var(--text-primary)]">OIDC mappings</h3>
							<p class="mt-1 text-sm text-[var(--text-secondary)]">
								OIDC groups that sync into <strong>{group.name}</strong>.
							</p>
						</div>
						<button
							type="button"
							onclick={openAddOidcModal}
							class="inline-flex items-center gap-2 rounded-lg bg-primary-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-primary-700"
						>
							<Plus class="h-4 w-4" />
							Add mapping
						</button>
					</div>
					{#if oidcError}
						<div
							class="mt-4 rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-700 dark:border-red-900 dark:bg-red-950/50 dark:text-red-400"
						>
							{oidcError}
						</div>
					{/if}
					{#if oidcLoading}
						<div class="flex justify-center py-8">
							<div class="h-6 w-6 animate-spin rounded-full border-2 border-primary-500 border-t-transparent"></div>
						</div>
					{:else if oidcMappings.length === 0}
						<p class="mt-4 text-sm text-[var(--text-secondary)]">No OIDC mappings yet.</p>
					{:else}
						<div class="mt-4 overflow-x-auto rounded-lg border border-[var(--border-primary)]">
							<table class="min-w-full divide-y divide-[var(--border-primary)] text-sm">
								<thead class="bg-[var(--bg-primary)]">
									<tr>
										<th class="px-3 py-2 text-left text-xs font-medium text-[var(--text-secondary)]">Provider</th>
										<th class="px-3 py-2 text-left text-xs font-medium text-[var(--text-secondary)]">OIDC group</th>
										<th class="px-3 py-2 text-left text-xs font-medium text-[var(--text-secondary)]">Role</th>
										<th class="px-3 py-2 text-right text-xs font-medium text-[var(--text-secondary)]"></th>
									</tr>
								</thead>
								<tbody class="divide-y divide-[var(--border-primary)]">
									{#each pagedOidc as mapping (mapping.id)}
										<tr>
											<td class="px-3 py-2 text-[var(--text-primary)]">{mapping.provider_name}</td>
											<td class="px-3 py-2">
												<code class="rounded bg-[var(--bg-primary)] px-1.5 py-0.5 text-xs">{mapping.oidc_group_claim}</code>
											</td>
											<td class="px-3 py-2 capitalize text-[var(--text-secondary)]">{mapping.role}</td>
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
						</div>
						{#if oidcMappings.length > oidcPerPage}
							<div class="mt-4">
								<Pagination bind:page={oidcPage} bind:perPage={oidcPerPage} total={oidcMappings.length} />
							</div>
						{/if}
					{/if}
				</div>
			</div>
		</div>
	{/if}
</div>

{#if showEditModal && group}
	<div class="fixed inset-0 z-50 flex items-center justify-center bg-black/50" onclick={() => (showEditModal = false)}>
		<div class="w-full max-w-md rounded-lg bg-[var(--bg-primary)] p-6 shadow-xl" onclick={(e) => e.stopPropagation()}>
			<div class="flex items-center justify-between">
				<h3 class="text-lg font-semibold text-[var(--text-primary)]">Edit group</h3>
				<button
					type="button"
					onclick={() => (showEditModal = false)}
					class="rounded p-1 text-[var(--text-secondary)] hover:bg-[var(--bg-secondary)]"
				>
					<X class="h-5 w-5" />
				</button>
			</div>
			{#if editError}
				<div
					class="mt-4 rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-700 dark:border-red-900 dark:bg-red-950/50 dark:text-red-400"
				>
					{editError}
				</div>
			{/if}
			<div class="mt-4 space-y-4">
				<div>
					<label for="g-edit-name" class="block text-sm font-medium text-[var(--text-primary)]">Name</label>
					<input
						id="g-edit-name"
						bind:value={editName}
						class="mt-1 w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm"
					/>
				</div>
				<div>
					<label for="g-edit-desc" class="block text-sm font-medium text-[var(--text-primary)]">Description</label>
					<textarea
						id="g-edit-desc"
						bind:value={editDescription}
						rows="3"
						class="mt-1 w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm"
					></textarea>
				</div>
			</div>
			<div class="mt-6 flex justify-end gap-3">
				<button
					type="button"
					onclick={() => (showEditModal = false)}
					class="rounded-lg border border-[var(--border-primary)] px-4 py-2 text-sm font-medium hover:bg-[var(--bg-secondary)]"
				>
					Cancel
				</button>
				<button
					type="button"
					onclick={updateGroup}
					disabled={editLoading || !editName.trim()}
					class="rounded-lg bg-primary-600 px-4 py-2 text-sm font-medium text-white hover:bg-primary-700 disabled:opacity-50"
				>
					{editLoading ? 'Saving…' : 'Save'}
				</button>
			</div>
		</div>
	</div>
{/if}

{#if showAddMemberModal}
	<div class="fixed inset-0 z-50 flex items-center justify-center bg-black/50" onclick={() => (showAddMemberModal = false)}>
		<div class="w-full max-w-lg rounded-lg bg-[var(--bg-primary)] p-6 shadow-xl" onclick={(e) => e.stopPropagation()}>
			<div class="flex items-center justify-between">
				<h3 class="text-lg font-semibold text-[var(--text-primary)]">Add member</h3>
				<button
					type="button"
					onclick={() => (showAddMemberModal = false)}
					class="rounded p-1 text-[var(--text-secondary)] hover:bg-[var(--bg-secondary)]"
				>
					<X class="h-5 w-5" />
				</button>
			</div>
			{#if addMemberError}
				<div
					class="mt-4 rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-700 dark:border-red-900 dark:bg-red-950/50 dark:text-red-400"
				>
					{addMemberError}
				</div>
			{/if}
			<div class="relative mt-4">
				<Search class="absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-[var(--text-tertiary)]" />
				<input
					type="text"
					bind:value={addUserSearch}
					placeholder="Search users…"
					class="w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] py-2 pl-10 pr-4 text-sm"
				/>
			</div>
			<div class="mt-4 max-h-56 space-y-1 overflow-y-auto rounded-lg border border-[var(--border-primary)] p-2">
				{#each filteredAddUsers as u (u.id)}
					<label class="flex cursor-pointer items-center gap-2 rounded px-2 py-1.5 hover:bg-[var(--bg-hover)]">
						<input type="radio" name="pick-user" value={u.id} bind:group={selectedUserId} />
						<span class="text-sm text-[var(--text-primary)]">{u.display_name || u.username}</span>
						<span class="text-xs text-[var(--text-secondary)]">{u.email}</span>
					</label>
				{:else}
					<p class="px-2 py-4 text-center text-sm text-[var(--text-secondary)]">No users to add.</p>
				{/each}
			</div>
			<div class="mt-4">
				<label for="g-add-role" class="block text-sm font-medium text-[var(--text-primary)]">Role</label>
				<select
					id="g-add-role"
					bind:value={selectedRole}
					class="mt-1 w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm"
				>
					<option value="member">Member</option>
					<option value="maintainer">Maintainer</option>
					<option value="owner">Owner</option>
				</select>
			</div>
			<div class="mt-6 flex justify-end gap-3">
				<button
					type="button"
					onclick={() => (showAddMemberModal = false)}
					class="rounded-lg border border-[var(--border-primary)] px-4 py-2 text-sm font-medium hover:bg-[var(--bg-secondary)]"
				>
					Cancel
				</button>
				<button
					type="button"
					onclick={addMember}
					disabled={addMemberLoading || !selectedUserId.trim()}
					class="rounded-lg bg-primary-600 px-4 py-2 text-sm font-medium text-white hover:bg-primary-700 disabled:opacity-50"
				>
					{addMemberLoading ? 'Adding…' : 'Add'}
				</button>
			</div>
		</div>
	</div>
{/if}

{#if showAddOidcModal}
	<div class="fixed inset-0 z-50 flex items-center justify-center bg-black/50" onclick={() => (showAddOidcModal = false)}>
		<div class="w-full max-w-md rounded-lg bg-[var(--bg-primary)] p-6 shadow-xl" onclick={(e) => e.stopPropagation()}>
			<div class="flex items-center justify-between">
				<h3 class="text-lg font-semibold text-[var(--text-primary)]">Add OIDC mapping</h3>
				<button
					type="button"
					onclick={() => (showAddOidcModal = false)}
					class="rounded p-1 text-[var(--text-secondary)] hover:bg-[var(--bg-secondary)]"
				>
					<X class="h-5 w-5" />
				</button>
			</div>
			{#if addOidcError}
				<div
					class="mt-4 rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-700 dark:border-red-900 dark:bg-red-950/50 dark:text-red-400"
				>
					{addOidcError}
				</div>
			{/if}
			<div class="mt-4 space-y-4">
				<div>
					<label for="oidc-p" class="block text-sm font-medium text-[var(--text-primary)]">Identity provider</label>
					<select
						id="oidc-p"
						bind:value={newMappingProviderId}
						class="mt-1 w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm"
					>
						<option value={null}>Select…</option>
						{#each authProviders as p (p.id)}
							<option value={p.id}>{p.name}</option>
						{/each}
					</select>
				</div>
				<div>
					<label for="oidc-g" class="block text-sm font-medium text-[var(--text-primary)]">OIDC group name</label>
					<input
						id="oidc-g"
						bind:value={newMappingOidcGroup}
						class="mt-1 w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm"
					/>
				</div>
				<div>
					<label for="oidc-r" class="block text-sm font-medium text-[var(--text-primary)]">Role</label>
					<select
						id="oidc-r"
						bind:value={newMappingRole}
						class="mt-1 w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm"
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
					onclick={() => (showAddOidcModal = false)}
					class="rounded-lg border border-[var(--border-primary)] px-4 py-2 text-sm font-medium hover:bg-[var(--bg-secondary)]"
				>
					Cancel
				</button>
				<button
					type="button"
					onclick={createOidcMapping}
					disabled={addOidcLoading || !newMappingProviderId || !newMappingOidcGroup.trim()}
					class="rounded-lg bg-primary-600 px-4 py-2 text-sm font-medium text-white hover:bg-primary-700 disabled:opacity-50"
				>
					{addOidcLoading ? 'Creating…' : 'Create'}
				</button>
			</div>
		</div>
	</div>
{/if}
