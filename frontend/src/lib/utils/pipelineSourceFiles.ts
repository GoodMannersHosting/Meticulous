import type { Pipeline } from '$api/types';

export type PipelineSourceRow = {
	kind: 'pipeline' | 'workflow_project' | 'workflow_global';
	label: string;
	repoPath: string | null;
	workflowRef?: string;
	version?: string;
};

/** owner/repo segments for github.com links (accepts owner/repo, full https URL, or git@ form). */
export function parseGithubRepository(repository: string | null | undefined): { owner: string; name: string } | null {
	if (!repository?.trim()) return null;
	let s = repository.trim();
	if (s.endsWith('.git')) s = s.slice(0, -4);
	// Repair `https:/host` (single slash) so URL() can parse.
	s = s.replace(/^https:\/(?!\/)/i, 'https://').replace(/^http:\/(?!\/)/i, 'http://');

	if (/^https?:\/\//i.test(s)) {
		try {
			const u = new URL(s);
			if (!/^github\.com$/i.test(u.hostname) && !/^www\.github\.com$/i.test(u.hostname)) {
				return null;
			}
			const segments = u.pathname.split('/').filter(Boolean);
			if (segments.length < 2) return null;
			return { owner: segments[0]!, name: segments.slice(1).join('/') };
		} catch {
			return null;
		}
	}

	if (/^git@github\.com:/i.test(s)) {
		const path = s.replace(/^git@github\.com:/i, '');
		const segments = path.split('/').filter(Boolean);
		if (segments.length < 2) return null;
		return { owner: segments[0]!, name: segments.slice(1).join('/') };
	}

	if (/^github\.com\//i.test(s)) {
		const rest = s.replace(/^github\.com\//i, '');
		const segments = rest.split('/').filter(Boolean);
		if (segments.length < 2) return null;
		return { owner: segments[0]!, name: segments.slice(1).join('/') };
	}

	const segments = s.split('/').filter(Boolean);
	if (segments.length < 2) return null;
	return { owner: segments[0]!, name: segments.slice(1).join('/') };
}

/** GitHub web URL for a file at ref (branch, tag, or full SHA). */
export function githubBlobUrl(
	owner: string,
	repo: string,
	gitRef: string,
	repoPath: string
): string {
	const enc = repoPath
		.split('/')
		.filter((s) => s.length > 0)
		.map(encodeURIComponent)
		.join('/');
	return `https://github.com/${owner}/${repo}/blob/${gitRef}/${enc}`;
}

export function pipelineGithubBlobRef(pipeline: Pipeline): string | null {
	const rev = pipeline.scm_revision?.trim();
	if (rev) return rev;
	const r = pipeline.scm_ref?.trim();
	return r || null;
}

/** Web URL for the repository tree at the same ref used for blob links (prefer resolved SHA). */
export function githubRepoTreeUrl(pipeline: Pipeline): string | null {
	if (pipeline.scm_provider !== 'github') return null;
	const slug = parseGithubRepository(pipeline.scm_repository);
	const gitRef = pipelineGithubBlobRef(pipeline);
	if (!slug || !gitRef) return null;
	return `https://github.com/${slug.owner}/${slug.name}/tree/${gitRef}`;
}

export function collectPipelineSourceRows(pipeline: Pipeline): PipelineSourceRow[] {
	const rows: PipelineSourceRow[] = [];
	const mainPath = pipeline.scm_path?.trim().replace(/^\//, '') || null;
	if (mainPath) {
		rows.push({
			kind: 'pipeline',
			label: 'Pipeline definition',
			repoPath: mainPath
		});
	}

	const def = pipeline.definition;
	if (!def || typeof def !== 'object') return rows;

	const workflows = (def as Record<string, unknown>).workflows;
	if (!Array.isArray(workflows)) return rows;

	for (const item of workflows) {
		if (!item || typeof item !== 'object') continue;
		const w = item as Record<string, unknown>;
		const ref = w.workflow;
		if (typeof ref !== 'string' || !ref.includes('/')) continue;

		const parts = ref.split('/');
		if (parts.length !== 2) continue;
		const scope = parts[0]!;
		const stem = parts[1]!;
		const versionRaw =
			typeof w.version === 'string'
				? w.version.trim()
				: typeof w.version === 'number'
					? String(w.version)
					: undefined;
		const stepName = typeof w.name === 'string' ? w.name : undefined;
		const label = stepName ? `${stepName} (${ref})` : ref;

		if (scope === 'project') {
			const v = versionRaw && versionRaw !== 'latest' ? versionRaw : undefined;
			const repoPath = v
				? `.stable/workflows/${stem}@${v}.yaml`
				: `.stable/workflows/${stem}.yaml`;
			rows.push({
				kind: 'workflow_project',
				label,
				repoPath,
				workflowRef: ref,
				version: versionRaw
			});
		} else if (scope === 'global') {
			rows.push({
				kind: 'workflow_global',
				label,
				repoPath: null,
				workflowRef: ref,
				version: versionRaw
			});
		}
	}

	return dedupeSourceRows(rows);
}

function dedupeSourceRows(rows: PipelineSourceRow[]): PipelineSourceRow[] {
	const seen = new Set<string>();
	const out: PipelineSourceRow[] = [];
	for (const r of rows) {
		const key =
			r.kind === 'workflow_global'
				? `g:${r.workflowRef ?? r.label}:${r.version ?? ''}`
				: `p:${r.repoPath ?? r.label}`;
		if (seen.has(key)) continue;
		seen.add(key);
		out.push(r);
	}
	return out;
}

export function upstreamLinkForRow(pipeline: Pipeline, row: PipelineSourceRow): string | null {
	if (pipeline.scm_provider !== 'github') return null;
	const slug = parseGithubRepository(pipeline.scm_repository);
	const gitRef = pipelineGithubBlobRef(pipeline);
	if (!slug || !gitRef || !row.repoPath) return null;
	return githubBlobUrl(slug.owner, slug.name, gitRef, row.repoPath);
}
