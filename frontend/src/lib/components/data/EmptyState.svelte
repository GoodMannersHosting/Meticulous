<script lang="ts" module>
	export interface EmptyStateProps {
		title: string;
		description?: string;
		icon?: typeof import('lucide-svelte').Inbox;
		class?: string;
	}
</script>

<script lang="ts">
	import { Inbox } from 'lucide-svelte';

	let {
		title,
		description,
		icon: Icon = Inbox,
		class: className = '',
		children
	}: EmptyStateProps & { children?: import('svelte').Snippet } = $props();
</script>

<div class="flex flex-col items-center justify-center py-12 text-center {className}">
	<div class="mb-4 rounded-full bg-[var(--bg-tertiary)] p-4">
		<Icon class="h-8 w-8 text-[var(--text-tertiary)]" />
	</div>

	<h3 class="text-lg font-semibold text-[var(--text-primary)]">
		{title}
	</h3>

	{#if description}
		<p class="mt-1 max-w-sm text-sm text-[var(--text-secondary)]">
			{description}
		</p>
	{/if}

	{#if children}
		<div class="mt-6">
			{@render children()}
		</div>
	{/if}
</div>
