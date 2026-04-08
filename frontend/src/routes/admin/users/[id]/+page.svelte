<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { 
		Mail, Calendar, Clock, Shield, ShieldCheck, Lock, Unlock, 
		Trash2, Key, ArrowLeft, Users, Plus, X, Check, AlertTriangle
	} from 'lucide-svelte';
	import { apiMethods } from '$lib/api';
	import type { AdminUser, UserRoleAssignment, RoleInfo, GroupMember } from '$lib/api/types';
	import { getGravatarUrl } from '$lib/utils/gravatar';
	import { auth } from '$stores';

	let { data } = $props();

	let user = $state<AdminUser | null>(null);
	let roles = $state<UserRoleAssignment[]>([]);
	let availableRoles = $state<RoleInfo[]>([]);
	let groupMemberships = $state<{ group_id: string; group_name: string; role: string }[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);

	let showRoleModal = $state(false);
	let selectedRole = $state<string>('');
	let roleLoading = $state(false);
	let roleError = $state<string | null>(null);

	let showResetPasswordModal = $state(false);
	let newPassword = $state('');
	let resetPasswordLoading = $state(false);
	let resetPasswordError = $state<string | null>(null);

	// Confirmation modal state
	let showConfirmModal = $state(false);
	let confirmTitle = $state('');
	let confirmMessage = $state('');
	let confirmButtonText = $state('Confirm');
	let confirmButtonClass = $state('bg-primary-600 hover:bg-primary-700');
	let confirmAction = $state<(() => Promise<void>) | null>(null);
	let confirmLoading = $state(false);

	let showServiceAccountTokenModal = $state(false);
	let saTokenCreating = $state(false);
	let saTokenError = $state<string | null>(null);
	let saTokenPlain = $state<string | null>(null);
	let saTokenForm = $state({
		name: '',
		description: '',
		scopes: ['read'] as string[],
		expiresIn: '365' as string,
		projectIdsRaw: '',
		pipelineIdsRaw: ''
	});

	function openConfirmModal(options: {
		title: string;
		message: string;
		buttonText?: string;
		danger?: boolean;
		onConfirm: () => Promise<void>;
	}) {
		confirmTitle = options.title;
		confirmMessage = options.message;
		confirmButtonText = options.buttonText || 'Confirm';
		confirmButtonClass = options.danger 
			? 'bg-red-600 hover:bg-red-700' 
			: 'bg-primary-600 hover:bg-primary-700';
		confirmAction = options.onConfirm;
		showConfirmModal = true;
	}

	async function executeConfirm() {
		if (!confirmAction) return;
		confirmLoading = true;
		try {
			await confirmAction();
			showConfirmModal = false;
		} catch (e) {
			alert(e instanceof Error ? e.message : 'Action failed');
		} finally {
			confirmLoading = false;
		}
	}

	onMount(async () => {
		await loadUser();
		await loadRoles();
		await loadAvailableRoles();
		await loadGroupMemberships();
	});

	async function loadUser() {
		loading = true;
		error = null;
		try {
			user = await apiMethods.admin.users.get(data.userId);
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load user';
		} finally {
			loading = false;
		}
	}

	async function loadRoles() {
		try {
			roles = await apiMethods.admin.roles.getUserRoles(data.userId);
		} catch (e) {
			console.error('Failed to load user roles:', e);
		}
	}

	async function loadAvailableRoles() {
		try {
			availableRoles = await apiMethods.admin.roles.list();
		} catch (e) {
			console.error('Failed to load available roles:', e);
		}
	}

	async function loadGroupMemberships() {
		try {
			const groupsResponse = await apiMethods.admin.groups.list({ limit: 100 });
			const memberships: { group_id: string; group_name: string; role: string }[] = [];
			
			for (const group of groupsResponse.data) {
				try {
					const members = await apiMethods.admin.groups.listMembers(group.id);
					const membership = members.find(m => m.user_id === data.userId);
					if (membership) {
						memberships.push({
							group_id: group.id,
							group_name: group.name,
							role: membership.role
						});
					}
				} catch {
					// Skip groups we can't access
				}
			}
			
			groupMemberships = memberships;
		} catch (e) {
			console.error('Failed to load group memberships:', e);
		}
	}

	function toggleAdmin() {
		if (!user) return;
		
		const isCurrentlyAdmin = user.is_admin;
		const userName = user.display_name || user.username;
		
		openConfirmModal({
			title: isCurrentlyAdmin ? 'Remove Admin Privileges' : 'Grant Admin Privileges',
			message: isCurrentlyAdmin 
				? `Are you sure you want to remove admin privileges from ${userName}? They will no longer be able to access admin features.`
				: `Are you sure you want to grant admin privileges to ${userName}? They will have full access to all admin features.`,
			buttonText: isCurrentlyAdmin ? 'Remove Admin' : 'Make Admin',
			danger: isCurrentlyAdmin,
			onConfirm: async () => {
				user = await apiMethods.admin.users.update(user!.id, { is_admin: !isCurrentlyAdmin });
			}
		});
	}

	function requestLockAccount() {
		if (!user || isSelf) return;
		const userName = user.display_name || user.username;
		openConfirmModal({
			title: 'Are you sure?',
			message: `Lock account for ${userName}? They will not be able to sign in until an admin unlocks the account.`,
			buttonText: 'Lock account',
			danger: true,
			onConfirm: async () => {
				user = await apiMethods.admin.users.lock(user!.id);
			}
		});
	}

	async function unlockUser() {
		if (!user) return;
		try {
			user = await apiMethods.admin.users.unlock(user.id);
		} catch (e) {
			alert(e instanceof Error ? e.message : 'Failed to unlock user');
		}
	}

	function deleteUser() {
		if (!user) return;
		const userName = user.display_name || user.username;
		
		openConfirmModal({
			title: 'Delete User',
			message: `Are you sure you want to delete ${userName}? This action cannot be undone and will remove all their data.`,
			buttonText: 'Delete User',
			danger: true,
			onConfirm: async () => {
				await apiMethods.admin.users.delete(user!.id);
				goto('/admin/users');
			}
		});
	}

	function openRoleModal() {
		selectedRole = '';
		roleError = null;
		showRoleModal = true;
	}

	async function assignRole() {
		if (!selectedRole) {
			roleError = 'Please select a role';
			return;
		}
		roleLoading = true;
		roleError = null;
		try {
			await apiMethods.admin.roles.assign(data.userId, selectedRole);
			await loadRoles();
			showRoleModal = false;
		} catch (e) {
			roleError = e instanceof Error ? e.message : 'Failed to assign role';
		} finally {
			roleLoading = false;
		}
	}

	function revokeRole(role: string) {
		openConfirmModal({
			title: 'Remove Role',
			message: `Are you sure you want to remove the "${role}" role from this user?`,
			buttonText: 'Remove Role',
			danger: true,
			onConfirm: async () => {
				await apiMethods.admin.roles.revoke(data.userId, role);
				await loadRoles();
			}
		});
	}

	async function resetPassword() {
		if (!newPassword || newPassword.length < 8) {
			resetPasswordError = 'Password must be at least 8 characters';
			return;
		}
		resetPasswordLoading = true;
		resetPasswordError = null;
		try {
			await apiMethods.admin.users.resetPassword(data.userId, newPassword);
			showResetPasswordModal = false;
			newPassword = '';
		} catch (e) {
			resetPasswordError = e instanceof Error ? e.message : 'Failed to reset password';
		} finally {
			resetPasswordLoading = false;
		}
	}

	const unassignedRoles = $derived(
		availableRoles.filter(r => !roles.some(ur => ur.role === r.name))
	);

	const isSelf = $derived(!!user && !!auth.user && user.id === auth.user.id);

	const saScopeOptions = [
		{ value: 'read', label: 'Read' },
		{ value: 'write', label: 'Write' },
		{ value: 'admin', label: 'Admin' }
	];

	function toggleSaScope(scope: string) {
		if (saTokenForm.scopes.includes(scope)) {
			saTokenForm.scopes = saTokenForm.scopes.filter((s) => s !== scope);
		} else {
			saTokenForm.scopes = [...saTokenForm.scopes, scope];
		}
	}

	function parseUuidList(raw: string): string[] {
		return raw
			.split(/[\s,]+/)
			.map((s) => s.trim())
			.filter(Boolean);
	}

	async function submitServiceAccountToken() {
		if (!user?.service_account || !user || !saTokenForm.name.trim() || saTokenForm.scopes.length === 0) return;
		saTokenCreating = true;
		saTokenError = null;
		try {
			const project_ids_raw = parseUuidList(saTokenForm.projectIdsRaw);
			const pipeline_ids_raw = parseUuidList(saTokenForm.pipelineIdsRaw);
			const expires_in_days =
				saTokenForm.expiresIn === 'never' ? null : parseInt(saTokenForm.expiresIn, 10);
			const res = await apiMethods.admin.users.createToken(user.id, {
				name: saTokenForm.name.trim(),
				description: saTokenForm.description.trim() || undefined,
				scopes: saTokenForm.scopes,
				expires_in_days,
				...(project_ids_raw.length > 0 ? { project_ids: project_ids_raw } : {}),
				...(pipeline_ids_raw.length > 0 ? { pipeline_ids: pipeline_ids_raw } : {})
			});
			saTokenPlain = res.plain_token;
			saTokenForm = {
				name: '',
				description: '',
				scopes: ['read'],
				expiresIn: '365',
				projectIdsRaw: '',
				pipelineIdsRaw: ''
			};
		} catch (e) {
			saTokenError = e instanceof Error ? e.message : 'Failed to create token';
		} finally {
			saTokenCreating = false;
		}
	}

	function closeServiceAccountTokenModal() {
		showServiceAccountTokenModal = false;
		saTokenError = null;
		saTokenPlain = null;
	}
</script>

<div class="space-y-6">
	<div class="flex items-center gap-4">
		<a
			href="/admin/users"
			class="rounded-lg p-2 text-[var(--text-secondary)] hover:bg-[var(--bg-secondary)] hover:text-[var(--text-primary)]"
		>
			<ArrowLeft class="h-5 w-5" />
		</a>
		<h2 class="text-lg font-semibold text-[var(--text-primary)]">User Profile</h2>
	</div>

	{#if loading}
		<div class="flex items-center justify-center py-12">
			<div class="h-8 w-8 animate-spin rounded-full border-2 border-primary-500 border-t-transparent"></div>
		</div>
	{:else if error}
		<div class="rounded-lg border border-red-200 bg-red-50 p-4 text-sm text-red-700 dark:border-red-900 dark:bg-red-950/50 dark:text-red-400">
			{error}
		</div>
	{:else if user}
		<div class="grid gap-6 lg:grid-cols-3">
			<!-- User Info Card -->
			<div class="lg:col-span-1">
				<div class="rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] p-6">
					<div class="flex flex-col items-center text-center">
						<img
							src={getGravatarUrl(user.email, { size: 120 })}
							alt={user.display_name || user.username}
							class="h-24 w-24 rounded-full border-4 border-[var(--bg-primary)]"
						/>
						<h3 class="mt-4 text-xl font-semibold text-[var(--text-primary)]">
							{user.display_name || user.username}
						</h3>
						<p class="text-sm text-[var(--text-secondary)]">@{user.username}</p>
						
						<div class="mt-4 flex flex-wrap justify-center gap-2">
							{#if user.is_admin}
								<span class="inline-flex items-center gap-1 rounded-full bg-primary-100 px-2.5 py-1 text-xs font-medium text-primary-700 dark:bg-primary-900/30 dark:text-primary-400">
									<ShieldCheck class="h-3 w-3" />
									Admin
								</span>
							{/if}
							{#if user.is_active}
								<span class="inline-flex items-center gap-1 rounded-full bg-green-100 px-2.5 py-1 text-xs font-medium text-green-700 dark:bg-green-900/30 dark:text-green-400">
									<Check class="h-3 w-3" />
									Active
								</span>
							{:else}
								<span class="inline-flex items-center gap-1 rounded-full bg-gray-100 px-2.5 py-1 text-xs font-medium text-gray-700 dark:bg-gray-800 dark:text-gray-300">
									<Lock class="h-3 w-3" />
									Locked
								</span>
							{/if}
						</div>
					</div>

					<div class="mt-6 space-y-3 border-t border-[var(--border-primary)] pt-6">
						<div class="flex items-center gap-3 text-sm">
							<Mail class="h-4 w-4 text-[var(--text-tertiary)]" />
							<span class="text-[var(--text-primary)]">{user.email}</span>
						</div>
						<div class="flex items-center gap-3 text-sm">
							<Calendar class="h-4 w-4 text-[var(--text-tertiary)]" />
							<span class="text-[var(--text-secondary)]">Joined {new Date(user.created_at).toLocaleDateString()}</span>
						</div>
						<div class="flex items-center gap-3 text-sm">
							<Clock class="h-4 w-4 shrink-0 text-[var(--text-tertiary)]" />
							<span class="text-[var(--text-secondary)]">
								Last login:
								{#if user.last_login_at}
									{' '}
									{new Date(user.last_login_at).toLocaleString(undefined, {
										dateStyle: 'medium',
										timeStyle: 'short'
									})}
								{:else}
									<span class="text-[var(--text-tertiary)]"> — never</span>
								{/if}
							</span>
						</div>
					</div>

					<div class="mt-6 space-y-2 border-t border-[var(--border-primary)] pt-6">
						<button
							type="button"
							onclick={toggleAdmin}
							class="flex w-full items-center gap-2 rounded-lg px-3 py-2 text-sm text-[var(--text-primary)] hover:bg-[var(--bg-primary)]"
						>
							{#if user.is_admin}
								<Shield class="h-4 w-4" />
								Remove Admin
							{:else}
								<ShieldCheck class="h-4 w-4" />
								Make Admin
							{/if}
						</button>
						{#if user.is_active}
							{#if !isSelf}
								<button
									type="button"
									onclick={requestLockAccount}
									class="flex w-full items-center gap-2 rounded-lg px-3 py-2 text-sm text-[var(--text-primary)] hover:bg-[var(--bg-primary)]"
								>
									<Lock class="h-4 w-4" />
									Lock Account
								</button>
							{/if}
						{:else}
							<button
								type="button"
								onclick={unlockUser}
								class="flex w-full items-center gap-2 rounded-lg px-3 py-2 text-sm text-[var(--text-primary)] hover:bg-[var(--bg-primary)]"
							>
								<Unlock class="h-4 w-4" />
								Unlock Account
							</button>
						{/if}
						<button
							type="button"
							onclick={() => showResetPasswordModal = true}
							class="flex w-full items-center gap-2 rounded-lg px-3 py-2 text-sm text-[var(--text-primary)] hover:bg-[var(--bg-primary)]"
						>
							<Key class="h-4 w-4" />
							Reset Password
						</button>
						<button
							type="button"
							onclick={deleteUser}
							class="flex w-full items-center gap-2 rounded-lg px-3 py-2 text-sm text-red-600 hover:bg-red-50 dark:text-red-400 dark:hover:bg-red-900/20"
						>
							<Trash2 class="h-4 w-4" />
							Delete User
						</button>
					</div>
				</div>
			</div>

			<!-- Roles and Groups -->
			<div class="space-y-6 lg:col-span-2">
				<!-- Roles Section -->
				<div class="rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] p-6">
					<div class="flex items-center justify-between">
						<h3 class="text-base font-semibold text-[var(--text-primary)]">Roles</h3>
						{#if unassignedRoles.length > 0}
							<button
								type="button"
								onclick={openRoleModal}
								class="inline-flex items-center gap-1.5 rounded-lg bg-primary-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-primary-700"
							>
								<Plus class="h-4 w-4" />
								Add Role
							</button>
						{/if}
					</div>

					<div class="mt-4">
						{#if roles.length === 0}
							<p class="text-sm text-[var(--text-secondary)]">No roles assigned yet.</p>
						{:else}
							<div class="space-y-2">
								{#each roles as role (role.role)}
									<div class="flex items-center justify-between rounded-lg bg-[var(--bg-primary)] px-4 py-3">
										<div>
											<span class="font-medium text-[var(--text-primary)] capitalize">{role.role}</span>
											<p class="text-xs text-[var(--text-secondary)]">
												Granted {new Date(role.granted_at).toLocaleDateString()}
											</p>
										</div>
										<button
											type="button"
											onclick={() => revokeRole(role.role)}
											class="rounded p-1 text-[var(--text-secondary)] hover:bg-[var(--bg-secondary)] hover:text-red-600"
										>
											<X class="h-4 w-4" />
										</button>
									</div>
								{/each}
							</div>
						{/if}
					</div>
				</div>

				{#if user.service_account}
					<div class="rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] p-6">
						<div class="flex flex-wrap items-center justify-between gap-3">
							<div>
								<h3 class="text-base font-semibold text-[var(--text-primary)]">Service account API tokens</h3>
								<p class="mt-1 text-sm text-[var(--text-secondary)]">
									Create a token on behalf of this service account. The plain value is shown only once.
									Service accounts are not subject to the two-active-token cap when provisioned by an admin.
								</p>
							</div>
							<button
								type="button"
								onclick={() => {
									saTokenPlain = null;
									saTokenError = null;
									showServiceAccountTokenModal = true;
								}}
								class="inline-flex items-center gap-2 rounded-lg bg-primary-600 px-3 py-2 text-sm font-medium text-white hover:bg-primary-700"
							>
								<Key class="h-4 w-4" />
								Create token
							</button>
						</div>
					</div>
				{/if}

				<!-- Groups Section -->
				<div class="rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] p-6">
					<h3 class="text-base font-semibold text-[var(--text-primary)]">Group Memberships</h3>

					<div class="mt-4">
						{#if groupMemberships.length === 0}
							<p class="text-sm text-[var(--text-secondary)]">Not a member of any groups.</p>
						{:else}
							<div class="space-y-2">
								{#each groupMemberships as membership (membership.group_id)}
									<a
										href="/admin/groups"
										class="flex items-center justify-between rounded-lg bg-[var(--bg-primary)] px-4 py-3 hover:bg-[var(--bg-hover)]"
									>
										<div class="flex items-center gap-3">
											<div class="flex h-8 w-8 items-center justify-center rounded-lg bg-[var(--bg-secondary)]">
												<Users class="h-4 w-4 text-[var(--text-secondary)]" />
											</div>
											<span class="font-medium text-[var(--text-primary)]">{membership.group_name}</span>
										</div>
										<span class="rounded-full bg-[var(--bg-secondary)] px-2 py-0.5 text-xs text-[var(--text-secondary)] capitalize">
											{membership.role}
										</span>
									</a>
								{/each}
							</div>
						{/if}
					</div>
				</div>
			</div>
		</div>
	{/if}
</div>

<!-- Add Role Modal -->
{#if showRoleModal}
	<div class="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
		<div class="w-full max-w-md rounded-lg bg-[var(--bg-primary)] p-6 shadow-xl">
			<div class="flex items-center justify-between">
				<h3 class="text-lg font-semibold text-[var(--text-primary)]">Add Role</h3>
				<button
					type="button"
					onclick={() => showRoleModal = false}
					class="rounded p-1 text-[var(--text-secondary)] hover:bg-[var(--bg-secondary)]"
				>
					<X class="h-5 w-5" />
				</button>
			</div>
			{#if roleError}
				<div class="mt-4 rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-700 dark:border-red-900 dark:bg-red-950/50 dark:text-red-400">
					{roleError}
				</div>
			{/if}
			<div class="mt-4">
				<label for="role-select" class="block text-sm font-medium text-[var(--text-primary)]">Role</label>
				<select
					id="role-select"
					bind:value={selectedRole}
					class="mt-1 w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm text-[var(--text-primary)] focus:border-primary-500 focus:outline-none focus:ring-1 focus:ring-primary-500"
				>
					<option value="">Select a role...</option>
					{#each unassignedRoles as role (role.name)}
						<option value={role.name}>{role.name} - {role.description}</option>
					{/each}
				</select>
			</div>
			<div class="mt-6 flex justify-end gap-3">
				<button
					type="button"
					onclick={() => showRoleModal = false}
					class="rounded-lg border border-[var(--border-primary)] px-4 py-2 text-sm font-medium text-[var(--text-primary)] hover:bg-[var(--bg-secondary)]"
				>
					Cancel
				</button>
				<button
					type="button"
					onclick={assignRole}
					disabled={roleLoading || !selectedRole}
					class="rounded-lg bg-primary-600 px-4 py-2 text-sm font-medium text-white hover:bg-primary-700 disabled:opacity-50"
				>
					{roleLoading ? 'Adding...' : 'Add Role'}
				</button>
			</div>
		</div>
	</div>
{/if}

<!-- Reset Password Modal -->
{#if showResetPasswordModal}
	<div class="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
		<div class="w-full max-w-md rounded-lg bg-[var(--bg-primary)] p-6 shadow-xl">
			<div class="flex items-center justify-between">
				<h3 class="text-lg font-semibold text-[var(--text-primary)]">Reset Password</h3>
				<button
					type="button"
					onclick={() => showResetPasswordModal = false}
					class="rounded p-1 text-[var(--text-secondary)] hover:bg-[var(--bg-secondary)]"
				>
					<X class="h-5 w-5" />
				</button>
			</div>
			<p class="mt-2 text-sm text-[var(--text-secondary)]">
				Set a new password for <strong>{user?.display_name || user?.username}</strong>
			</p>
			{#if resetPasswordError}
				<div class="mt-4 rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-700 dark:border-red-900 dark:bg-red-950/50 dark:text-red-400">
					{resetPasswordError}
				</div>
			{/if}
			<div class="mt-4">
				<label for="new-password" class="block text-sm font-medium text-[var(--text-primary)]">New Password</label>
				<input
					type="password"
					id="new-password"
					bind:value={newPassword}
					placeholder="Enter new password"
					class="mt-1 w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm text-[var(--text-primary)] focus:border-primary-500 focus:outline-none focus:ring-1 focus:ring-primary-500"
				/>
				<p class="mt-1 text-xs text-[var(--text-tertiary)]">Minimum 8 characters</p>
			</div>
			<div class="mt-6 flex justify-end gap-3">
				<button
					type="button"
					onclick={() => showResetPasswordModal = false}
					class="rounded-lg border border-[var(--border-primary)] px-4 py-2 text-sm font-medium text-[var(--text-primary)] hover:bg-[var(--bg-secondary)]"
				>
					Cancel
				</button>
				<button
					type="button"
					onclick={resetPassword}
					disabled={resetPasswordLoading || !newPassword}
					class="rounded-lg bg-primary-600 px-4 py-2 text-sm font-medium text-white hover:bg-primary-700 disabled:opacity-50"
				>
					{resetPasswordLoading ? 'Resetting...' : 'Reset Password'}
				</button>
			</div>
		</div>
	</div>
{/if}

<!-- Service account token -->
{#if showServiceAccountTokenModal}
	<div class="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4">
		<div class="max-h-[90vh] w-full max-w-lg overflow-y-auto rounded-lg bg-[var(--bg-primary)] p-6 shadow-xl">
			<div class="flex items-center justify-between gap-2">
				<h3 class="text-lg font-semibold text-[var(--text-primary)]">Create API token</h3>
				<button
					type="button"
					onclick={closeServiceAccountTokenModal}
					class="rounded p-1 text-[var(--text-secondary)] hover:bg-[var(--bg-secondary)]"
				>
					<X class="h-5 w-5" />
				</button>
			</div>
			{#if saTokenPlain}
				<p class="mt-3 text-sm text-amber-700 dark:text-amber-400">
					Copy this token now; you will not be able to see it again.
				</p>
				<div class="mt-3 break-all rounded-lg bg-[var(--bg-tertiary)] p-3 font-mono text-sm text-[var(--text-primary)]">
					{saTokenPlain}
				</div>
				<div class="mt-4 flex justify-end">
					<button
						type="button"
						onclick={closeServiceAccountTokenModal}
						class="rounded-lg bg-primary-600 px-4 py-2 text-sm font-medium text-white hover:bg-primary-700"
					>
						Done
					</button>
				</div>
			{:else}
				<p class="mt-2 text-sm text-[var(--text-secondary)]">
					Token is created for <strong>{user?.display_name || user?.username}</strong>
				</p>
				{#if saTokenError}
					<div class="mt-4 rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-700 dark:border-red-900 dark:bg-red-950/50 dark:text-red-400">
						{saTokenError}
					</div>
				{/if}
				<div class="mt-4 space-y-4">
					<div>
						<label for="sa-tok-name" class="block text-sm font-medium text-[var(--text-primary)]">Name</label>
						<input
							id="sa-tok-name"
							bind:value={saTokenForm.name}
							class="mt-1 w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm"
						/>
					</div>
					<div>
						<label for="sa-tok-desc" class="block text-sm font-medium text-[var(--text-primary)]">Description (optional)</label>
						<input
							id="sa-tok-desc"
							bind:value={saTokenForm.description}
							class="mt-1 w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm"
						/>
					</div>
					<div>
						<span class="block text-sm font-medium text-[var(--text-primary)]">Scopes</span>
						<div class="mt-2 space-y-2">
							{#each saScopeOptions as opt (opt.value)}
								<label class="flex items-center gap-2 text-sm">
									<input
										type="checkbox"
										class="h-4 w-4 rounded border-secondary-300"
										checked={saTokenForm.scopes.includes(opt.value)}
										onchange={() => toggleSaScope(opt.value)}
									/>
									{opt.label}
								</label>
							{/each}
						</div>
					</div>
					<div>
						<label for="sa-tok-exp" class="block text-sm font-medium text-[var(--text-primary)]">Expiration</label>
						<select
							id="sa-tok-exp"
							bind:value={saTokenForm.expiresIn}
							class="mt-1 w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm"
						>
							<option value="30">30 days</option>
							<option value="90">90 days</option>
							<option value="365">1 year</option>
							<option value="never">Never</option>
						</select>
					</div>
					<div>
						<label for="sa-tok-proj" class="block text-sm font-medium text-[var(--text-primary)]"
							>Project IDs (optional)</label
						>
						<p class="mt-0.5 text-xs text-[var(--text-tertiary)]">Comma or space separated UUIDs. Empty = all projects.</p>
						<textarea
							id="sa-tok-proj"
							rows="2"
							bind:value={saTokenForm.projectIdsRaw}
							class="mt-1 w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 font-mono text-xs"
						></textarea>
					</div>
					<div>
						<label for="sa-tok-pipe" class="block text-sm font-medium text-[var(--text-primary)]"
							>Pipeline IDs (optional)</label
						>
						<p class="mt-0.5 text-xs text-[var(--text-tertiary)]">
							Further restrict to pipelines. Empty = all pipelines in allowed projects.
						</p>
						<textarea
							id="sa-tok-pipe"
							rows="2"
							bind:value={saTokenForm.pipelineIdsRaw}
							class="mt-1 w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 font-mono text-xs"
						></textarea>
					</div>
				</div>
				<div class="mt-6 flex justify-end gap-3">
					<button
						type="button"
						onclick={closeServiceAccountTokenModal}
						disabled={saTokenCreating}
						class="rounded-lg border border-[var(--border-primary)] px-4 py-2 text-sm font-medium hover:bg-[var(--bg-secondary)] disabled:opacity-50"
					>
						Cancel
					</button>
					<button
						type="button"
						onclick={submitServiceAccountToken}
						disabled={saTokenCreating || !saTokenForm.name.trim() || saTokenForm.scopes.length === 0}
						class="rounded-lg bg-primary-600 px-4 py-2 text-sm font-medium text-white hover:bg-primary-700 disabled:opacity-50"
					>
						{saTokenCreating ? 'Creating…' : 'Create token'}
					</button>
				</div>
			{/if}
		</div>
	</div>
{/if}

<!-- Confirmation Modal -->
{#if showConfirmModal}
	<div class="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
		<div class="w-full max-w-md rounded-lg bg-[var(--bg-primary)] p-6 shadow-xl">
			<div class="flex items-start gap-4">
				<div class="flex h-10 w-10 flex-shrink-0 items-center justify-center rounded-full bg-red-100 dark:bg-red-900/30">
					<AlertTriangle class="h-5 w-5 text-red-600 dark:text-red-400" />
				</div>
				<div class="flex-1">
					<h3 class="text-lg font-semibold text-[var(--text-primary)]">{confirmTitle}</h3>
					<p class="mt-2 text-sm text-[var(--text-secondary)]">{confirmMessage}</p>
				</div>
			</div>
			<div class="mt-6 flex justify-end gap-3">
				<button
					type="button"
					onclick={() => showConfirmModal = false}
					disabled={confirmLoading}
					class="rounded-lg border border-[var(--border-primary)] px-4 py-2 text-sm font-medium text-[var(--text-primary)] hover:bg-[var(--bg-secondary)] disabled:opacity-50"
				>
					Cancel
				</button>
				<button
					type="button"
					onclick={executeConfirm}
					disabled={confirmLoading}
					class="rounded-lg px-4 py-2 text-sm font-medium text-white disabled:opacity-50 {confirmButtonClass}"
				>
					{confirmLoading ? 'Please wait...' : confirmButtonText}
				</button>
			</div>
		</div>
	</div>
{/if}
