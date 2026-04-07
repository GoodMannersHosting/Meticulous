<script lang="ts">
	import { Card } from '$components/ui';
	import LineDiffPre from '$components/diff/LineDiffPre.svelte';
	import { apiMethods } from '$api/client';
	import type { JobRun } from '$api/types';
	import { stableStringify } from '$utils/stableStringify';
	import { GitBranch } from 'lucide-svelte';

	let {
		currentRunId,
		previousRunId,
		currentRunLabel,
		previousRunLabel,
		jobRuns,
		prevJobRuns
	}: {
		currentRunId: string;
		previousRunId: string;
		currentRunLabel: string;
		previousRunLabel: string;
		jobRuns: JobRun[];
		prevJobRuns: JobRun[];
	} = $props();

	type Row = {
		name: string;
		kind: 'new' | 'removed' | 'both';
		sourceDiff?: { before: string; after: string };
		pipelineDiff?: { before: string; after: string };
		workflowDiff?: { before: string; after: string };
		pipelineNote?: string;
		workflowNote?: string;
	};

	let rows = $state<Row[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);

	$effect(() => {
		const cr = currentRunId;
		const pr = previousRunId;
		const jr = jobRuns;
		const pjr = prevJobRuns;
		if (!cr || !pr) {
			rows = [];
			loading = false;
			return;
		}

		let cancelled = false;
		loading = true;
		error = null;

		void (async () => {
			try {
				const names = [
					...new Set([...jr.map((j) => j.job_name), ...pjr.map((j) => j.job_name)])
				].sort();
				const out: Row[] = [];

				for (const name of names) {
					if (cancelled) return;
					const c = jr.find((j) => j.job_name === name);
					const p = pjr.find((j) => j.job_name === name);

					if (c && !p) {
						out.push({ name, kind: 'new' });
						continue;
					}
					if (!c && p) {
						out.push({ name, kind: 'removed' });
						continue;
					}
					if (!c || !p) continue;

					const row: Row = { name, kind: 'both' };

					const swC = stableStringify(c.source_workflow ?? null);
					const swP = stableStringify(p.source_workflow ?? null);
					if (swC !== swP) {
						row.sourceDiff = { before: swP, after: swC };
					}

					const samePipe = c.pipeline_definition_sha256 === p.pipeline_definition_sha256;
					const sameWf = c.workflow_definition_sha256 === p.workflow_definition_sha256;

					if (!samePipe || !sameWf) {
						const [snapC, snapP] = await Promise.all([
							apiMethods.runs.jobRunSnapshots(cr, c.id),
							apiMethods.runs.jobRunSnapshots(pr, p.id)
						]);
						if (cancelled) return;

						if (!samePipe) {
							const hasP = snapP.pipeline_definition != null && snapC.pipeline_definition != null;
							if (hasP) {
								row.pipelineDiff = {
									before: stableStringify(snapP.pipeline_definition),
									after: stableStringify(snapC.pipeline_definition)
								};
							} else {
								row.pipelineNote =
									'Pipeline definition SHA differs, but snapshot bodies are missing on one or both runs (often older runs before snapshots were recorded).';
							}
						}

						if (!sameWf) {
							const hasW =
								snapP.workflow_definition != null && snapC.workflow_definition != null;
							if (hasW) {
								row.workflowDiff = {
									before: stableStringify(snapP.workflow_definition),
									after: stableStringify(snapC.workflow_definition)
								};
							} else {
								row.workflowNote =
									'Workflow definition SHA differs, but snapshot bodies are missing on one or both runs.';
							}
						}
					}

					out.push(row);
				}

				if (!cancelled) {
					rows = out;
				}
			} catch (e) {
				if (!cancelled) {
					error = e instanceof Error ? e.message : 'Failed to compare job definitions';
					rows = [];
				}
			} finally {
				if (!cancelled) {
					loading = false;
				}
			}
		})();

		return () => {
			cancelled = true;
		};
	});
</script>

<div class="space-y-4">
	<div class="flex items-center gap-2 text-sm font-medium text-[var(--text-primary)]">
		<GitBranch class="h-4 w-4" />
		Pipeline &amp; workflow (per job)
	</div>

	{#if loading}
		<p class="text-sm text-[var(--text-secondary)]">Loading definition snapshots…</p>
	{:else if error}
		<p class="text-sm text-[var(--color-error-700)]">{error}</p>
	{:else if rows.length === 0}
		<p class="text-sm text-[var(--text-secondary)]">No jobs to compare.</p>
	{:else}
		{#each rows as row (row.name + row.kind)}
			<Card padding="sm">
				<h4 class="mb-2 font-medium text-[var(--text-primary)]">{row.name}</h4>

				{#if row.kind === 'new'}
					<p class="text-sm text-emerald-700 dark:text-emerald-400">New job in {currentRunLabel}.</p>
				{:else if row.kind === 'removed'}
					<p class="text-sm text-rose-700 dark:text-rose-400">
						Job absent in {currentRunLabel} (present in {previousRunLabel}).
					</p>
				{:else}
					{#if row.sourceDiff}
						<div class="mb-4">
							<p class="mb-1 text-xs font-medium uppercase tracking-wide text-[var(--text-tertiary)]">
								Source workflow ref
							</p>
							<LineDiffPre
								beforeLabel={previousRunLabel}
								afterLabel={currentRunLabel}
								beforeText={row.sourceDiff.before}
								afterText={row.sourceDiff.after}
								maxHeightClass="max-h-48"
							/>
						</div>
					{/if}

					{#if row.pipelineDiff}
						<div class="mb-4">
							<p class="mb-1 text-xs font-medium uppercase tracking-wide text-[var(--text-tertiary)]">
								Pipeline definition snapshot
							</p>
							<LineDiffPre
								beforeLabel={previousRunLabel}
								afterLabel={currentRunLabel}
								beforeText={row.pipelineDiff.before}
								afterText={row.pipelineDiff.after}
							/>
						</div>
					{:else if row.pipelineNote}
						<p class="mb-2 text-xs text-[var(--text-secondary)]">{row.pipelineNote}</p>
					{/if}

					{#if row.workflowDiff}
						<div>
							<p class="mb-1 text-xs font-medium uppercase tracking-wide text-[var(--text-tertiary)]">
								Reusable workflow definition snapshot
							</p>
							<LineDiffPre
								beforeLabel={previousRunLabel}
								afterLabel={currentRunLabel}
								beforeText={row.workflowDiff.before}
								afterText={row.workflowDiff.after}
							/>
						</div>
					{:else if row.workflowNote}
						<p class="text-xs text-[var(--text-secondary)]">{row.workflowNote}</p>
					{/if}

					{#if !row.sourceDiff && !row.pipelineDiff && !row.workflowDiff && !row.pipelineNote && !row.workflowNote}
						<p class="text-sm text-[var(--text-tertiary)]">
							No recorded changes for this job (same snapshot hashes and source workflow).
						</p>
					{/if}
				{/if}
			</Card>
		{/each}
	{/if}
</div>
