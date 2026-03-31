<script lang="ts" module>
	export interface LogViewerProps {
		runId: string;
		jobRunId: string;
		class?: string;
	}
</script>

<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { apiMethods, getWebSocketManager } from '$api';
	import type { LogLinePayload } from '$api';
	import { Skeleton } from '$components/data';
	import { Button, CopyButton } from '$components/ui';
	import { Download, Terminal, ArrowDown, Search, X } from 'lucide-svelte';

	let { runId, jobRunId, class: className = '' }: LogViewerProps = $props();

	let lines = $state<LogLinePayload[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let autoScroll = $state(true);
	let searchQuery = $state('');
	let showSearch = $state(false);
	let logContainer: HTMLElement;
	let wsUnsubscribe: (() => void) | null = null;

	$effect(() => {
		loadLogs();
		subscribeToLogs();

		return () => {
			if (wsUnsubscribe) {
				wsUnsubscribe();
			}
		};
	});

	async function loadLogs() {
		loading = true;
		error = null;
		try {
			const response = await apiMethods.runs.logs(runId, jobRunId);
			lines = response.lines ?? [];
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load logs';
			lines = [];
		} finally {
			loading = false;
		}
	}

	function subscribeToLogs() {
		const ws = getWebSocketManager();
		if (!ws) return;

		wsUnsubscribe = ws.on<LogLinePayload>('log_line', (message) => {
			if (message.payload.job_run_id === jobRunId) {
				lines = [...lines, message.payload];
				if (autoScroll && logContainer) {
					requestAnimationFrame(() => {
						logContainer.scrollTop = logContainer.scrollHeight;
					});
				}
			}
		});
	}

	function scrollToBottom() {
		if (logContainer) {
			logContainer.scrollTop = logContainer.scrollHeight;
			autoScroll = true;
		}
	}

	function handleScroll() {
		if (logContainer) {
			const { scrollTop, scrollHeight, clientHeight } = logContainer;
			autoScroll = scrollHeight - scrollTop - clientHeight < 50;
		}
	}

	function downloadLogs() {
		const content = lines.map((l) => `${l.timestamp} [${l.level}] ${l.line}`).join('\n');
		const blob = new Blob([content], { type: 'text/plain' });
		const url = URL.createObjectURL(blob);
		const a = document.createElement('a');
		a.href = url;
		a.download = `logs-${jobRunId}.txt`;
		a.click();
		URL.revokeObjectURL(url);
	}

	function copyAllLogs() {
		const content = lines.map((l) => l.line).join('\n');
		navigator.clipboard.writeText(content);
	}

	const filteredLines = $derived(
		searchQuery
			? lines.filter((l) => l.line.toLowerCase().includes(searchQuery.toLowerCase()))
			: lines
	);

	function getLevelClass(level: string): string {
		switch (level) {
			case 'stderr':
				return 'text-error-500';
			case 'system':
				return 'text-primary-500';
			default:
				return 'text-[var(--text-primary)]';
		}
	}
</script>

<div class="flex h-full flex-col {className}">
	<div class="flex items-center justify-between border-b border-[var(--border-primary)] px-3 py-2">
		<div class="flex items-center gap-2">
			<Terminal class="h-4 w-4 text-[var(--text-secondary)]" />
			<span class="text-sm text-[var(--text-secondary)]">
				{lines.length} lines
			</span>
		</div>

		<div class="flex items-center gap-1">
			{#if showSearch}
				<div class="relative">
					<input
						type="text"
						placeholder="Search logs..."
						bind:value={searchQuery}
						class="
							h-7 w-48 rounded border border-[var(--border-primary)]
							bg-[var(--bg-tertiary)] px-2 text-sm
							focus:outline-none focus:ring-1 focus:ring-primary-500
						"
					/>
					<button
						type="button"
						class="absolute right-1 top-1/2 -translate-y-1/2 p-0.5 text-[var(--text-tertiary)] hover:text-[var(--text-secondary)]"
						onclick={() => { showSearch = false; searchQuery = ''; }}
					>
						<X class="h-3.5 w-3.5" />
					</button>
				</div>
			{:else}
				<Button variant="ghost" size="sm" onclick={() => (showSearch = true)}>
					<Search class="h-4 w-4" />
				</Button>
			{/if}
			<Button variant="ghost" size="sm" onclick={copyAllLogs}>
				Copy
			</Button>
			<Button variant="ghost" size="sm" onclick={downloadLogs}>
				<Download class="h-4 w-4" />
			</Button>
			{#if !autoScroll}
				<Button variant="ghost" size="sm" onclick={scrollToBottom}>
					<ArrowDown class="h-4 w-4" />
				</Button>
			{/if}
		</div>
	</div>

	<div
		bind:this={logContainer}
		onscroll={handleScroll}
		class="flex-1 overflow-auto bg-secondary-950 font-mono text-xs"
	>
		{#if loading}
			<div class="space-y-1 p-4">
				{#each Array(20) as _, i (i)}
					<div class="h-4 animate-pulse rounded bg-secondary-800" style="width: {50 + Math.random() * 50}%"></div>
				{/each}
			</div>
		{:else if error}
			<div class="p-4 text-error-500">{error}</div>
		{:else if filteredLines.length === 0}
			<div class="flex h-full items-center justify-center text-secondary-500">
				{searchQuery ? 'No matching lines' : 'No logs available'}
			</div>
		{:else}
			<div class="p-2">
				{#each filteredLines as line, index (index)}
					<div class="group flex hover:bg-secondary-900/50">
						<span class="w-12 flex-shrink-0 select-none pr-2 text-right text-secondary-600">
							{index + 1}
						</span>
						<span class="flex-1 whitespace-pre-wrap break-all {getLevelClass(line.level)}">
							{line.line}
						</span>
					</div>
				{/each}
			</div>
		{/if}
	</div>
</div>
