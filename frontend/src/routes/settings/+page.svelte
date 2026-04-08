<script lang="ts">
	import { Button, Card, Input, Tabs, Alert, Badge, Avatar } from '$components/ui';
	import { auth, theme } from '$stores';
	import { User, Bell, Palette, Camera, Users } from 'lucide-svelte';

	let activeTab = $state('profile');
	let saving = $state(false);
	let message = $state<{ type: 'success' | 'error'; text: string } | null>(null);

	const tabs = [
		{ id: 'profile', label: 'Profile', icon: User },
		{ id: 'notifications', label: 'Notifications', icon: Bell },
		{ id: 'appearance', label: 'Appearance', icon: Palette }
	];

	let profileForm = $state({
		name: auth.user?.name ?? '',
		email: auth.user?.email ?? ''
	});

	async function saveProfile() {
		saving = true;
		try {
			await new Promise((resolve) => setTimeout(resolve, 500));
			message = { type: 'success', text: 'Profile updated successfully' };
		} catch (e) {
			message = { type: 'error', text: 'Failed to update profile' };
		} finally {
			saving = false;
		}
	}
</script>

<svelte:head>
	<title>Settings | Meticulous</title>
</svelte:head>

<div class="space-y-6">
	<div>
		<h1 class="text-2xl font-bold text-[var(--text-primary)]">Settings</h1>
		<p class="mt-1 text-[var(--text-secondary)]">
			Manage your account settings and preferences.
		</p>
		<p class="mt-2 text-sm">
			<a href="/settings/security" class="text-primary-600 hover:underline dark:text-primary-400">
				API tokens and security settings
			</a>
		</p>
	</div>

	{#if message}
		<Alert
			variant={message.type === 'success' ? 'success' : 'error'}
			dismissible
			ondismiss={() => (message = null)}
		>
			{message.text}
		</Alert>
	{/if}

	<Tabs items={tabs} bind:value={activeTab} />

	{#if activeTab === 'profile'}
		<Card>
			<div class="space-y-6">
				<div>
					<h3 class="text-lg font-medium text-[var(--text-primary)]">Profile Information</h3>
					<p class="mt-1 text-sm text-[var(--text-secondary)]">
						Update your personal information.
					</p>
				</div>

				<form onsubmit={(e) => { e.preventDefault(); saveProfile(); }} class="space-y-4">
					<div class="flex items-start gap-6">
						<div class="relative">
							<Avatar
								src={auth.user?.avatar}
								email={auth.user?.email}
								name={auth.user?.name}
								size="xl"
								gravatarDefault="identicon"
							/>
							<div class="absolute -bottom-1 -right-1 rounded-full bg-[var(--bg-secondary)] p-1 shadow-sm border border-[var(--border-primary)]">
								<Camera class="h-4 w-4 text-[var(--text-tertiary)]" />
							</div>
						</div>
						<div class="flex-1 space-y-1">
							<p class="text-sm font-medium text-[var(--text-primary)]">Profile Picture</p>
							<p class="text-xs text-[var(--text-secondary)]">
								Your avatar is powered by <a href="https://gravatar.com" target="_blank" rel="noopener noreferrer" class="text-primary-600 hover:underline dark:text-primary-400">Gravatar</a>.
								Update your Gravatar to change your profile picture.
							</p>
						</div>
					</div>

					<div class="grid gap-4 sm:grid-cols-2">
						<div>
							<label for="name" class="block text-sm font-medium text-[var(--text-primary)]">
								Name
							</label>
							<Input
								id="name"
								bind:value={profileForm.name}
								class="mt-1"
							/>
						</div>
						<div>
							<label for="email" class="block text-sm font-medium text-[var(--text-primary)]">
								Email
							</label>
							<Input
								id="email"
								type="email"
								bind:value={profileForm.email}
								class="mt-1"
								disabled
							/>
							<p class="mt-1 text-xs text-[var(--text-tertiary)]">
								Email is managed by your identity provider
							</p>
						</div>
					</div>

					<div class="flex justify-end">
						<Button variant="primary" type="submit" loading={saving}>
							Save Changes
						</Button>
					</div>
				</form>

				<div class="border-t border-[var(--border-primary)] pt-6">
					<div class="flex items-start gap-3">
						<div class="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-[var(--bg-tertiary)]">
							<Users class="h-5 w-5 text-[var(--text-secondary)]" />
						</div>
						<div class="min-w-0 flex-1">
							<h3 class="text-lg font-medium text-[var(--text-primary)]">Groups</h3>
							<p class="mt-1 text-sm text-[var(--text-secondary)]">
								Organization groups you belong to and your role in each.
							</p>
							{#if auth.user?.groups && auth.user.groups.length > 0}
								<ul class="mt-4 divide-y divide-[var(--border-secondary)] rounded-lg border border-[var(--border-primary)]">
									{#each auth.user.groups as g (g.id)}
										<li class="flex flex-wrap items-center justify-between gap-2 px-4 py-3">
											<span class="font-medium text-[var(--text-primary)]">{g.name}</span>
											<Badge variant="outline" size="sm">{g.role}</Badge>
										</li>
									{/each}
								</ul>
							{:else}
								<p class="mt-3 text-sm text-[var(--text-tertiary)]">
									You are not in any groups, or group data is not available yet.
								</p>
							{/if}
						</div>
					</div>
				</div>
			</div>
		</Card>
	{:else if activeTab === 'notifications'}
		<Card>
			<div class="space-y-6">
				<div>
					<h3 class="text-lg font-medium text-[var(--text-primary)]">Notification Preferences</h3>
					<p class="mt-1 text-sm text-[var(--text-secondary)]">
						Choose how you want to be notified about pipeline events.
					</p>
				</div>

				<div class="space-y-4">
					<label class="flex items-center justify-between">
						<div>
							<p class="font-medium text-[var(--text-primary)]">Pipeline failures</p>
							<p class="text-sm text-[var(--text-secondary)]">
								Get notified when a pipeline run fails
							</p>
						</div>
						<input type="checkbox" checked class="h-5 w-5 rounded border-secondary-300" />
					</label>

					<label class="flex items-center justify-between">
						<div>
							<p class="font-medium text-[var(--text-primary)]">Pipeline successes</p>
							<p class="text-sm text-[var(--text-secondary)]">
								Get notified when a pipeline run succeeds
							</p>
						</div>
						<input type="checkbox" class="h-5 w-5 rounded border-secondary-300" />
					</label>

					<label class="flex items-center justify-between">
						<div>
							<p class="font-medium text-[var(--text-primary)]">Agent status changes</p>
							<p class="text-sm text-[var(--text-secondary)]">
								Get notified when agents go offline or come online
							</p>
						</div>
						<input type="checkbox" checked class="h-5 w-5 rounded border-secondary-300" />
					</label>
				</div>
			</div>
		</Card>
	{:else if activeTab === 'appearance'}
		<Card>
			<div class="space-y-6">
				<div>
					<h3 class="text-lg font-medium text-[var(--text-primary)]">Appearance</h3>
					<p class="mt-1 text-sm text-[var(--text-secondary)]">
						Customize the look and feel of the application.
					</p>
				</div>

				<div>
					<p class="mb-3 text-sm font-medium text-[var(--text-primary)]">Theme</p>
					<div class="flex gap-3">
						<button
							type="button"
							class="
								flex flex-col items-center gap-2 rounded-lg border-2 p-4 transition-colors
								{theme.preference === 'light'
									? 'border-primary-500 bg-primary-50 dark:bg-primary-900/20'
									: 'border-[var(--border-primary)] hover:border-[var(--border-secondary)]'}
							"
							onclick={() => theme.set('light')}
						>
							<div class="h-12 w-20 rounded border bg-white"></div>
							<span class="text-sm">Light</span>
						</button>
						<button
							type="button"
							class="
								flex flex-col items-center gap-2 rounded-lg border-2 p-4 transition-colors
								{theme.preference === 'dark'
									? 'border-primary-500 bg-primary-50 dark:bg-primary-900/20'
									: 'border-[var(--border-primary)] hover:border-[var(--border-secondary)]'}
							"
							onclick={() => theme.set('dark')}
						>
							<div class="h-12 w-20 rounded border bg-secondary-900"></div>
							<span class="text-sm">Dark</span>
						</button>
						<button
							type="button"
							class="
								flex flex-col items-center gap-2 rounded-lg border-2 p-4 transition-colors
								{theme.preference === 'system'
									? 'border-primary-500 bg-primary-50 dark:bg-primary-900/20'
									: 'border-[var(--border-primary)] hover:border-[var(--border-secondary)]'}
							"
							onclick={() => theme.set('system')}
						>
							<div class="flex h-12 w-20 overflow-hidden rounded border">
								<div class="w-1/2 bg-white"></div>
								<div class="w-1/2 bg-secondary-900"></div>
							</div>
							<span class="text-sm">System</span>
						</button>
					</div>
				</div>
			</div>
		</Card>
	{/if}
</div>
