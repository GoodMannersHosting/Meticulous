import type { Run, RunStatus } from '$api/types';
import type { SortDirection } from '$components/data/DataTable.svelte';

/** Lifecycle-style rank: in-flight statuses sort before terminal ones. */
const STATUS_RANK: Record<string, number> = {
	pending: 0,
	queued: 1,
	running: 2,
	succeeded: 3,
	failed: 4,
	cancelled: 5,
	timed_out: 6
};

function statusRank(status: string): number {
	const k = String(status).toLowerCase().replace(/-/g, '_');
	return STATUS_RANK[k] ?? 99;
}

function compareRuns(a: Run, b: Run, key: string): number {
	switch (key) {
		case 'run_number':
			return Number(a.run_number) - Number(b.run_number);
		case 'pipeline_name':
			return (a.pipeline_name ?? '').toLowerCase().localeCompare((b.pipeline_name ?? '').toLowerCase());
		case 'project_name':
			return (a.project_name ?? '').toLowerCase().localeCompare((b.project_name ?? '').toLowerCase());
		case 'status': {
			const ra = statusRank(a.status as RunStatus);
			const rb = statusRank(b.status as RunStatus);
			return ra - rb;
		}
		case 'branch': {
			const sa = (a.branch ?? '').toLowerCase();
			const sb = (b.branch ?? '').toLowerCase();
			const c = sa.localeCompare(sb);
			if (c !== 0) return c;
			return (a.commit_sha ?? '').localeCompare(b.commit_sha ?? '');
		}
		case 'triggered_by':
			return a.triggered_by.toLowerCase().localeCompare(b.triggered_by.toLowerCase());
		case 'duration_ms': {
			const da = a.duration_ms;
			const db = b.duration_ms;
			if (da == null && db == null) return 0;
			if (da == null) return -1;
			if (db == null) return 1;
			return da - db;
		}
		case 'created_at': {
			const sa = a.started_at ?? a.created_at;
			const sb = b.started_at ?? b.created_at;
			return sa.localeCompare(sb);
		}
		default:
			return 0;
	}
}

export function sortRunList(
	runs: Run[],
	sortKey: string | null,
	sortDirection: SortDirection | undefined
): Run[] {
	if (!sortKey || !sortDirection) return runs;
	const out = [...runs];
	out.sort((a, b) => {
		let c = compareRuns(a, b, sortKey);
		// Stable, visible ordering when the primary key ties (e.g. status filter → all "succeeded").
		if (c === 0) {
			const ta = a.started_at ?? a.created_at;
			const tb = b.started_at ?? b.created_at;
			c = ta.localeCompare(tb);
			if (c === 0) c = a.id.localeCompare(b.id);
		}
		return sortDirection === 'asc' ? c : -c;
	});
	return out;
}
