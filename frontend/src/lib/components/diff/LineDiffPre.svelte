<script lang="ts">
	import { diffLines } from 'diff';

	let {
		beforeLabel,
		afterLabel,
		beforeText,
		afterText,
		maxHeightClass = 'max-h-[28rem]',
		class: className = ''
	}: {
		beforeLabel: string;
		afterLabel: string;
		beforeText: string;
		afterText: string;
		maxHeightClass?: string;
		class?: string;
	} = $props();

	const hunks = $derived(diffLines(beforeText, afterText));
</script>

<div class="space-y-2 {className}">
	<div class="flex flex-wrap items-center gap-2 text-xs text-[var(--text-secondary)]">
		<span class="font-medium text-[var(--text-primary)]">{beforeLabel}</span>
		<span aria-hidden="true">→</span>
		<span class="font-medium text-[var(--text-primary)]">{afterLabel}</span>
	</div>
	<pre
		class="overflow-auto rounded-md border border-[var(--border-primary)] bg-[var(--bg-tertiary)] p-3 font-mono text-xs leading-relaxed whitespace-pre-wrap break-words {maxHeightClass}"
	><code class="block">{#each hunks as part, i (i)}{#if part.added}<span
					class="block bg-emerald-200/90 text-emerald-950 dark:bg-emerald-900/45 dark:text-emerald-100"
					>+ {part.value}</span
				>{:else if part.removed}<span
					class="block bg-rose-200/90 text-rose-950 dark:bg-rose-900/45 dark:text-rose-100"
					>− {part.value}</span
				>{:else}<span class="block text-[var(--text-primary)]"> {part.value}</span>{/if}{/each}</code></pre>
</div>
