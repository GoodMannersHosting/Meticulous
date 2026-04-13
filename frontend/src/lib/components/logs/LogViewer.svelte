<script lang="ts" module>
	export interface LogTimeFilter {
		/** ISO-8601 inclusive lower bound (typically assignment started_at or accepted_at). */
		startIso: string;
		/** ISO-8601 inclusive upper bound; omit or null for open-ended (running attempt). */
		endIso: string | null;
	}

	export const LOG_STEP_FILTER_ALL = 'all';
	export const LOG_STEP_FILTER_UNSCOPED = '__step_unscoped__';

	export interface LogViewerProps {
		runId: string;
		jobRunId: string;
		/** Polls while running/queued; merges incremental pages so the view does not flash. WebSocket lines merge when connected. */
		jobStatus?: string;
		/** When set, only log lines whose timestamps fall in [startIso, endIso] are shown (best-effort for multi-dispatch jobs). */
		logTimeFilter?: LogTimeFilter | null;
		/** Workflow step filter; use `bind:stepLogFilter` from the run page. */
		stepLogFilter?: string;
		/** Optional map of step_run_id → display name for exports. */
		stepDisplayNames?: Record<string, string>;
		/** Set by the viewer when any line lacks `step_run_id`; use `bind:hasUnscopedLogLines`. */
		hasUnscopedLogLines?: boolean;
		class?: string;
	}
</script>

<script lang="ts">
	import { browser } from '$app/environment';
	import { getWebSocketManager } from '$api';
	import type { LogLinePayload } from '$api';
	import { Button } from '$components/ui';
	import { Download, Terminal, ArrowDown, Search, X, Maximize2, Minimize2 } from 'lucide-svelte';
	import { apiMethods } from '$api/client';
	import { format, parseISO } from 'date-fns';

	let {
		runId,
		jobRunId,
		jobStatus = '',
		logTimeFilter = null,
		stepLogFilter = $bindable(LOG_STEP_FILTER_ALL),
		stepDisplayNames = {},
		hasUnscopedLogLines = $bindable(false),
		class: className = ''
	}: LogViewerProps = $props();

	let lines = $state<LogLinePayload[]>([]);
	/**
	 * Rows already consumed from the logs API (SQL OFFSET). Must not use `lines.length`:
	 * WebSocket lines merge into `lines` without existing in prior API pages, so an offset
	 * based on line count skips DB rows and the viewer shows output starting "mid-command".
	 */
	let apiLogOffset = $state(0);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let autoScroll = $state(true);
	let searchQuery = $state('');
	let showSearch = $state(false);
	let logContainer = $state<HTMLDivElement | undefined>(undefined);
	let logFullWindow = $state(false);
	let wsUnsubscribe: (() => void) | null = null;

	/** Git and many CLIs use `\r` to redraw a line; normalize so the log viewer shows separate lines. */
	function normalizeLogDisplayText(raw: string): string {
		return raw.replace(/\r\n/g, '\n').replace(/\r/g, '\n');
	}

	function jobIsLive(): boolean {
		return (
			jobStatus === 'running' || jobStatus === 'queued' || jobStatus === 'pending'
		);
	}

	function scrollToBottomIfFollowing() {
		if (!browser || !logContainer || !autoScroll) return;
		requestAnimationFrame(() => {
			requestAnimationFrame(() => {
				if (logContainer) {
					logContainer.scrollTop = logContainer.scrollHeight;
				}
			});
		});
	}

	/**
	 * Sequences restart from 0 per workflow step (`log_cache` PK is job_run_id + step_key + sequence).
	 * Keys and poll dedupe must include step scope, or multi-step jobs (e.g. SAST) collide and {#each} breaks.
	 */
	function logLineSequenceKey(line: LogLinePayload): string | null {
		if (line.sequence == null || !Number.isFinite(line.sequence)) return null;
		const step = (line.step_run_id ?? '').trim();
		return `${step}\0${line.sequence}`;
	}

	/** Stable {#each} key: DB sequence when present; else hash of content (WS / legacy). */
	function lineStableKey(line: LogLinePayload): string {
		const seqKey = logLineSequenceKey(line);
		if (seqKey != null) {
			return `seq:${seqKey}`;
		}
		let h = 5381;
		const s = `${line.timestamp}\0${line.level}\0${line.step_run_id ?? ''}\0${line.line}`;
		for (let i = 0; i < s.length; i++) {
			h = ((h << 5) + h) ^ s.charCodeAt(i);
		}
		return `h:${line.job_run_id}:${h >>> 0}`;
	}

	function mergeAppendBySequence(base: LogLinePayload[], incoming: LogLinePayload[]): LogLinePayload[] {
		const seen = new Set(
			base.map((l) => logLineSequenceKey(l)).filter((k): k is string => k != null)
		);
		const out = [...base];
		for (const l of incoming) {
			const k = logLineSequenceKey(l);
			if (k != null) {
				if (seen.has(k)) continue;
				seen.add(k);
			}
			out.push(l);
		}
		return out;
	}

	/**
	 * Show time with microsecond fractional digits (6), parsed from the ISO string so we do not
	 * lose sub-millisecond precision (JS Date is only millisecond-resolution).
	 */
	function formatLogTime(iso: string): string {
		const t = iso.trim();
		const m = t.match(/^(\d{4}-\d{2}-\d{2})T(\d{2}):(\d{2}):(\d{2})(\.\d+)?/);
		if (m) {
			const [, , hh, mm, ss, fracWithDot] = m;
			let micro = '000000';
			if (fracWithDot && fracWithDot.startsWith('.')) {
				let digits = fracWithDot.slice(1).replace(/[^\d]/g, '');
				if (digits.length > 6) digits = digits.slice(0, 6);
				else if (digits.length < 6) digits = digits.padEnd(6, '0');
				micro = digits;
			}
			return `${hh}:${mm}:${ss}.${micro}`;
		}
		try {
			const d = parseISO(iso);
			if (Number.isNaN(d.getTime())) return '—';
			return `${format(d, 'HH:mm:ss')}.000000`;
		} catch {
			return '—';
		}
	}

	function formatLogTimeTitle(iso: string): string {
		return iso.trim() || '—';
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

	/** Match API `StepRunId` JSON (`srun_<uuid>`). Log lines historically used bare UUID strings. */
	function normalizeStepRunId(id: string | undefined): string | undefined {
		if (id == null) return undefined;
		const t = id.trim();
		if (t === '') return undefined;
		if (t.startsWith('srun_')) return t;
		return `srun_${t}`;
	}

	function lineMatchesStepFilter(line: LogLinePayload, filter: string): boolean {
		if (filter === LOG_STEP_FILTER_ALL) return true;
		if (filter === LOG_STEP_FILTER_UNSCOPED) return !line.step_run_id?.trim();
		const lineKey = normalizeStepRunId(line.step_run_id);
		const filterKey = normalizeStepRunId(filter);
		return lineKey != null && filterKey != null && lineKey === filterKey;
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
			return data.lines.map((l) => ({
				sequence: l.sequence,
				run_id: l.run_id,
				job_run_id: l.job_run_id,
				step_run_id: l.step_run_id,
				line: l.line,
				level: l.level,
				timestamp: l.timestamp
			}));
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
		// Explicit reads so job / run switches always rebind polling and WebSocket filters.
		void runId;
		void jobRunId;
		void jobStatus;

		let poll: ReturnType<typeof setInterval> | null = null;
		const active =
			jobStatus === 'running' ||
			jobStatus === 'queued' ||
			jobStatus === 'pending';

		void loadLogsInitial();
		subscribeToLogs();

		if (active) {
			poll = setInterval(() => void loadLogsPoll(), 2000);
		}

		return () => {
			if (wsUnsubscribe) {
				wsUnsubscribe();
				wsUnsubscribe = null;
			}
			if (poll) clearInterval(poll);
		};
	});

	$effect(() => {
		hasUnscopedLogLines = lines.some((l) => !l.step_run_id?.trim());
	});

	/** Keep the viewport pinned when following (live or after End / ↓). */
	$effect(() => {
		if (loading && lines.length === 0) return;
		void lines.length;
		void stepLogFilter;
		void logTimeFilter?.startIso;
		void logTimeFilter?.endIso;
		scrollToBottomIfFollowing();
	});

	const MAX_LOG_PAGES = 200;

	async function loadLogPagesFrom(startOffset: number): Promise<{
		lines: LogLinePayload[];
		nextApiOffset: number;
	}> {
		const acc: LogLinePayload[] = [];
		let o = startOffset;
		for (let page = 0; page < MAX_LOG_PAGES; page++) {
			const response = await apiMethods.runs.logs(runId, jobRunId, {
				offset: o,
				limit: 10000
			});
			const batch = normalizeLogResponse(response, jobRunId, runId);
			acc.push(...batch);
			o += batch.length;
			if (!response.has_more || batch.length === 0) break;
		}
		return { lines: acc, nextApiOffset: o };
	}

	async function loadLogsInitial() {
		loading = true;
		error = null;
		apiLogOffset = 0;
		try {
			const { lines: initial, nextApiOffset } = await loadLogPagesFrom(0);
			lines = initial;
			apiLogOffset = nextApiOffset;
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load logs';
			lines = [];
			apiLogOffset = 0;
		} finally {
			loading = false;
		}
		autoScroll = true;
		scrollToBottomIfFollowing();
	}

	async function loadLogsPoll() {
		if (loading) return;
		try {
			const response = await apiMethods.runs.logs(runId, jobRunId, {
				offset: apiLogOffset,
				limit: 10000
			});
			const batch = normalizeLogResponse(response, jobRunId, runId);
			if (batch.length === 0) return;
			apiLogOffset += batch.length;
			lines = mergeAppendBySequence(lines, batch);
			scrollToBottomIfFollowing();
		} catch {
			/* ignore transient poll failures */
		}
	}

	function subscribeToLogs() {
		const ws = getWebSocketManager();
		if (!ws) return;

		wsUnsubscribe = ws.on<LogLinePayload>('log_line', (message) => {
			if (message.payload.job_run_id === jobRunId) {
				lines = mergeAppendBySequence(lines, [message.payload]);
				scrollToBottomIfFollowing();
			}
		});
	}

	function scrollToBottom() {
		if (logContainer) {
			logContainer.scrollTop = logContainer.scrollHeight;
			autoScroll = true;
			scrollToBottomIfFollowing();
		}
	}

	function handleScroll() {
		if (logContainer) {
			const { scrollTop, scrollHeight, clientHeight } = logContainer;
			const distanceFromBottom = scrollHeight - scrollTop - clientHeight;
			autoScroll = distanceFromBottom < 80;
		}
	}

	function onLogContainerKeydown(e: KeyboardEvent) {
		const goEnd =
			e.key === 'End' ||
			(e.key === 'ArrowDown' && (e.ctrlKey || e.metaKey));
		if (goEnd && !e.altKey) {
			e.preventDefault();
			scrollToBottom();
		}
	}

	function stepLabelForExport(line: LogLinePayload): string {
		const id = line.step_run_id?.trim();
		if (!id) return '';
		return (
			stepDisplayNames[id] ??
			stepDisplayNames[normalizeStepRunId(id) ?? id] ??
			id.replace(/^srun_/, '').slice(0, 8)
		);
	}

	const linesAfterScope = $derived.by(() => {
		let xs = lines;
		if (logTimeFilter) {
			xs = xs.filter((line) => lineMatchesTimeWindow(line, logTimeFilter));
		}
		xs = xs.filter((line) => lineMatchesStepFilter(line, stepLogFilter));
		return xs;
	});

	const indexedLines = $derived(
		linesAfterScope.map((line, i) => ({
			line,
			lineNo: i + 1
		}))
	);

	const filteredLines = $derived.by(() => {
		let rows = indexedLines;
		if (searchQuery.trim()) {
			rows = rows.filter(({ line }) =>
				lineMatchesSearch(normalizeLogDisplayText(line.line), searchQuery)
			);
		}
		return rows;
	});

	function downloadLogs() {
		const content = filteredLines
			.map(({ line }) => {
				const step = stepLabelForExport(line);
				const stepPart = step ? ` [${step}]` : '';
				return `${line.timestamp} [${line.level}]${stepPart} ${normalizeLogDisplayText(line.line)}`;
			})
			.join('\n');
		const blob = new Blob([content], { type: 'text/plain' });
		const url = URL.createObjectURL(blob);
		const a = document.createElement('a');
		a.href = url;
		const suffix = stepLogFilter !== LOG_STEP_FILTER_ALL ? `-${stepLogFilter.slice(0, 8)}` : '';
		a.download = `logs-${jobRunId}${suffix}.txt`;
		a.click();
		URL.revokeObjectURL(url);
	}

	function copyAllLogs() {
		const content = filteredLines.map(({ line }) => normalizeLogDisplayText(line.line)).join('\n');
		void navigator.clipboard.writeText(content);
	}

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
		class="flex shrink-0 flex-col gap-2 border-b border-zinc-700/80 bg-zinc-900/90 px-3 py-2"
	>
		<div class="flex items-center justify-between gap-2">
			<div class="flex min-w-0 items-center gap-2">
				<Terminal class="h-4 w-4 shrink-0 text-zinc-400" />
				<span class="truncate text-sm text-zinc-300">
					{lines.length} lines loaded
					{#if linesAfterScope.length !== lines.length}
						<span class="text-zinc-500"> · {linesAfterScope.length} shown</span>
					{/if}
					{#if !autoScroll}
						<span class="text-zinc-500"> · paused — End or ↓ to follow</span>
					{/if}
				</span>
			</div>

			<div class="flex shrink-0 items-center gap-1">
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
							onclick={() => {
								showSearch = false;
								searchQuery = '';
							}}
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
	</div>

	<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
	<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
	<div
		bind:this={logContainer}
		onscroll={handleScroll}
		onkeydown={onLogContainerKeydown}
		onmousedown={() => logContainer?.focus()}
		tabindex="0"
		role="log"
		aria-live="polite"
		aria-relevant="additions"
		aria-label="Build log output"
		class="min-h-0 flex-1 overflow-auto bg-zinc-950 font-mono text-xs outline-none focus-visible:ring-1 focus-visible:ring-sky-500/50"
	>
		{#if loading && lines.length === 0}
			<div class="space-y-1 p-4">
				{#each Array(20) as _, i (i)}
					<div class="h-4 animate-pulse rounded bg-zinc-800" style="width: {50 + Math.random() * 50}%"></div>
				{/each}
			</div>
		{:else if error}
			<div class="p-4 text-rose-400">{error}</div>
		{:else if filteredLines.length === 0}
			<div class="flex h-full flex-col items-center justify-center gap-1 px-4 text-center text-zinc-500">
				<p>
					{searchQuery.trim()
						? 'No matching lines'
						: lines.length === 0
							? 'No logs available'
							: 'No logs match the current step / dispatch filters'}
				</p>
			</div>
		{:else}
			<div class="p-2">
				{#each filteredLines as row (lineStableKey(row.line))}
					<div class="group flex min-h-0 hover:bg-zinc-900/70">
						<span
							class="w-14 flex-shrink-0 select-none pr-2 text-right tabular-nums text-zinc-500 group-hover:text-zinc-400"
						>
							{row.lineNo}
						</span>
						<span
							class="min-w-[9.5rem] flex-shrink-0 select-none pr-2 tabular-nums text-zinc-500 group-hover:text-zinc-400"
							title={formatLogTimeTitle(row.line.timestamp)}
						>
							{formatLogTime(row.line.timestamp)}
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
