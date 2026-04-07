<script lang="ts">
	import hljs from 'highlight.js/lib/core';
	import yaml from 'highlight.js/lib/languages/yaml';
	import 'highlight.js/styles/github-dark.min.css';

	hljs.registerLanguage('yaml', yaml);

	/** Match code line-height so gutters align (13px × 1.45). */
	const LINE_STYLE = 'text-[13px] leading-[1.45]';

	let { source }: { source: string } = $props();

	let html = $state('');

	const lineNumbers = $derived.by(() => {
		const text = source ?? '';
		const n = text.split('\n').length;
		return Array.from({ length: Math.max(1, n) }, (_, i) => i + 1);
	});

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
		<p class="px-3 py-4 text-center text-sm text-[var(--text-tertiary)]">No definition to display.</p>
	{:else}
		<div
			class="flex max-h-[min(70vh,720px)] overflow-auto"
			style="scrollbar-gutter: stable;"
		>
			<div
				class="sticky left-0 z-10 shrink-0 select-none border-r border-[#30363d] bg-[#0d1117] py-1.5 pl-2 pr-2 text-right font-mono tabular-nums text-[#8b949e] {LINE_STYLE}"
				aria-hidden="true"
			>
				{#each lineNumbers as n (n)}
					<div class="whitespace-pre">{n}</div>
				{/each}
			</div>
			<!-- No whitespace inside <code>: it would become a text node and indent the first line. -->
			<!-- github-dark.min.css sets `pre code.hljs { padding: 1em }` which misaligns the gutter -->
			<pre
				class="m-0 min-w-0 flex-1 whitespace-pre py-1.5 pl-1 pr-2 font-mono {LINE_STYLE} text-[#e6edf3] [&_.hljs]:!bg-transparent [&_code.hljs]:!m-0 [&_code.hljs]:!p-0"
			><code class="hljs language-yaml">{#if html}{@html html}{:else}{source}{/if}</code></pre>
		</div>
	{/if}
</div>
