/** Normalized fields for the run Agents tab — always from `job_runs.agent_snapshot` only. */
export type JobRunAgentAudit = {
	/** When the controller stored `agent_snapshot_captured_at` */
	snapshotCapturedAt?: string;
	name: string;
	status: string;
	os: string;
	arch: string;
	version: string;
	pool: string | null;
	pool_tags: string[];
	tags: string[];
	running_jobs: number;
	max_jobs: number;
	last_heartbeat_at: string | null;
	created_at: string;
	last_security_bundle: Record<string, unknown> | null;
	agent_id?: string;
};

function str(v: unknown, fallback = ''): string {
	if (v == null) return fallback;
	if (typeof v === 'string') return v;
	return String(v);
}

function num(v: unknown, fallback = 0): number {
	if (typeof v === 'number' && Number.isFinite(v)) return v;
	if (typeof v === 'string' && v.trim() !== '') {
		const n = Number(v);
		if (Number.isFinite(n)) return n;
	}
	return fallback;
}

function strArr(v: unknown): string[] {
	if (!Array.isArray(v)) return [];
	return v.map((x) => String(x));
}

export function auditFromSnapshot(
	snapshot: Record<string, unknown>,
	snapshotCapturedAt?: string
): JobRunAgentAudit {
	const bundle = snapshot.last_security_bundle;
	return {
		snapshotCapturedAt,
		name: str(snapshot.name, 'Unknown agent'),
		status: str(snapshot.status, 'offline').toLowerCase(),
		os: str(snapshot.os, '—'),
		arch: str(snapshot.arch, '—'),
		version: str(snapshot.version, '—'),
		pool: snapshot.pool == null || snapshot.pool === '' ? null : str(snapshot.pool),
		pool_tags: strArr(snapshot.pool_tags),
		tags: strArr(snapshot.tags),
		running_jobs: num(snapshot.running_jobs),
		max_jobs: num(snapshot.max_jobs, 1),
		last_heartbeat_at: snapshot.last_heartbeat_at ? str(snapshot.last_heartbeat_at) : null,
		created_at: str(snapshot.created_at, ''),
		last_security_bundle:
			bundle && typeof bundle === 'object' && !Array.isArray(bundle)
				? (bundle as Record<string, unknown>)
				: null,
		agent_id: snapshot.id ? str(snapshot.id) : undefined
	};
}

/** For agents sidebar: draining + idle → paused badge */
export function agentBadgeStatusFromAudit(audit: JobRunAgentAudit): string {
	if (audit.status === 'draining' && audit.running_jobs === 0) return 'paused';
	return audit.status;
}
