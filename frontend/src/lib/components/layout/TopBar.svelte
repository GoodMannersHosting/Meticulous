<script lang="ts">
	import { page } from '$app/stores';
	import { auth, theme, sidebar } from '$stores';
	import { Menu, Sun, Moon, Bell, User, LogOut, Settings } from 'lucide-svelte';
	import { DropdownMenu } from 'bits-ui';
	import Breadcrumbs from './Breadcrumbs.svelte';
	import { Avatar } from '$components/ui';

	const breadcrumbs = $derived($page.data.breadcrumbs ?? []);
</script>

<header
	class="
		sticky top-0 z-20
		h-16 border-b border-[var(--border-primary)]
		bg-[var(--bg-secondary)]/95 backdrop-blur
	"
>
	<div class="flex h-full items-center justify-between px-4 sm:px-6">
		<div class="flex items-center gap-4">
			{#if sidebar.isMobile}
				<button
					type="button"
					class="
						flex h-9 w-9 items-center justify-center rounded-lg
						text-[var(--text-secondary)] transition-colors
						hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)]
					"
					onclick={() => sidebar.toggle()}
					aria-label="Toggle sidebar"
				>
					<Menu class="h-5 w-5" />
				</button>
			{/if}

			{#if breadcrumbs.length > 0}
				<Breadcrumbs items={breadcrumbs} />
			{/if}
		</div>

		<div class="flex items-center gap-2">
			<button
				type="button"
				class="
					flex h-9 w-9 items-center justify-center rounded-lg
					text-[var(--text-secondary)] transition-colors
					hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)]
				"
				onclick={() => theme.toggle()}
				aria-label={theme.isDark ? 'Switch to light mode' : 'Switch to dark mode'}
			>
				{#if theme.isDark}
					<Sun class="h-5 w-5" />
				{:else}
					<Moon class="h-5 w-5" />
				{/if}
			</button>

			<button
				type="button"
				class="
					relative flex h-9 w-9 items-center justify-center rounded-lg
					text-[var(--text-secondary)] transition-colors
					hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)]
				"
				aria-label="Notifications"
			>
				<Bell class="h-5 w-5" />
			</button>

			<DropdownMenu.Root>
				<DropdownMenu.Trigger
					class="
						flex h-9 w-9 items-center justify-center rounded-full
						overflow-hidden
						transition-colors hover:ring-2 hover:ring-primary-300 dark:hover:ring-primary-700
					"
					aria-label="User menu"
				>
					<Avatar
						src={auth.user?.avatar}
						email={auth.user?.email}
						name={auth.user?.name}
						size="sm"
						gravatarDefault="identicon"
					/>
				</DropdownMenu.Trigger>

				<DropdownMenu.Portal>
					<DropdownMenu.Content
						class="
							z-50 min-w-[12rem] overflow-hidden rounded-lg
							border border-[var(--border-primary)]
							bg-[var(--bg-secondary)] p-1 shadow-lg
							data-[state=open]:animate-in data-[state=closed]:animate-out 
							data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 
							data-[state=closed]:zoom-out-95 data-[state=open]:zoom-in-95
						"
						sideOffset={8}
						align="end"
					>
						{#if auth.user}
							<div class="border-b border-[var(--border-secondary)] px-3 py-2">
								<p class="text-sm font-medium text-[var(--text-primary)]">{auth.user.name}</p>
								<p class="text-xs text-[var(--text-secondary)]">{auth.user.email}</p>
							</div>
						{/if}

						<DropdownMenu.Item
							class="
								flex cursor-pointer items-center gap-2 rounded-md px-3 py-2 text-sm
								text-[var(--text-primary)] outline-none
								data-[highlighted]:bg-[var(--bg-hover)]
							"
						>
							<Settings class="h-4 w-4 text-[var(--text-secondary)]" />
							Settings
						</DropdownMenu.Item>

						<DropdownMenu.Separator class="my-1 h-px bg-[var(--border-secondary)]" />

						<DropdownMenu.Item
							class="
								flex cursor-pointer items-center gap-2 rounded-md px-3 py-2 text-sm
								text-error-600 outline-none
								data-[highlighted]:bg-error-50 dark:text-error-500 dark:data-[highlighted]:bg-error-900/20
							"
							onclick={() => auth.logout()}
						>
							<LogOut class="h-4 w-4" />
							Sign out
						</DropdownMenu.Item>
					</DropdownMenu.Content>
				</DropdownMenu.Portal>
			</DropdownMenu.Root>
		</div>
	</div>
</header>
