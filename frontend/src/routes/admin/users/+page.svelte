<script lang="ts">
	import { goto } from '$app/navigation';
	import { Users, Plus, Search, MoreVertical, Shield, ShieldCheck, Lock, Unlock, Trash2, Key, X, ExternalLink, AlertTriangle } from 'lucide-svelte';
	import { apiMethods } from '$lib/api';
	import type { AdminUser } from '$lib/api/types';
	import { getGravatarUrl } from '$lib/utils/gravatar';
	import { auth } from '$stores';
	import Pagination from '$lib/components/data/Pagination.svelte';

	let searchQuery = $state('');
	let users = $state<AdminUser[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let actionMenuOpen = $state<string | null>(null);
	let showResetPasswordModal = $state(false);
	let resetPasswordUserId = $state<string | null>(null);
	let resetPasswordUsername = $state('');
	let newPassword = $state('');
	let resetPasswordLoading = $state(false);
	let resetPasswordError = $state<string | null>(null);

	let usersPage = $state(1);
	let usersPerPage = $state(20);

	let showServiceAccountModal = $state(false);
	let saUsername = $state('');
	let saEmail = $state('');
	let saDisplayName = $state('');
	let saSubmitting = $state(false);
	let saError = $state<string | null>(null);

	// Confirmation modal state
	let showConfirmModal = $state(false);
	let confirmTitle = $state('');
	let confirmMessage = $state('');
	let confirmButtonText = $state('Confirm');
	let confirmButtonClass = $state('bg-primary-600 hover:bg-primary-700');
	let confirmAction = $state<(() => Promise<void>) | null>(null);
	let confirmLoading = $state(false);

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

	async function loadUsers() {
		loading = true;
		error = null;
		try {
			const response = await apiMethods.admin.users.list({ limit: 100 });
			users = response.data;
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load users';
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		loadUsers();
	});

	const filteredUsers = $derived(
		users.filter(
			(u) =>
				u.username.toLowerCase().includes(searchQuery.toLowerCase()) ||
				u.email.toLowerCase().includes(searchQuery.toLowerCase()) ||
				(u.display_name?.toLowerCase().includes(searchQuery.toLowerCase()) ?? false)
		)
	);

	const pagedUsers = $derived(
		filteredUsers.slice((usersPage - 1) * usersPerPage, usersPage * usersPerPage)
	);

	$effect(() => {
		searchQuery;
		usersPage = 1;
	});

	$effect(() => {
		const n = filteredUsers.length;
		const maxPage = Math.max(1, Math.ceil(n / usersPerPage) || 1);
		if (usersPage > maxPage) usersPage = maxPage;
	});

	function toggleActionMenu(userId: string) {
		actionMenuOpen = actionMenuOpen === userId ? null : userId;
	}

	function closeActionMenu() {
		actionMenuOpen = null;
	}

	function requestLockUser(userId: string) {
		closeActionMenu();
		const u = users.find((row) => row.id === userId);
		if (!u) return;
		if (auth.user && userId === auth.user.id) {
			return;
		}
		const display = u.display_name || u.username;
		openConfirmModal({
			title: 'Are you sure?',
			message: `Lock account for ${display}? They will not be able to sign in until an admin unlocks the account.`,
			buttonText: 'Lock account',
			danger: true,
			onConfirm: async () => {
				await apiMethods.admin.users.lock(userId);
				await loadUsers();
			}
		});
	}

	async function unlockUser(userId: string) {
		closeActionMenu();
		try {
			await apiMethods.admin.users.unlock(userId);
			await loadUsers();
		} catch (e) {
			alert(e instanceof Error ? e.message : 'Failed to unlock user');
		}
	}

	function deleteUser(userId: string) {
		closeActionMenu();
		const user = users.find(u => u.id === userId);
		const userName = user?.display_name || user?.username || 'this user';
		
		openConfirmModal({
			title: 'Delete User',
			message: `Are you sure you want to delete ${userName}? This action cannot be undone and will remove all their data.`,
			buttonText: 'Delete User',
			danger: true,
			onConfirm: async () => {
				await apiMethods.admin.users.delete(userId);
				await loadUsers();
			}
		});
	}

	function openResetPasswordModal(userId: string, username: string) {
		closeActionMenu();
		resetPasswordUserId = userId;
		resetPasswordUsername = username;
		newPassword = '';
		resetPasswordError = null;
		showResetPasswordModal = true;
	}

	async function resetPassword() {
		if (!resetPasswordUserId || !newPassword) return;
		if (newPassword.length < 8) {
			resetPasswordError = 'Password must be at least 8 characters';
			return;
		}

		resetPasswordLoading = true;
		resetPasswordError = null;
		try {
			await apiMethods.admin.users.resetPassword(resetPasswordUserId, newPassword);
			showResetPasswordModal = false;
			resetPasswordUserId = null;
			newPassword = '';
		} catch (e) {
			resetPasswordError = e instanceof Error ? e.message : 'Failed to reset password';
		} finally {
			resetPasswordLoading = false;
		}
	}

	function toggleAdmin(userId: string, currentIsAdmin: boolean, e: Event) {
		e.stopPropagation();
		closeActionMenu();
		
		const user = users.find(u => u.id === userId);
		const userName = user?.display_name || user?.username || 'this user';
		
		openConfirmModal({
			title: currentIsAdmin ? 'Remove Admin Privileges' : 'Grant Admin Privileges',
			message: currentIsAdmin 
				? `Are you sure you want to remove admin privileges from ${userName}? They will no longer be able to access admin features.`
				: `Are you sure you want to grant admin privileges to ${userName}? They will have full access to all admin features.`,
			buttonText: currentIsAdmin ? 'Remove Admin' : 'Make Admin',
			danger: currentIsAdmin,
			onConfirm: async () => {
				await apiMethods.admin.users.update(userId, { is_admin: !currentIsAdmin });
				await loadUsers();
			}
		});
	}

	function goToUser(userId: string) {
		goto(`/admin/users/${userId}`);
	}

	async function submitServiceAccount() {
		saSubmitting = true;
		saError = null;
		try {
			await apiMethods.admin.users.createServiceAccount({
				username: saUsername.trim(),
				email: saEmail.trim(),
				...(saDisplayName.trim() ? { display_name: saDisplayName.trim() } : {})
			});
			showServiceAccountModal = false;
			saUsername = '';
			saEmail = '';
			saDisplayName = '';
			await loadUsers();
		} catch (e) {
			saError = e instanceof Error ? e.message : 'Failed to create service account';
		} finally {
			saSubmitting = false;
		}
	}
</script>

<div class="space-y-6">
	<div class="flex items-center justify-between gap-4">
		<div>
			<h2 class="text-lg font-semibold text-[var(--text-primary)]">Users</h2>
			<p class="text-sm text-[var(--text-secondary)]">Manage user accounts and their roles</p>
		</div>
		<button
			type="button"
			class="inline-flex items-center gap-2 rounded-lg bg-primary-600 px-4 py-2 text-sm font-medium text-white hover:bg-primary-700"
			onclick={() => {
				showServiceAccountModal = true;
				saError = null;
			}}
		>
			<Plus class="h-4 w-4" />
			Service account
		</button>
	</div>

	<div class="relative">
		<Search class="absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-[var(--text-tertiary)]" />
		<input
			type="text"
			bind:value={searchQuery}
			placeholder="Search users..."
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
	{:else if filteredUsers.length === 0}
		<div class="flex flex-col items-center justify-center rounded-lg border border-dashed border-[var(--border-primary)] py-12">
			<Users class="h-12 w-12 text-[var(--text-tertiary)]" />
			<h3 class="mt-4 text-sm font-medium text-[var(--text-primary)]">
				{searchQuery ? 'No users found' : 'No users yet'}
			</h3>
			<p class="mt-1 text-sm text-[var(--text-secondary)]">
				{searchQuery ? 'Try adjusting your search' : 'Add your first user to get started'}
			</p>
		</div>
	{:else}
		<div class="overflow-visible rounded-lg border border-[var(--border-primary)]">
			<table class="min-w-full divide-y divide-[var(--border-primary)]">
				<thead class="bg-[var(--bg-secondary)]">
					<tr>
						<th class="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-[var(--text-secondary)]">
							User
						</th>
						<th class="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-[var(--text-secondary)]">
							Role
						</th>
						<th class="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-[var(--text-secondary)]">
							Status
						</th>
						<th class="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-[var(--text-secondary)]">
							Last login
						</th>
						<th class="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-[var(--text-secondary)]">
							Created
						</th>
						<th class="px-4 py-3 text-right text-xs font-medium uppercase tracking-wider text-[var(--text-secondary)]">
							Actions
						</th>
					</tr>
				</thead>
				<tbody class="divide-y divide-[var(--border-primary)] bg-[var(--bg-primary)]">
					{#each pagedUsers as user (user.id)}
						<tr class="cursor-pointer hover:bg-[var(--bg-hover)]" onclick={() => goToUser(user.id)}>
							<td class="whitespace-nowrap px-4 py-3">
								<div class="flex items-center gap-3">
									<img
										src={getGravatarUrl(user.email, { size: 32 })}
										alt={user.display_name || user.username}
										class="h-8 w-8 rounded-full"
									/>
									<div>
										<div class="font-medium text-[var(--text-primary)]">
											{user.display_name || user.username}
										</div>
										<div class="text-sm text-[var(--text-secondary)]">{user.email}</div>
									</div>
								</div>
							</td>
							<td class="whitespace-nowrap px-4 py-3">
								<button
									type="button"
									onclick={(e) => toggleAdmin(user.id, user.is_admin, e)}
									class="group inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-xs font-medium transition-colors {user.is_admin 
										? 'bg-primary-100 text-primary-700 hover:bg-primary-200 dark:bg-primary-900/30 dark:text-primary-400 dark:hover:bg-primary-900/50' 
										: 'bg-gray-100 text-gray-700 hover:bg-gray-200 dark:bg-gray-800 dark:text-gray-300 dark:hover:bg-gray-700'}"
									title={user.is_admin ? 'Click to remove admin' : 'Click to make admin'}
								>
									{#if user.service_account}
										<span class="text-[var(--text-tertiary)]" title="Service account">SA</span>
									{/if}
									{#if user.is_admin}
										<ShieldCheck class="h-3 w-3" />
										Admin
									{:else}
										<Shield class="h-3 w-3" />
										User
									{/if}
								</button>
							</td>
							<td class="whitespace-nowrap px-4 py-3">
								{#if user.is_active}
									<span class="inline-flex items-center gap-1.5 text-sm text-green-600 dark:text-green-400">
										<span class="h-1.5 w-1.5 rounded-full bg-green-500"></span>
										Active
									</span>
								{:else}
									<span class="inline-flex items-center gap-1.5 text-sm text-gray-500">
										<span class="h-1.5 w-1.5 rounded-full bg-gray-400"></span>
										Locked
									</span>
								{/if}
							</td>
							<td class="whitespace-nowrap px-4 py-3 text-sm text-[var(--text-secondary)]">
								{#if user.last_login_at}
									{new Date(user.last_login_at).toLocaleString(undefined, {
										dateStyle: 'medium',
										timeStyle: 'short'
									})}
								{:else}
									<span class="text-[var(--text-tertiary)]">—</span>
								{/if}
							</td>
							<td class="whitespace-nowrap px-4 py-3 text-sm text-[var(--text-secondary)]">
								{new Date(user.created_at).toLocaleDateString()}
							</td>
							<td class="whitespace-nowrap px-4 py-3 text-right">
								<div class="relative inline-flex items-center gap-1">
									<a
										href="/admin/users/{user.id}"
										onclick={(e) => e.stopPropagation()}
										class="rounded p-1 text-[var(--text-secondary)] hover:bg-[var(--bg-secondary)] hover:text-[var(--text-primary)]"
										title="View profile"
									>
										<ExternalLink class="h-4 w-4" />
									</a>
									<button
										type="button"
										onclick={(e) => { e.stopPropagation(); toggleActionMenu(user.id); }}
										class="rounded p-1 text-[var(--text-secondary)] hover:bg-[var(--bg-secondary)] hover:text-[var(--text-primary)]"
									>
										<MoreVertical class="h-4 w-4" />
									</button>
									{#if actionMenuOpen === user.id}
										<div class="absolute right-0 z-50 mt-1 w-48 rounded-lg border border-[var(--border-primary)] bg-[var(--bg-primary)] py-1 shadow-lg" style="top: 100%;">
											<button
												type="button"
												onclick={(e) => { e.stopPropagation(); openResetPasswordModal(user.id, user.username); }}
												class="flex w-full items-center gap-2 px-4 py-2 text-left text-sm text-[var(--text-primary)] hover:bg-[var(--bg-hover)]"
											>
												<Key class="h-4 w-4" />
												Reset Password
											</button>
											{#if user.is_active && auth.user?.id !== user.id}
												<button
													type="button"
													onclick={(e) => { e.stopPropagation(); requestLockUser(user.id); }}
													class="flex w-full items-center gap-2 px-4 py-2 text-left text-sm text-[var(--text-primary)] hover:bg-[var(--bg-hover)]"
												>
													<Lock class="h-4 w-4" />
													Lock Account
												</button>
											{:else}
												<button
													type="button"
													onclick={(e) => { e.stopPropagation(); unlockUser(user.id); }}
													class="flex w-full items-center gap-2 px-4 py-2 text-left text-sm text-[var(--text-primary)] hover:bg-[var(--bg-hover)]"
												>
													<Unlock class="h-4 w-4" />
													Unlock Account
												</button>
											{/if}
											<button
												type="button"
												onclick={(e) => { e.stopPropagation(); deleteUser(user.id); }}
												class="flex w-full items-center gap-2 px-4 py-2 text-left text-sm text-red-600 hover:bg-[var(--bg-hover)] dark:text-red-400"
											>
												<Trash2 class="h-4 w-4" />
												Delete User
											</button>
										</div>
									{/if}
								</div>
							</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
		{#if filteredUsers.length > usersPerPage}
			<div class="mt-4">
				<Pagination bind:page={usersPage} bind:perPage={usersPerPage} total={filteredUsers.length} />
			</div>
		{/if}
	{/if}
</div>

{#if showResetPasswordModal}
	<div class="fixed inset-0 z-50 flex items-center justify-center p-4">
		<button
			type="button"
			class="absolute inset-0 bg-black/50"
			aria-label="Close dialog"
			onclick={() => (showResetPasswordModal = false)}
		></button>
		<div class="relative z-10 w-full max-w-md rounded-lg bg-[var(--bg-primary)] p-6 shadow-xl">
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
				Set a new password for <strong>{resetPasswordUsername}</strong>
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

{#if showServiceAccountModal}
	<div class="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4">
		<div class="w-full max-w-md rounded-lg bg-[var(--bg-primary)] p-6 shadow-xl">
			<h3 class="text-lg font-semibold text-[var(--text-primary)]">Create service account</h3>
			<p class="mt-2 text-sm text-[var(--text-secondary)]">
				API-only user: password login and interactive SSO completion are blocked. Use organization-scoped API tokens
				for this principal.
			</p>
			{#if saError}
				<div
					class="mt-4 rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-700 dark:border-red-900 dark:bg-red-950/50 dark:text-red-400"
				>
					{saError}
				</div>
			{/if}
			<div class="mt-4 space-y-3">
				<div>
					<label for="sa-user" class="text-sm font-medium text-[var(--text-primary)]">Username</label>
					<input
						id="sa-user"
						bind:value={saUsername}
						class="mt-1 w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm"
					/>
				</div>
				<div>
					<label for="sa-email" class="text-sm font-medium text-[var(--text-primary)]">Email</label>
					<input
						id="sa-email"
						type="email"
						bind:value={saEmail}
						class="mt-1 w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm"
					/>
				</div>
				<div>
					<label for="sa-disp" class="text-sm font-medium text-[var(--text-primary)]">Display name (optional)</label>
					<input
						id="sa-disp"
						bind:value={saDisplayName}
						class="mt-1 w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm"
					/>
				</div>
			</div>
			<div class="mt-6 flex justify-end gap-3">
				<button
					type="button"
					class="rounded-lg border border-[var(--border-primary)] px-4 py-2 text-sm"
					onclick={() => (showServiceAccountModal = false)}
				>
					Cancel
				</button>
				<button
					type="button"
					class="rounded-lg bg-primary-600 px-4 py-2 text-sm text-white disabled:opacity-50"
					disabled={saSubmitting || !saUsername.trim() || !saEmail.trim()}
					onclick={() => void submitServiceAccount()}
				>
					{saSubmitting ? 'Creating…' : 'Create'}
				</button>
			</div>
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
