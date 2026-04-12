<script lang="ts">
	import type { Member, MemberRole, AddMemberInput, PrincipalSearchResult } from '$lib/api/types';
	import { apiMethods } from '$api/client';
	import { Button, Input, Select, Alert } from '$components/ui';
	import { Plus, Trash2, UserCog, Search } from 'lucide-svelte';

	interface Props {
		members: Member[];
		loading?: boolean;
		error?: string | null;
		showInherited?: boolean;
		onAdd?: (input: AddMemberInput) => Promise<void>;
		onRemove?: (principalId: string) => Promise<void>;
		onUpdateRole?: (principalId: string, role: MemberRole) => Promise<void>;
	}

	let {
		members,
		loading = false,
		error = null,
		showInherited = false,
		onAdd,
		onRemove,
		onUpdateRole
	}: Props = $props();

	let showAddForm = $state(false);
	let searchQuery = $state('');
	let searchResults = $state<PrincipalSearchResult[]>([]);
	let searchLoading = $state(false);
	let selectedPrincipal = $state<PrincipalSearchResult | null>(null);
	let addRole = $state<MemberRole>('operator');
	let addLoading = $state(false);
	let addError = $state<string | null>(null);
	let searchTimeout: ReturnType<typeof setTimeout> | null = null;

	const roleOptions = [
		{ value: 'readonly', label: 'Read-Only' },
		{ value: 'operator', label: 'Operator' },
		{ value: 'admin', label: 'Admin' }
	];

	function roleBadgeClass(role: string): string {
		switch (role) {
			case 'admin':
				return 'bg-red-500/20 text-red-400 border-red-500/30';
			case 'operator':
				return 'bg-blue-500/20 text-blue-400 border-blue-500/30';
			default:
				return 'bg-zinc-500/20 text-zinc-400 border-zinc-500/30';
		}
	}

	function roleDisplayName(role: string): string {
		switch (role) {
			case 'admin':
				return 'Admin';
			case 'operator':
				return 'Operator';
			case 'readonly':
				return 'Read-Only';
			default:
				return role;
		}
	}

	function handleSearchInput() {
		if (searchTimeout) clearTimeout(searchTimeout);
		selectedPrincipal = null;
		if (searchQuery.trim().length < 2) {
			searchResults = [];
			return;
		}
		searchTimeout = setTimeout(async () => {
			searchLoading = true;
			try {
				searchResults = await apiMethods.principalSearch.search(searchQuery.trim());
			} catch {
				searchResults = [];
			} finally {
				searchLoading = false;
			}
		}, 250);
	}

	function selectPrincipal(p: PrincipalSearchResult) {
		selectedPrincipal = p;
		searchQuery = p.name + (p.email ? ` (${p.email})` : '');
		searchResults = [];
	}

	async function handleAdd() {
		if (!selectedPrincipal || !onAdd) return;
		addLoading = true;
		addError = null;
		try {
			await onAdd({
				principal_type: selectedPrincipal.principal_type,
				principal_id: selectedPrincipal.id,
				role: addRole
			});
			searchQuery = '';
			selectedPrincipal = null;
			addRole = 'operator';
			showAddForm = false;
		} catch (e) {
			addError = e instanceof Error ? e.message : 'Failed to add member';
		} finally {
			addLoading = false;
		}
	}

	async function handleRoleChange(principalId: string, newRole: string) {
		if (!onUpdateRole) return;
		try {
			await onUpdateRole(principalId, newRole as MemberRole);
		} catch {
			/* role change failed */
		}
	}

	async function handleRemove(principalId: string) {
		if (!onRemove) return;
		try {
			await onRemove(principalId);
		} catch {
			/* removal failed */
		}
	}
</script>

<div class="space-y-4">
	{#if error}
		<Alert variant="error">{error}</Alert>
	{/if}

	<div class="flex items-center justify-between">
		<div>
			<h3 class="text-base font-medium text-[var(--text-primary)]">Members</h3>
			<p class="text-xs text-[var(--text-secondary)]">
				Users and groups with access to this resource.
			</p>
		</div>
		{#if onAdd}
			<Button variant="outline" size="sm" onclick={() => (showAddForm = !showAddForm)}>
				<Plus class="h-4 w-4" />
				Add member
			</Button>
		{/if}
	</div>

	{#if showAddForm}
		<div class="rounded-lg border border-[var(--border-primary)] bg-[var(--bg-tertiary)] p-4 space-y-3">
			{#if addError}
				<Alert variant="error">{addError}</Alert>
			{/if}
			<div class="grid gap-3 sm:grid-cols-3">
				<div class="sm:col-span-2 relative">
					<label class="mb-1 block text-xs text-[var(--text-secondary)]">
						<Search class="inline h-3 w-3" /> Search users or groups
					</label>
					<Input
						bind:value={searchQuery}
						oninput={handleSearchInput}
						placeholder="Type a name or email..."
					/>
					{#if searchResults.length > 0}
						<div class="absolute z-10 mt-1 w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] shadow-lg max-h-48 overflow-y-auto">
							{#each searchResults as result}
								<button
									class="flex w-full items-center gap-3 px-3 py-2 text-left text-sm hover:bg-[var(--bg-hover)] transition-colors"
									onclick={() => selectPrincipal(result)}
								>
									<span class="rounded px-1.5 py-0.5 text-[10px] font-medium uppercase {result.principal_type === 'group' ? 'bg-purple-500/20 text-purple-400' : 'bg-sky-500/20 text-sky-400'}">
										{result.principal_type}
									</span>
									<div class="min-w-0 flex-1">
										<p class="truncate text-[var(--text-primary)]">{result.name}</p>
										{#if result.email}
											<p class="truncate text-xs text-[var(--text-tertiary)]">{result.email}</p>
										{/if}
									</div>
								</button>
							{/each}
						</div>
					{/if}
					{#if searchLoading}
						<p class="mt-1 text-xs text-[var(--text-tertiary)]">Searching...</p>
					{/if}
					{#if selectedPrincipal}
						<p class="mt-1 text-xs text-green-500">
							Selected: {selectedPrincipal.name}
							<span class="opacity-60">({selectedPrincipal.principal_type})</span>
						</p>
					{/if}
				</div>
				<div>
					<label class="mb-1 block text-xs text-[var(--text-secondary)]">Role</label>
					<Select options={roleOptions} bind:value={addRole} />
				</div>
			</div>
			<div class="flex justify-end gap-2">
				<Button variant="ghost" size="sm" onclick={() => { showAddForm = false; searchQuery = ''; selectedPrincipal = null; searchResults = []; }}>Cancel</Button>
				<Button variant="primary" size="sm" onclick={handleAdd} loading={addLoading} disabled={!selectedPrincipal}>
					Add
				</Button>
			</div>
		</div>
	{/if}

	{#if loading}
		<p class="py-4 text-center text-sm text-[var(--text-secondary)]">Loading members...</p>
	{:else}
		<div class="rounded-lg border border-[var(--border-primary)] overflow-hidden">
			<table class="w-full text-sm">
				<thead>
					<tr class="border-b border-[var(--border-primary)] bg-[var(--bg-secondary)] text-left text-xs text-[var(--text-secondary)]">
						<th class="px-4 py-2.5 font-medium">Member</th>
						<th class="px-4 py-2.5 font-medium">Type</th>
						<th class="px-4 py-2.5 font-medium">Role</th>
						{#if showInherited}
							<th class="px-4 py-2.5 font-medium">Source</th>
						{/if}
						{#if onRemove || onUpdateRole}
							<th class="px-4 py-2.5 font-medium w-28 text-right">Actions</th>
						{/if}
					</tr>
				</thead>
				<tbody>
					{#each members as member (member.id)}
						<tr class="border-b border-[var(--border-primary)] last:border-0 hover:bg-[var(--bg-hover)]">
							<td class="px-4 py-2.5 text-[var(--text-primary)]">
								{member.display_name || member.principal_id}
							</td>
							<td class="px-4 py-2.5 text-[var(--text-secondary)] capitalize">{member.principal_type}</td>
							<td class="px-4 py-2.5">
								{#if onUpdateRole && !member.inherited}
									<select
										class="rounded border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-2 py-1 text-xs text-[var(--text-primary)]"
										value={member.role}
										onchange={(e) => handleRoleChange(member.principal_id, e.currentTarget.value)}
									>
										{#each roleOptions as opt}
											<option value={opt.value}>{opt.label}</option>
										{/each}
									</select>
								{:else}
									<span class="inline-block rounded-full border px-2 py-0.5 text-xs font-medium {roleBadgeClass(member.role)}">
										{roleDisplayName(member.role)}
									</span>
								{/if}
							</td>
							{#if showInherited}
								<td class="px-4 py-2.5 text-xs text-[var(--text-secondary)]">
									{member.inherited ? 'Inherited' : 'Direct'}
								</td>
							{/if}
							{#if onRemove || onUpdateRole}
								<td class="px-4 py-2.5 text-right">
									{#if !member.inherited && onRemove}
										<button
											class="rounded p-1 text-[var(--text-tertiary)] hover:bg-red-500/10 hover:text-red-400"
											onclick={() => handleRemove(member.principal_id)}
											title="Remove member"
										>
											<Trash2 class="h-3.5 w-3.5" />
										</button>
									{:else if member.inherited}
										<span class="text-xs text-[var(--text-tertiary)]">via project</span>
									{/if}
								</td>
							{/if}
						</tr>
					{:else}
						<tr>
							<td colspan={3 + (showInherited ? 1 : 0) + (onRemove || onUpdateRole ? 1 : 0)} class="px-4 py-8 text-center text-[var(--text-secondary)]">
								<UserCog class="mx-auto mb-2 h-8 w-8 opacity-40" />
								<p>No members yet</p>
							</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
	{/if}
</div>
