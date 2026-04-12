<script lang="ts">
	import { marked } from 'marked';
	import { onMount } from 'svelte';

	interface Props {
		source: string;
		class?: string;
	}

	let { source, class: className = '' }: Props = $props();

	/** Synchronous fallback: marked.parse may return a Promise; use parseInline for sync. */
	function render(md: string): string {
		const result = marked.parse(md, { async: false });
		return typeof result === 'string' ? result : '';
	}

	const html = $derived(render(source ?? ''));
</script>

<!-- eslint-disable-next-line svelte/no-at-html-tags -->
<div class="prose prose-sm dark:prose-invert max-w-none {className}">{@html html}</div>

<style>
	.prose :global(ul) {
		list-style: disc;
		padding-left: 1.5em;
	}
	.prose :global(ol) {
		list-style: decimal;
		padding-left: 1.5em;
	}
	.prose :global(h1),
	.prose :global(h2),
	.prose :global(h3),
	.prose :global(h4) {
		font-weight: 600;
		margin-top: 0.75em;
		margin-bottom: 0.25em;
		color: var(--text-primary);
	}
	.prose :global(p) {
		margin-bottom: 0.5em;
		color: var(--text-secondary);
	}
	.prose :global(code) {
		font-family: monospace;
		font-size: 0.875em;
		background: var(--bg-tertiary);
		border-radius: 0.2em;
		padding: 0.1em 0.3em;
	}
	.prose :global(pre) {
		background: var(--bg-tertiary);
		border-radius: 0.375rem;
		padding: 0.75rem 1rem;
		overflow-x: auto;
		margin-bottom: 0.5em;
	}
	.prose :global(pre code) {
		background: none;
		padding: 0;
	}
	.prose :global(blockquote) {
		border-left: 3px solid var(--border-primary);
		padding-left: 0.75em;
		color: var(--text-tertiary);
	}
	.prose :global(a) {
		color: var(--color-primary-600);
		text-decoration: underline;
	}
	.prose :global(hr) {
		border-color: var(--border-primary);
		margin: 0.75em 0;
	}
</style>
