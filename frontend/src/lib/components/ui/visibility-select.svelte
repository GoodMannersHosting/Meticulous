<script lang="ts">
	import type { ResourceVisibility } from '$lib/api/types';

	interface Props {
		value: ResourceVisibility;
		onchange?: (value: ResourceVisibility) => void;
		disabled?: boolean;
	}

	let { value = $bindable(), onchange, disabled = false }: Props = $props();

	const options: { value: ResourceVisibility; label: string; description: string }[] = [
		{
			value: 'public',
			label: 'Public',
			description: 'Visible to anyone (metadata only)'
		},
		{
			value: 'authenticated',
			label: 'Authenticated',
			description: 'All org members can see this'
		},
		{
			value: 'private',
			label: 'Private',
			description: 'Only explicit members'
		}
	];

	function handleChange(e: Event) {
		const target = e.target as HTMLSelectElement;
		value = target.value as ResourceVisibility;
		onchange?.(value);
	}
</script>

<div class="flex flex-col gap-1">
	<label class="text-sm font-medium text-zinc-300">Visibility</label>
	<select
		class="rounded-md border border-zinc-700 bg-zinc-800 px-3 py-2 text-sm text-zinc-200 focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
		{value}
		{disabled}
		onchange={handleChange}
	>
		{#each options as opt}
			<option value={opt.value}>{opt.label} — {opt.description}</option>
		{/each}
	</select>
</div>
