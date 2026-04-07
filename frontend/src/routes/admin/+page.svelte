<script lang="ts">
	import { Users, UserCog, Key, Server, ListOrdered, ChevronRight, AppWindow } from 'lucide-svelte';

	interface AdminLink {
		title: string;
		description: string;
		href: string;
		icon: typeof Users;
	}

	const adminLinks: AdminLink[] = [
		{
			title: 'Users',
			description: 'Manage user accounts, roles, and permissions',
			href: '/admin/users',
			icon: Users
		},
		{
			title: 'Groups',
			description: 'Create and manage user groups',
			href: '/admin/groups',
			icon: UserCog
		},
		{
			title: 'Authentication',
			description: 'Configure OIDC, GitHub, and other auth providers',
			href: '/admin/auth',
			icon: Key
		},
		{
			title: 'Meticulous Apps',
			description: 'Machine integrations, JWT signing keys, and project installations',
			href: '/admin/apps',
			icon: AppWindow
		},
		{
			title: 'Agents',
			description: 'Manage agent join tokens and registration',
			href: '/admin/agents',
			icon: Server
		},
		{
			title: 'Job queue',
			description: 'See pipeline jobs that are pending or queued for agents',
			href: '/admin/job-queue',
			icon: ListOrdered
		}
	];
</script>

<div class="space-y-6">
	<div class="grid gap-4 sm:grid-cols-2">
		{#each adminLinks as link (link.href)}
			{@const Icon = link.icon}
			<a
				href={link.href}
				class="group flex items-center gap-4 rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] p-4 transition-colors hover:border-primary-300 hover:bg-[var(--bg-hover)] dark:hover:border-primary-700"
			>
				<div class="flex h-12 w-12 shrink-0 items-center justify-center rounded-lg bg-[var(--bg-primary)]">
					<Icon class="h-6 w-6 text-[var(--text-secondary)] group-hover:text-primary-500" />
				</div>
				<div class="flex-1 min-w-0">
					<h3 class="font-medium text-[var(--text-primary)]">{link.title}</h3>
					<p class="text-sm text-[var(--text-secondary)] truncate">{link.description}</p>
				</div>
				<ChevronRight class="h-5 w-5 text-[var(--text-tertiary)] group-hover:text-primary-500" />
			</a>
		{/each}
	</div>
</div>
