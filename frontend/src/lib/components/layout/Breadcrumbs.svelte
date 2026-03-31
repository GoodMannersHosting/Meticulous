<script lang="ts">
	import { ChevronRight, Home } from 'lucide-svelte';

	interface BreadcrumbItem {
		label: string;
		href?: string;
	}

	let { items }: { items: BreadcrumbItem[] } = $props();
</script>

<nav aria-label="Breadcrumb">
	<ol class="flex items-center gap-1 text-sm">
		<li>
			<a
				href="/dashboard"
				class="flex items-center text-[var(--text-secondary)] transition-colors hover:text-[var(--text-primary)]"
				aria-label="Home"
			>
				<Home class="h-4 w-4" />
			</a>
		</li>

		{#each items as item, index (index)}
			<li class="flex items-center gap-1">
				<ChevronRight class="h-4 w-4 text-[var(--text-tertiary)]" />
				{#if item.href && index < items.length - 1}
					<a
						href={item.href}
						class="text-[var(--text-secondary)] transition-colors hover:text-[var(--text-primary)]"
					>
						{item.label}
					</a>
				{:else}
					<span class="font-medium text-[var(--text-primary)]">{item.label}</span>
				{/if}
			</li>
		{/each}
	</ol>
</nav>
