<script lang="ts">
	import type {
		Member,
		MemberRole,
		AddMemberInput,
		PrincipalSearchResult,
		AccessControlSaveBatch
	} from '$lib/api/types';
	import { apiMethods } from '$api/client';
	import { Button, Input, Select, Alert } from '$components/ui';
	import Pagination from '$lib/components/data/Pagination.svelte';
	import { Plus, Trash2, UserCog, Search, Save, RotateCcw } from 'lucide-svelte';

	interface Props {
		members: Member[];
		loading?: boolean;
		error?: string | null;
		showInherited?: boolean;
		onSaveAccess: (batch: AccessControlSaveBatch) => Promise<void>;
	}

	let {
		members,
		loading = false,
		error = null,
		showInherited = false,
		onSaveAccess
	}: Props = $props();

	let showAddForm = $state(false);
	let searchQuery = $state('');
	let searchResults = $state<PrincipalSearchResult[]>([]);
	let searchLoading = $state(false);
	let selectedPrincipal = $state<PrincipalSearchResult | null>(null);
	let addRole = $state<MemberRole>('operator');
	let addError = $state<string | null>(null);
	let searchTimeout: ReturnType<typeof setTimeout> | null = null;

	let memberFilter = $state('');
	let listPage = $state(1);
	let listPerPage = $state(20);

	let membersSig = $state('');
	let pendingRemoves = $state<Set<string>>(new Set());
	let pendingRoleChanges = $state<Record<string, MemberRole>>({});
	interface PendingAdd {
		tempId: string;
		input: AddMemberInput;
		displayName: string;
		createdAt: string;
	}
	let pendingAdds = $state<PendingAdd[]>([]);

	let saveLoading = $state(false);
	let saveError = $state<string | null>(null);

	$effect(() => {
		const s = JSON.stringify(
			members.map((m) => [m.id, m.principal_id, m.role, m.inherited === true])
		);
		if (s !== membersSig) {
			membersSig = s;
			pendingRemoves = new Set();
			pendingRoleChanges = {};
			pendingAdds = [];
			memberFilter = '';
			listPage = 1;
			saveError = null;
		}
	});

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

	function canEditRow(m: Member): boolean {
		return m.inherited !== true;
	}

	function effectiveRole(m: Member, pendingId?: string): MemberRole {
		if (pendingId) {
			const pa = pendingAdds.find((p) => p.tempId === pendingId);
			return pa?.input.role ?? 'operator';
		}
		return pendingRoleChanges[m.principal_id] ?? m.role;
	}

	const mergedRows = $derived.by(() => {
		const q = memberFilter.trim().toLowerCase();
		const base: (Member & { pendingId?: string })[] = members
			.filter((m) => !pendingRemoves.has(m.principal_id))
			.map((m) => {
				const r = pendingRoleChanges[m.principal_id];
				return r !== undefined ? { ...m, role: r } : { ...m };
			});
		const pending: (Member & { pendingId?: string })[] = pendingAdds.map((pa) => ({
			id: pa.tempId,
			principal_type: pa.input.principal_type,
			principal_id: pa.input.principal_id,
			role: pa.input.role,
			display_name: pa.displayName,
			created_at: pa.createdAt,
			inherited: undefined,
			pendingId: pa.tempId
		}));
		let rows = [...base, ...pending];
		if (q) {
			rows = rows.filter((m) => {
				const dn = (m.display_name || '').toLowerCase();
				const pid = m.principal_id.toLowerCase();
				const pt = m.principal_type.toLowerCase();
				return dn.includes(q) || pid.includes(q) || pt.includes(q);
			});
		}
		return rows;
	});

	const pagedRows = $derived(
		mergedRows.slice((listPage - 1) * listPerPage, listPage * listPerPage)
	);

	const isDirty = $derived.by(() => {
		if (pendingRemoves.size > 0 || pendingAdds.length > 0) return true;
		for (const [pid, role] of Object.entries(pendingRoleChanges)) {
			const orig = members.find((m) => m.principal_id === pid)?.role;
			if (orig !== undefined && orig !== role) return true;
		}
		return false;
	});

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

	function isAlreadyMember(principalId: string): boolean {
		if (pendingRemoves.has(principalId)) return false;
		const onServer = members.some((m) => m.principal_id === principalId);
		const pending = pendingAdds.some((p) => p.input.principal_id === principalId);
		return onServer || pending;
	}

	function stageAddMember() {
		if (!selectedPrincipal) return;
		addError = null;
		if (isAlreadyMember(selectedPrincipal.id)) {
			addError = 'This user or group is already in the members list.';
			return;
		}
		const tempId = crypto.randomUUID();
		pendingAdds = [
			...pendingAdds,
			{
				tempId,
				displayName: selectedPrincipal.name,
				createdAt: new Date().toISOString(),
				input: {
					principal_type: selectedPrincipal.principal_type,
					principal_id: selectedPrincipal.id,
					role: addRole
				}
			}
		];
		searchQuery = '';
		selectedPrincipal = null;
		addRole = 'operator';
		showAddForm = false;
		listPage = Math.ceil(mergedRows.length / listPerPage) || 1;
	}

	function setRoleForRow(m: Member & { pendingId?: string }, newRole: MemberRole) {
		if (m.pendingId) {
			pendingAdds = pendingAdds.map((p) =>
				p.tempId === m.pendingId ? { ...p, input: { ...p.input, role: newRole } } : p
			);
			return;
		}
		const orig = members.find((x) => x.principal_id === m.principal_id)?.role;
		if (orig === newRole) {
			const { [m.principal_id]: _, ...rest } = pendingRoleChanges;
			pendingRoleChanges = rest;
		} else {
			pendingRoleChanges = { ...pendingRoleChanges, [m.principal_id]: newRole };
		}
	}

	function removeRow(m: Member & { pendingId?: string }) {
		if (m.pendingId) {
			pendingAdds = pendingAdds.filter((p) => p.tempId !== m.pendingId);
			return;
		}
		if (m.inherited) return;
		pendingRemoves = new Set([...pendingRemoves, m.principal_id]);
		const { [m.principal_id]: _, ...rest } = pendingRoleChanges;
		pendingRoleChanges = rest;
	}

	function discardChanges() {
		pendingRemoves = new Set();
		pendingRoleChanges = {};
		pendingAdds = [];
		saveError = null;
		memberFilter = '';
		listPage = 1;
	}

	async function saveChanges() {
		saveError = null;
		const roleUpdates: { principalId: string; role: MemberRole }[] = [];
		for (const [pid, role] of Object.entries(pendingRoleChanges)) {
			const orig = members.find((m) => m.principal_id === pid)?.role;
			const m = members.find((x) => x.principal_id === pid);
			if (m?.inherited) continue;
			if (orig !== undefined && orig !== role) {
				roleUpdates.push({ principalId: pid, role });
			}
		}
		const batch: AccessControlSaveBatch = {
			removePrincipalIds: [...pendingRemoves],
			roleUpdates,
			adds: pendingAdds.map((p) => p.input)
		};
		saveLoading = true;
		try {
			await onSaveAccess(batch);
			pendingRemoves = new Set();
			pendingRoleChanges = {};
			pendingAdds = [];
		} catch (e) {
			saveError = e instanceof Error ? e.message : 'Failed to save access changes';
		} finally {
			saveLoading = false;
		}
	}

	$effect(() => {
		const n = mergedRows.length;
		const maxPage = Math.max(1, Math.ceil(n / listPerPage) || 1);
		if (listPage > maxPage) listPage = maxPage;
	});
</script>

<div class="space-y-4">
	{#if error}
		<Alert variant="error">{error}</Alert>
	{/if}
	{#if saveError}
		<Alert variant="error">{saveError}</Alert>
	{/if}

	<div class="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
		<div>
			<h3 class="text-base font-medium text-[var(--text-primary)]">Members</h3>
			<p class="text-xs text-[var(--text-secondary)]">
				Users and groups with access to this resource. Changes apply when you click Save changes.
			</p>
		</div>
		<div class="flex flex-wrap items-center gap-2">
			{#if isDirty}
				<Button variant="ghost" size="sm" onclick={discardChanges} disabled={saveLoading}>
					<RotateCcw class="h-4 w-4" />
					Discard
				</Button>
				<Button variant="primary" size="sm" onclick={saveChanges} loading={saveLoading}>
					<Save class="h-4 w-4" />
					Save changes
				</Button>
			{/if}
			<Button variant="outline" size="sm" onclick={() => (showAddForm = !showAddForm)}>
				<Plus class="h-4 w-4" />
				Add member
			</Button>
		</div>
	</div>

	{#if showAddForm}
		<div class="rounded-lg border border-[var(--border-primary)] bg-[var(--bg-tertiary)] p-4 space-y-3">
			{#if addError}
				<Alert variant="error">{addError}</Alert>
			{/if}
			<div class="grid gap-3 sm:grid-cols-3">
				<div class="sm:col-span-2 relative">
					<label class="mb-1 block text-xs text-[var(--text-secondary)]">
						<Search class="inline h-3 w-3" /> Search users or groups to add
					</label>
					<Input
						bind:value={searchQuery}
						oninput={handleSearchInput}
						placeholder="Type a name or email..."
					/>
					{#if searchResults.length > 0}
						<div
							class="absolute z-10 mt-1 w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] shadow-lg max-h-48 overflow-y-auto"
						>
							{#each searchResults as result}
								<button
									type="button"
									class="flex w-full items-center gap-3 px-3 py-2 text-left text-sm hover:bg-[var(--bg-hover)] transition-colors"
									onclick={() => selectPrincipal(result)}
								>
									<span
										class="rounded px-1.5 py-0.5 text-[10px] font-medium uppercase {result.principal_type ===
										'group'
											? 'bg-purple-500/20 text-purple-400'
											: 'bg-sky-500/20 text-sky-400'}"
									>
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
					<label for="access-add-role" class="mb-1 block text-xs text-[var(--text-secondary)]">Role</label>
					<Select id="access-add-role" options={roleOptions} bind:value={addRole} />
				</div>
			</div>
			<div class="flex justify-end gap-2">
				<Button
					variant="ghost"
					size="sm"
					onclick={() => {
						showAddForm = false;
						searchQuery = '';
						selectedPrincipal = null;
						searchResults = [];
						addError = null;
					}}>Cancel</Button
				>
				<Button
					variant="primary"
					size="sm"
					onclick={stageAddMember}
					disabled={!selectedPrincipal}
				>
					Add to list
				</Button>
			</div>
		</div>
	{/if}

	<div>
				<label for="access-member-filter" class="mb-1 block text-xs text-[var(--text-secondary)]">
			<Search class="inline h-3 w-3" /> Filter current members
		</label>
		<Input
			id="access-member-filter"
			bind:value={memberFilter}
			placeholder="Search by name, id, or type…"
			class="max-w-md"
		/>
	</div>

	{#if loading}
		<p class="py-4 text-center text-sm text-[var(--text-secondary)]">Loading members...</p>
	{:else}
		<div class="rounded-lg border border-[var(--border-primary)] overflow-hidden">
			<table class="w-full text-sm">
				<thead>
					<tr
						class="border-b border-[var(--border-primary)] bg-[var(--bg-secondary)] text-left text-xs text-[var(--text-secondary)]"
					>
						<th class="px-4 py-2.5 font-medium">Member</th>
						<th class="px-4 py-2.5 font-medium">Type</th>
						<th class="px-4 py-2.5 font-medium">Role</th>
						{#if showInherited}
							<th class="px-4 py-2.5 font-medium">Source</th>
						{/if}
						<th class="px-4 py-2.5 font-medium w-28 text-right">Actions</th>
					</tr>
				</thead>
				<tbody>
					{#each pagedRows as member (member.pendingId ?? member.id)}
						<tr class="border-b border-[var(--border-primary)] last:border-0 hover:bg-[var(--bg-hover)]">
							<td class="px-4 py-2.5 text-[var(--text-primary)]">
								{member.display_name || member.principal_id}
								{#if member.pendingId}
									<span class="ml-1 text-[10px] uppercase text-amber-500/90">pending</span>
								{/if}
							</td>
							<td class="px-4 py-2.5 text-[var(--text-secondary)] capitalize"
								>{member.principal_type}</td
							>
							<td class="px-4 py-2.5">
								{#if canEditRow(member)}
									<select
										class="rounded border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-2 py-1 text-xs text-[var(--text-primary)]"
										value={effectiveRole(member, member.pendingId)}
										onchange={(e) =>
											setRoleForRow(
												member,
												e.currentTarget.value as MemberRole
											)}
									>
										{#each roleOptions as opt}
											<option value={opt.value}>{opt.label}</option>
										{/each}
									</select>
								{:else}
									<span
										class="inline-block rounded-full border px-2 py-0.5 text-xs font-medium {roleBadgeClass(
											effectiveRole(member, member.pendingId)
										)}"
									>
										{roleDisplayName(effectiveRole(member, member.pendingId))}
									</span>
								{/if}
							</td>
							{#if showInherited}
								<td class="px-4 py-2.5 text-xs text-[var(--text-secondary)]">
									{member.inherited ? 'Inherited' : member.pendingId ? 'Pending add' : 'Direct'}
								</td>
							{/if}
							<td class="px-4 py-2.5 text-right">
								{#if member.pendingId}
									<button
										type="button"
										class="rounded p-1 text-[var(--text-tertiary)] hover:bg-red-500/10 hover:text-red-400"
										onclick={() => removeRow(member)}
										title="Remove pending add"
									>
										<Trash2 class="h-3.5 w-3.5" />
									</button>
								{:else if canEditRow(member)}
									<button
										type="button"
										class="rounded p-1 text-[var(--text-tertiary)] hover:bg-red-500/10 hover:text-red-400"
										onclick={() => removeRow(member)}
										title="Remove member"
									>
										<Trash2 class="h-3.5 w-3.5" />
									</button>
								{:else}
									<span class="text-xs text-[var(--text-tertiary)]">via project</span>
								{/if}
							</td>
						</tr>
					{:else}
						<tr>
							<td
								colspan={3 + (showInherited ? 1 : 0) + 1}
								class="px-4 py-8 text-center text-[var(--text-secondary)]"
							>
								<UserCog class="mx-auto mb-2 h-8 w-8 opacity-40" />
								<p>
									{#if members.length === 0 && pendingAdds.length === 0 && !memberFilter.trim()}
										No members yet
									{:else}
										No members match this filter
									{/if}
								</p>
							</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>

		{#if mergedRows.length > 0}
			<div class="mt-4">
				<Pagination bind:page={listPage} bind:perPage={listPerPage} total={mergedRows.length} />
			</div>
		{/if}
	{/if}
</div>
