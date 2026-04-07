<script lang="ts">
	import hljs from 'highlight.js/lib/core';
	import yaml from 'highlight.js/lib/languages/yaml';
	import 'highlight.js/styles/github-dark.min.css';

	hljs.registerLanguage('yaml', yaml);

	let { source }: { source: string } = $props();

	let html = $state('');

	$effect(() => {
		const text = source ?? '';
		if (!text.trim()) {
			html = '';
			return;
		}
		try {
			html = hljs.highlight(text, { language: 'yaml' }).value;
		} catch {
			html = hljs.highlightAuto(text).value;
		}
	});
</script>

<div
	class="overflow-hidden rounded-lg border border-[var(--border-primary)] bg-[#0d1117] shadow-inner"
	role="region"
	aria-label="Workflow YAML"
>
	{#if !(source ?? '').trim()}
		<p class="px-4 py-6 text-center text-sm text-[var(--text-tertiary)]">No definition to display.</p>
	{:else}
		<pre
			class="m-0 max-h-[min(70vh,720px)] overflow-auto p-4 text-[13px] leading-relaxed [&_.hljs]:!bg-transparent"
		><code class="hljs yaml">
				{#if html}
					{@html html}
				{:else}
					{source}
				{/if}
			</code></pre>
	{/if}
</div>
