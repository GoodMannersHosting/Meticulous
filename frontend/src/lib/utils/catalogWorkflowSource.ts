import type { CatalogWorkflow } from '$api/types';
import { githubBlobUrl, parseGithubRepository } from './pipelineSourceFiles';

/** Resolved git ref for linking: prefer pinned SHA, else branch/tag from import. */
export function catalogWorkflowGitRef(workflow: CatalogWorkflow): string | null {
	const rev = workflow.scm_revision?.trim();
	if (rev) return rev;
	const r = workflow.scm_ref?.trim();
	return r || null;
}

function parseGitlabWebRepository(
	repository: string | null | undefined
): { origin: string; projectPath: string } | null {
	if (!repository?.trim()) return null;
	let s = repository.trim();
	if (s.endsWith('.git')) s = s.slice(0, -4);
	s = s.replace(/^https:\/(?!\/)/i, 'https://').replace(/^http:\/(?!\/)/i, 'http://');

	if (/^git@/i.test(s)) {
		const m = /^git@([^:]+):(.+)$/i.exec(s);
		if (!m) return null;
		const host = m[1]!;
		const path = m[2]!.replace(/\.git$/, '');
		if (!host.includes('gitlab')) return null;
		return { origin: `https://${host}`, projectPath: path };
	}

	if (!/^https?:\/\//i.test(s)) {
		return null;
	}

	try {
		const u = new URL(s);
		if (!u.hostname.includes('gitlab')) return null;
		let projectPath = u.pathname.replace(/^\//, '').replace(/\.git$/, '');
		if (!projectPath) return null;
		// Hosted: /group/subgroup/repo
		return { origin: u.origin, projectPath };
	} catch {
		return null;
	}
}

function gitlabBlobUrl(origin: string, projectPath: string, gitRef: string, repoPath: string): string {
	const enc = repoPath
		.split('/')
		.filter((seg) => seg.length > 0)
		.map(encodeURIComponent)
		.join('/');
	const encRef = encodeURIComponent(gitRef);
	return `${origin}/${projectPath}/-/blob/${encRef}/${enc}`;
}

/**
 * Web URL for the workflow YAML at the pinned revision (or ref) and path.
 * Supports GitHub (including `owner/repo` shorthand) and GitLab (`https://…/group/repo`).
 */
export function catalogWorkflowUpstreamBlobUrl(workflow: CatalogWorkflow): string | null {
	const gitRef = catalogWorkflowGitRef(workflow);
	const rawPath = workflow.scm_path?.trim().replace(/^\//, '') ?? '';
	if (!gitRef || !rawPath || !workflow.scm_repository?.trim()) return null;

	const repo = workflow.scm_repository.trim();
	const gl = parseGitlabWebRepository(repo);
	if (gl) {
		return gitlabBlobUrl(gl.origin, gl.projectPath, gitRef, rawPath);
	}

	const gh = parseGithubRepository(repo);
	if (gh) {
		return githubBlobUrl(gh.owner, gh.name, gitRef, rawPath);
	}

	return null;
}
