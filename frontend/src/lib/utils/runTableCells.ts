import type { Run, RunStatus } from '$api/types';
import {
	formatDateTimeForTitle,
	formatDurationMs,
	formatRelativeTime,
	formatRunTriggeredBy
} from './format';
import { escapeHtml } from './html';

export function runNumberHtml(value: unknown, row?: Run): string {
	const num = `<span class="font-mono text-sm">#${escapeHtml(String(value ?? ''))}</span>`;
	if (row?.parent_run_id) {
		return `<span class="inline-flex flex-wrap items-center gap-1.5">${num}<span class="rounded border border-amber-200 bg-amber-50 px-1.5 py-0.5 text-[10px] font-medium uppercase tracking-wide text-amber-900 dark:border-amber-800 dark:bg-amber-950/50 dark:text-amber-200">Retry</span></span>`;
	}
	return num;
}

/** Link to pipeline detail; stops row click from opening the run. */
export function runPipelineLinkHtml(_value: unknown, row: Run): string {
	const name = row.pipeline_name;
	if (!name) {
		return '<span class="text-[var(--text-tertiary)]">—</span>';
	}
	const href = `/pipelines/${encodeURIComponent(row.pipeline_id)}`;
	return `<a href="${href}" class="text-sm font-medium text-primary-600 hover:text-primary-700 hover:underline dark:text-primary-400" onclick="event.stopPropagation()">${escapeHtml(name)}</a>`;
}

const STATUS_BADGE_CLASSES: Record<RunStatus, string> = {
	pending: 'bg-secondary-100 dark:bg-secondary-800 text-secondary-700 dark:text-secondary-300',
	queued: 'bg-secondary-100 dark:bg-secondary-800 text-secondary-700 dark:text-secondary-300',
	running: 'bg-primary-100 dark:bg-primary-900/30 text-primary-700 dark:text-primary-400',
	succeeded: 'bg-success-100 dark:bg-success-900/30 text-success-700 dark:text-success-400',
	failed: 'bg-error-100 dark:bg-error-900/30 text-error-700 dark:text-error-400',
	cancelled: 'bg-secondary-100 dark:bg-secondary-800 text-secondary-600 dark:text-secondary-400',
	timed_out: 'bg-warning-100 dark:bg-warning-900/30 text-warning-700 dark:text-warning-400'
};

function formatRunStatusLabel(status: string): string {
	return status
		.replace(/_/g, ' ')
		.replace(/\b\w/g, (c) => c.toUpperCase());
}

/** Primary badge status: API may suggest `queued` while `status` is still `running`. */
export function effectiveRunStatusForBadge(row: Run): RunStatus | string {
	return row.status_display ?? row.status;
}

export function runStatusBadgeHtml(status: RunStatus | string): string {
	const key = status as RunStatus;
	const cls = STATUS_BADGE_CLASSES[key] ?? STATUS_BADGE_CLASSES.pending;
	const label = formatRunStatusLabel(String(status));
	return `<span class="inline-flex items-center rounded-full font-medium px-2 py-0.5 text-xs ${cls}">${escapeHtml(label)}</span>`;
}

export function runBranchColumnHtml(_value: unknown, row: Run): string {
	if (!row.branch && !row.commit_sha) {
		return '<span class="text-[var(--text-tertiary)]">—</span>';
	}
	let html = '';
	if (row.branch) {
		html += `<span class="text-sm">${escapeHtml(row.branch)}</span>`;
	}
	if (row.commit_sha) {
		html += `<span class="ml-2 font-mono text-xs text-[var(--text-tertiary)]">${escapeHtml(row.commit_sha.slice(0, 7))}</span>`;
	}
	return html;
}

export function runTriggeredByHtml(value: unknown, row?: Partial<Run>): string {
	const display = formatRunTriggeredBy(String(value ?? '—'), row?.webhook_remote_addr);
	return `<span class="text-sm">${escapeHtml(display)}</span>`;
}

export function runDurationHtml(value: unknown): string {
	return `<span class="text-sm">${escapeHtml(formatDurationMs(value as number))}</span>`;
}

/** Relative “started” time with native tooltip showing absolute local start time. */
export function runStartedAtHtml(_value: unknown, row: Run): string {
	const iso = row.started_at ?? row.created_at;
	if (iso == null || iso === '') {
		return '<span class="text-sm text-[var(--text-tertiary)]">—</span>';
	}
	const rel = formatRelativeTime(iso);
	const abs = formatDateTimeForTitle(iso);
	const titleAttr = abs ? ` title="${escapeHtml(abs)}"` : '';
	return `<span class="met-run-started-at text-sm text-[var(--text-secondary)]"${titleAttr}>${escapeHtml(rel)}</span>`;
}
