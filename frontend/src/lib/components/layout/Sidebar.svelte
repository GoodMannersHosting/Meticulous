<script lang="ts">
	import { page } from '$app/stores';
	import { sidebar } from '$stores';
	import {
		LayoutDashboard,
		FolderKanban,
		GitBranch,
		Play,
		Server,
		Settings,
		ChevronLeft,
		ChevronRight,
		Shield
	} from 'lucide-svelte';
	import Tooltip from '../ui/tooltip.svelte';

	interface NavItem {
		label: string;
		href: string;
		icon: typeof LayoutDashboard;
	}

	interface NavSection {
		title?: string;
		items: NavItem[];
	}

	const navSections: NavSection[] = [
		{
			items: [
				{ label: 'Dashboard', href: '/dashboard', icon: LayoutDashboard }
			]
		},
		{
			title: 'CI/CD',
			items: [
				{ label: 'Projects', href: '/projects', icon: FolderKanban },
				{ label: 'Pipelines', href: '/pipelines', icon: GitBranch },
				{ label: 'Runs', href: '/runs', icon: Play }
			]
		},
		{
			title: 'Infrastructure',
			items: [
				{ label: 'Agents', href: '/agents', icon: Server }
			]
		},
		{
			title: 'Administration',
			items: [
				{ label: 'Settings', href: '/settings', icon: Settings },
				{ label: 'Security', href: '/settings/security', icon: Shield }
			]
		}
	];

	const sidebarWidth = $derived(
		sidebar.collapsed ? 'var(--sidebar-collapsed-width)' : 'var(--sidebar-width)'
	);

	const translateX = $derived(
		sidebar.isMobile && !sidebar.mobileOpen ? '-100%' : '0'
	);

	function isActive(href: string): boolean {
		return $page.url.pathname === href || $page.url.pathname.startsWith(href + '/');
	}
</script>

<aside
	class="
		fixed left-0 top-0 z-40 h-screen
		border-r border-[var(--border-primary)]
		bg-[var(--bg-secondary)]
		transition-all duration-200 ease-out
	"
	style="width: {sidebarWidth}; transform: translateX({translateX});"
	aria-label="Sidebar navigation"
>
	<div class="flex h-full flex-col">
		<div class="flex h-16 items-center justify-between border-b border-[var(--border-primary)] px-4">
			{#if !sidebar.collapsed}
				<a href="/dashboard" class="flex items-center gap-2">
					<div class="flex h-8 w-8 items-center justify-center rounded-lg bg-gradient-to-br from-primary-500 to-primary-700">
						<svg class="h-5 w-5 text-white" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
							<path d="M5 12L10 17L20 7" />
						</svg>
					</div>
					<span class="text-lg font-semibold text-[var(--text-primary)]">Meticulous</span>
				</a>
			{:else}
				<a href="/dashboard" class="mx-auto flex h-8 w-8 items-center justify-center rounded-lg bg-gradient-to-br from-primary-500 to-primary-700" aria-label="Meticulous Home">
					<svg class="h-5 w-5 text-white" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
						<path d="M5 12L10 17L20 7" />
					</svg>
				</a>
			{/if}
		</div>

		<nav class="flex-1 overflow-y-auto p-3">
			<div class="space-y-6">
				{#each navSections as section, sectionIndex (sectionIndex)}
					<div>
						{#if section.title && !sidebar.collapsed}
							<h3 class="mb-2 px-3 text-xs font-semibold uppercase tracking-wider text-[var(--text-tertiary)]">
								{section.title}
							</h3>
						{/if}
						<ul class="space-y-1">
							{#each section.items as item (item.href)}
								{@const active = isActive(item.href)}
								{@const Icon = item.icon}
								<li>
									{#if sidebar.collapsed}
										<Tooltip content={item.label} side="right">
											<a
												href={item.href}
												class="
													flex h-10 w-10 items-center justify-center rounded-lg
													transition-colors duration-150
													{active
														? 'bg-primary-100 text-primary-700 dark:bg-primary-900/30 dark:text-primary-400'
														: 'text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)]'}
												"
												aria-current={active ? 'page' : undefined}
											>
												<Icon class="h-5 w-5" />
											</a>
										</Tooltip>
									{:else}
										<a
											href={item.href}
											class="
												flex items-center gap-3 rounded-lg px-3 py-2
												transition-colors duration-150
												{active
													? 'bg-primary-100 text-primary-700 dark:bg-primary-900/30 dark:text-primary-400'
													: 'text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)]'}
											"
											aria-current={active ? 'page' : undefined}
										>
											<Icon class="h-5 w-5" />
											<span class="text-sm font-medium">{item.label}</span>
										</a>
									{/if}
								</li>
							{/each}
						</ul>
					</div>
				{/each}
			</div>
		</nav>

		{#if !sidebar.isMobile}
			<div class="border-t border-[var(--border-primary)] p-3">
				<button
					type="button"
					class="
						flex w-full items-center justify-center gap-2 rounded-lg px-3 py-2
						text-[var(--text-secondary)] transition-colors duration-150
						hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)]
					"
					onclick={() => sidebar.toggle()}
					aria-label={sidebar.collapsed ? 'Expand sidebar' : 'Collapse sidebar'}
				>
					{#if sidebar.collapsed}
						<ChevronRight class="h-5 w-5" />
					{:else}
						<ChevronLeft class="h-5 w-5" />
						<span class="text-sm font-medium">Collapse</span>
					{/if}
				</button>
			</div>
		{/if}
	</div>
</aside>
