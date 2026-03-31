<script lang="ts" module>
	export interface TabItem {
		id: string;
		label: string;
		icon?: typeof import('lucide-svelte').Activity;
		disabled?: boolean;
	}

	export interface TabsProps {
		items: TabItem[];
		value: string;
		class?: string;
		onchange?: (id: string) => void;
	}
</script>

<script lang="ts">
	let {
		items,
		value = $bindable(),
		class: className = '',
		onchange
	}: TabsProps = $props();

	function selectTab(id: string) {
		value = id;
		onchange?.(id);
	}
</script>

<div class="border-b border-[var(--border-primary)] {className}">
	<nav class="-mb-px flex gap-4" aria-label="Tabs">
		{#each items as item (item.id)}
			{@const isActive = value === item.id}
			{@const Icon = item.icon}
			<button
				type="button"
				class="
					relative flex items-center gap-2 px-1 py-3 text-sm font-medium
					transition-colors
					{isActive
						? 'text-primary-600 dark:text-primary-400'
						: 'text-[var(--text-secondary)] hover:text-[var(--text-primary)]'}
					{item.disabled ? 'pointer-events-none opacity-50' : ''}
				"
				onclick={() => !item.disabled && selectTab(item.id)}
				disabled={item.disabled}
				aria-selected={isActive}
				role="tab"
			>
				{#if Icon}
					<Icon class="h-4 w-4" />
				{/if}
				{item.label}

				{#if isActive}
					<span
						class="absolute inset-x-0 -bottom-px h-0.5 bg-primary-600 dark:bg-primary-400"
					></span>
				{/if}
			</button>
		{/each}
	</nav>
</div>
