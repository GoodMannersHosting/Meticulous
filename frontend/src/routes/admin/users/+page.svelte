<script lang="ts">
	import { Users, Plus, Search, MoreVertical, Shield, ShieldCheck, User, Lock, Unlock, Trash2, Key, X } from 'lucide-svelte';
	import { apiMethods } from '$lib/api';
	import type { AdminUser } from '$lib/api/types';

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

	async function loadUsers() {
		loading = true;
		error = null;
		try {
			const response = await apiMethods.admin.users.list({ limit: 100 });
			users = response.items;
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

	function toggleActionMenu(userId: string) {
		actionMenuOpen = actionMenuOpen === userId ? null : userId;
	}

	function closeActionMenu() {
		actionMenuOpen = null;
	}

	async function lockUser(userId: string) {
		closeActionMenu();
		try {
			await apiMethods.admin.users.lock(userId);
			await loadUsers();
		} catch (e) {
			alert(e instanceof Error ? e.message : 'Failed to lock user');
		}
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

	async function deleteUser(userId: string) {
		closeActionMenu();
		if (!confirm('Are you sure you want to delete this user? This action cannot be undone.')) {
			return;
		}
		try {
			await apiMethods.admin.users.delete(userId);
			await loadUsers();
		} catch (e) {
			alert(e instanceof Error ? e.message : 'Failed to delete user');
		}
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
			disabled
		>
			<Plus class="h-4 w-4" />
			Add User
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
		<div class="overflow-hidden rounded-lg border border-[var(--border-primary)]">
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
							Created
						</th>
						<th class="px-4 py-3 text-right text-xs font-medium uppercase tracking-wider text-[var(--text-secondary)]">
							Actions
						</th>
					</tr>
				</thead>
				<tbody class="divide-y divide-[var(--border-primary)] bg-[var(--bg-primary)]">
					{#each filteredUsers as user (user.id)}
						<tr class="hover:bg-[var(--bg-hover)]">
							<td class="whitespace-nowrap px-4 py-3">
								<div class="flex items-center gap-3">
									<div class="flex h-8 w-8 items-center justify-center rounded-full bg-[var(--bg-secondary)]">
										<User class="h-4 w-4 text-[var(--text-secondary)]" />
									</div>
									<div>
										<div class="font-medium text-[var(--text-primary)]">
											{user.display_name || user.username}
										</div>
										<div class="text-sm text-[var(--text-secondary)]">{user.email}</div>
									</div>
								</div>
							</td>
							<td class="whitespace-nowrap px-4 py-3">
								{#if user.is_admin}
									<span class="inline-flex items-center gap-1 rounded-full bg-primary-100 px-2 py-0.5 text-xs font-medium text-primary-700 dark:bg-primary-900/30 dark:text-primary-400">
										<ShieldCheck class="h-3 w-3" />
										Admin
									</span>
								{:else}
									<span class="inline-flex items-center gap-1 rounded-full bg-gray-100 px-2 py-0.5 text-xs font-medium text-gray-700 dark:bg-gray-800 dark:text-gray-300">
										<Shield class="h-3 w-3" />
										User
									</span>
								{/if}
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
								{new Date(user.created_at).toLocaleDateString()}
							</td>
							<td class="whitespace-nowrap px-4 py-3 text-right">
								<div class="relative inline-block">
									<button
										type="button"
										onclick={() => toggleActionMenu(user.id)}
										class="rounded p-1 text-[var(--text-secondary)] hover:bg-[var(--bg-secondary)] hover:text-[var(--text-primary)]"
									>
										<MoreVertical class="h-4 w-4" />
									</button>
									{#if actionMenuOpen === user.id}
										<div class="absolute right-0 z-10 mt-1 w-48 rounded-lg border border-[var(--border-primary)] bg-[var(--bg-primary)] py-1 shadow-lg">
											<button
												type="button"
												onclick={() => openResetPasswordModal(user.id, user.username)}
												class="flex w-full items-center gap-2 px-4 py-2 text-left text-sm text-[var(--text-primary)] hover:bg-[var(--bg-hover)]"
											>
												<Key class="h-4 w-4" />
												Reset Password
											</button>
											{#if user.is_active}
												<button
													type="button"
													onclick={() => lockUser(user.id)}
													class="flex w-full items-center gap-2 px-4 py-2 text-left text-sm text-[var(--text-primary)] hover:bg-[var(--bg-hover)]"
												>
													<Lock class="h-4 w-4" />
													Lock Account
												</button>
											{:else}
												<button
													type="button"
													onclick={() => unlockUser(user.id)}
													class="flex w-full items-center gap-2 px-4 py-2 text-left text-sm text-[var(--text-primary)] hover:bg-[var(--bg-hover)]"
												>
													<Unlock class="h-4 w-4" />
													Unlock Account
												</button>
											{/if}
											<button
												type="button"
												onclick={() => deleteUser(user.id)}
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
	{/if}
</div>

{#if showResetPasswordModal}
	<div class="fixed inset-0 z-50 flex items-center justify-center bg-black/50" onclick={() => showResetPasswordModal = false}>
		<div class="w-full max-w-md rounded-lg bg-[var(--bg-primary)] p-6 shadow-xl" onclick={(e) => e.stopPropagation()}>
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
