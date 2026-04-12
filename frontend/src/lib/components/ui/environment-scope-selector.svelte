<script lang="ts">
	import type { Environment } from '$lib/api/types';

	interface Props {
		environments: Environment[];
		selected: string | null;
		onchange?: (envId: string | null) => void;
	}

	let { environments, selected = $bindable(null), onchange }: Props = $props();

	function select(envId: string | null) {
		selected = envId;
		onchange?.(envId);
	}
</script>

<div class="flex flex-wrap gap-1.5">
	<button
		class="rounded-full px-3 py-1 text-xs font-medium transition-colors {selected === null
			? 'bg-primary-500/20 text-primary-400 ring-1 ring-primary-500/40'
			: 'bg-[var(--bg-tertiary)] text-[var(--text-secondary)] hover:text-[var(--text-primary)]'}"
		onclick={() => select(null)}
	>
		Global
	</button>
	{#each environments as env}
		<button
			class="rounded-full px-3 py-1 text-xs font-medium transition-colors {selected === env.id
				? 'bg-primary-500/20 text-primary-400 ring-1 ring-primary-500/40'
				: 'bg-[var(--bg-tertiary)] text-[var(--text-secondary)] hover:text-[var(--text-primary)]'}"
			onclick={() => select(env.id)}
		>
			{env.display_name}
			<span class="ml-1 opacity-50 text-[10px]">({env.tier})</span>
		</button>
	{/each}
</div>
