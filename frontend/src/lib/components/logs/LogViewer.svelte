<script lang="ts" module>
	export interface LogTimeFilter {
		/** ISO-8601 inclusive lower bound (typically assignment started_at or accepted_at). */
		startIso: string;
		/** ISO-8601 inclusive upper bound; omit or null for open-ended (running attempt). */
		endIso: string | null;
	}

	export interface LogViewerProps {
		runId: string;
		jobRunId: string;
		/** When running/queued, logs are polled periodically (live WebSocket hub is not wired yet). */
		jobStatus?: string;
		/** When set, only log lines whose timestamps fall in [startIso, endIso] are shown (best-effort for multi-dispatch jobs). */
		logTimeFilter?: LogTimeFilter | null;
		class?: string;
	}
</script>

<script lang="ts">
	import { browser } from '$app/environment';
	import { apiMethods, getWebSocketManager } from '$api';
	import type { LogLinePayload } from '$api';
	import { Button } from '$components/ui';
	import { Download, Terminal, ArrowDown, Search, X, Maximize2, Minimize2 } from 'lucide-svelte';

	let {
		runId,
		jobRunId,
		jobStatus = '',
		logTimeFilter = null,
		class: className = ''
	}: LogViewerProps = $props();

	let lines = $state<LogLinePayload[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let autoScroll = $state(true);
	let searchQuery = $state('');
	let showSearch = $state(false);
	let logContainer: HTMLElement;
	let logFullWindow = $state(false);
	let wsUnsubscribe: (() => void) | null = null;

	/** Git and many CLIs use `\r` to redraw a line; normalize so the log viewer shows separate lines. */
	function normalizeLogDisplayText(raw: string): string {
		return raw.replace(/\r\n/g, '\n').replace(/\r/g, '\n');
	}

	/** Leading `!` excludes lines matching the pattern; `*` is a wildcard (any substring). */
	function parseSearchQuery(q: string): { exclude: boolean; pattern: string } | null {
		const t = q.trim();
		if (!t) return null;
		if (t.startsWith('!')) {
			const p = t.slice(1).trim();
			if (!p) return null;
			return { exclude: true, pattern: p };
		}
		return { exclude: false, pattern: t };
	}

	function globToRegex(pattern: string): RegExp {
		const escaped = pattern
			.split('*')
			.map((s) => s.replace(/[.+^${}()|[\]\\]/g, '\\$&'))
			.join('.*');
		return new RegExp(escaped, 'i');
	}

	function lineMatchesTimeWindow(line: LogLinePayload, filter: LogTimeFilter): boolean {
		const t = Date.parse(line.timestamp);
		if (Number.isNaN(t)) return true;
		const start = Date.parse(filter.startIso);
		if (!Number.isNaN(start) && t < start) return false;
		if (filter.endIso != null && filter.endIso !== '') {
			const end = Date.parse(filter.endIso);
			if (!Number.isNaN(end) && t > end) return false;
		}
		return true;
	}

	function lineMatchesSearch(normalizedLine: string, q: string): boolean {
		const parsed = parseSearchQuery(q);
		if (!parsed) return true;
		try {
			const re = globToRegex(parsed.pattern);
			const matches = re.test(normalizedLine);
			return parsed.exclude ? !matches : matches;
		} catch {
			return true;
		}
	}

	function toggleLogFullWindow() {
		logFullWindow = !logFullWindow;
	}

	$effect(() => {
		if (!browser || !logFullWindow) return;
		const prevOverflow = document.body.style.overflow;
		document.body.style.overflow = 'hidden';
		const onKey = (e: KeyboardEvent) => {
			if (e.key === 'Escape') logFullWindow = false;
		};
		window.addEventListener('keydown', onKey);
		return () => {
			document.body.style.overflow = prevOverflow;
			window.removeEventListener('keydown', onKey);
		};
	});

	function normalizeLogResponse(
		data: {
			lines?: LogLinePayload[];
			content?: string;
		},
		jobRun: string,
		run: string
	): LogLinePayload[] {
		if (data.lines && data.lines.length > 0) {
			return data.lines;
		}
		const raw = data.content?.trim();
		if (!raw) return [];
		const ts = new Date().toISOString();
		return raw.split('\n').map((line) => ({
			run_id: run,
			job_run_id: jobRun,
			line,
			level: 'stdout' as const,
			timestamp: ts
		}));
	}

	$effect(() => {
		let poll: ReturnType<typeof setInterval> | null = null;
		const active =
			jobStatus === 'running' ||
			jobStatus === 'queued' ||
			jobStatus === 'pending';

		void loadLogs(false);
		subscribeToLogs();

		if (active) {
			poll = setInterval(() => void loadLogs(true), 2000);
		}

		return () => {
			if (wsUnsubscribe) {
				wsUnsubscribe();
			}
			if (poll) clearInterval(poll);
		};
	});

	async function loadLogs(silent: boolean) {
		if (!silent) {
			loading = true;
			error = null;
		}
		try {
			const response = await apiMethods.runs.logs(runId, jobRunId);
			lines = normalizeLogResponse(response, jobRunId, runId);
		} catch (e) {
			if (!silent) {
				error = e instanceof Error ? e.message : 'Failed to load logs';
				lines = [];
			}
		} finally {
			if (!silent) loading = false;
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
		const content = lines
			.map((l) => `${l.timestamp} [${l.level}] ${normalizeLogDisplayText(l.line)}`)
			.join('\n');
		const blob = new Blob([content], { type: 'text/plain' });
		const url = URL.createObjectURL(blob);
		const a = document.createElement('a');
		a.href = url;
		a.download = `logs-${jobRunId}.txt`;
		a.click();
		URL.revokeObjectURL(url);
	}

	function copyAllLogs() {
		const content = lines.map((l) => normalizeLogDisplayText(l.line)).join('\n');
		navigator.clipboard.writeText(content);
	}

	const indexedLines = $derived(
		lines.map((line, i) => ({
			line,
			lineNo: i + 1
		}))
	);

	const filteredLines = $derived.by(() => {
		let rows = indexedLines;
		if (logTimeFilter) {
			rows = rows.filter(({ line }) => lineMatchesTimeWindow(line, logTimeFilter));
		}
		if (searchQuery.trim()) {
			rows = rows.filter(({ line }) =>
				lineMatchesSearch(normalizeLogDisplayText(line.line), searchQuery)
			);
		}
		return rows;
	});

	function getLevelClass(level: string): string {
		switch (level) {
			case 'stderr':
				return 'text-rose-300';
			case 'system':
				return 'text-sky-300';
			default:
				return 'text-zinc-200';
		}
	}
</script>

<div
	class="
		flex min-h-0 flex-col bg-zinc-950 {className}
		{logFullWindow
		? 'fixed inset-0 z-50 h-dvh max-h-dvh w-full shadow-2xl'
		: 'h-full'}
	"
>
	<div
		class="flex shrink-0 items-center justify-between border-b border-zinc-700/80 bg-zinc-900/90 px-3 py-2"
	>
		<div class="flex items-center gap-2">
			<Terminal class="h-4 w-4 text-zinc-400" />
			<span class="text-sm text-zinc-300">
				{lines.length} lines
			</span>
		</div>

		<div class="flex items-center gap-1">
			{#if showSearch}
				<div class="relative">
					<input
						type="text"
						placeholder="Pattern or !exclude — * wildcards"
						bind:value={searchQuery}
						class="
							h-7 w-56 rounded border border-zinc-600 bg-zinc-900 px-2 text-sm text-zinc-100
							placeholder:text-zinc-500
							focus:border-sky-500 focus:outline-none focus:ring-1 focus:ring-sky-500/40
						"
					/>
					<button
						type="button"
						class="absolute right-1 top-1/2 -translate-y-1/2 p-0.5 text-zinc-500 hover:text-zinc-300"
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
			<Button
				variant="ghost"
				size="sm"
				onclick={toggleLogFullWindow}
				title={logFullWindow ? 'Exit full window' : 'Full window'}
			>
				{#if logFullWindow}
					<Minimize2 class="h-4 w-4" />
				{:else}
					<Maximize2 class="h-4 w-4" />
				{/if}
			</Button>
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
		class="min-h-0 flex-1 overflow-auto bg-zinc-950 font-mono text-xs"
	>
		{#if loading}
			<div class="space-y-1 p-4">
				{#each Array(20) as _, i (i)}
					<div class="h-4 animate-pulse rounded bg-zinc-800" style="width: {50 + Math.random() * 50}%"></div>
				{/each}
			</div>
		{:else if error}
			<div class="p-4 text-rose-400">{error}</div>
		{:else if filteredLines.length === 0}
			<div class="flex h-full items-center justify-center text-zinc-500">
				{searchQuery.trim() ? 'No matching lines' : 'No logs available'}
			</div>
		{:else}
			<div class="p-2">
				{#each filteredLines as row (row.lineNo)}
					<div class="group flex min-h-0 hover:bg-zinc-900/70">
						<span
							class="w-14 flex-shrink-0 select-none pr-2 text-right tabular-nums text-zinc-500 group-hover:text-zinc-400"
						>
							{row.lineNo}
						</span>
						<span
							class="min-w-0 flex-1 whitespace-pre-wrap break-words {getLevelClass(row.line.level)}"
						>
							{normalizeLogDisplayText(row.line.line)}
						</span>
					</div>
				{/each}
			</div>
		{/if}
	</div>
</div>
