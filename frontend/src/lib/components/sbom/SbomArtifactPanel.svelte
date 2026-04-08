<script lang="ts">
	import SbomViewer from './SbomViewer.svelte';
	import type { SbomArtifactApi } from '$api/types';
	import { Badge, CopyButton } from '$components/ui';
	import { catalogWorkflowRef } from '$lib/utils/catalogWorkflowRef';
	import { ChevronDown, FileJson } from 'lucide-svelte';

	let { artifact, runId }: { artifact: SbomArtifactApi; runId: string } = $props();

	const workflowRef = $derived(
		artifact.source_workflow && typeof artifact.source_workflow === 'object'
			? catalogWorkflowRef(artifact.source_workflow as Record<string, unknown>)
			: null
	);

	const exportSlug = $derived(artifact.artifact_name || artifact.artifact_id);

	const statusLabel = $derived.by(() => {
		if (artifact.status === 'inline') return 'Loaded';
		if (artifact.status === 'artifact_registered') return 'Registered';
		return artifact.status;
	});
</script>

<details
	class="group rounded-lg border border-[var(--border-primary)] bg-[var(--surface-elevated)]/60 [&_summary::-webkit-details-marker]:hidden"
>
	<summary
		class="flex cursor-pointer list-none items-start gap-2 px-3 py-3 text-left text-sm hover:bg-[var(--bg-secondary)]/80"
	>
		<ChevronDown
			class="mt-0.5 h-4 w-4 shrink-0 text-[var(--text-tertiary)] transition-transform group-open:rotate-180"
			aria-hidden="true"
		/>
		<div class="min-w-0 flex-1 space-y-1">
			<div class="flex flex-wrap items-center gap-2">
				<FileJson class="h-4 w-4 shrink-0 text-[var(--text-tertiary)]" aria-hidden="true" />
				<span class="font-mono text-xs font-medium text-[var(--text-primary)] break-all">
					{artifact.artifact_name}
				</span>
				<Badge variant="outline" size="sm">{artifact.format}</Badge>
				<Badge variant="secondary" size="sm">{statusLabel}</Badge>
			</div>
			<div class="space-y-0.5 text-xs text-[var(--text-secondary)]">
				{#if workflowRef}
					<p>
						<span class="font-medium text-[var(--text-tertiary)]">Workflow</span>
						<span class="text-[var(--text-primary)]"> {workflowRef}</span>
					</p>
				{/if}
				{#if artifact.job_name}
					<p>
						<span class="font-medium text-[var(--text-tertiary)]">Job</span>
						<span class="text-[var(--text-primary)]"> {artifact.job_name}</span>
					</p>
				{/if}
				{#if artifact.step_name}
					<p>
						<span class="font-medium text-[var(--text-tertiary)]">Step</span>
						<span class="text-[var(--text-primary)]"> {artifact.step_name}</span>
					</p>
				{/if}
				<p class="font-mono text-[var(--text-tertiary)] break-all" title={artifact.artifact_path}>
					<span class="font-sans font-medium">Target</span>
					{artifact.artifact_path}
				</p>
				<p class="flex flex-wrap items-center gap-2">
					<span class="font-medium text-[var(--text-tertiary)]">Artifact</span>
					<code class="rounded bg-[var(--bg-secondary)] px-1.5 py-0.5 text-[var(--text-primary)]">
						{artifact.artifact_id}
					</code>
					<CopyButton text={artifact.artifact_id} size="sm" />
				</p>
			</div>
		</div>
	</summary>
	<div class="border-t border-[var(--border-primary)] px-3 py-3">
		{#if artifact.sbom}
			<SbomViewer
				rawDocument={artifact.sbom}
				apiFormat={artifact.format}
				{runId}
				exportArtifactSlug={exportSlug}
			/>
		{:else if artifact.status === 'artifact_registered'}
			<div class="space-y-2 text-sm text-[var(--text-secondary)]">
				<p class="font-medium text-[var(--text-primary)]">SBOM artifact linked, preview not loaded</p>
				<p>
					The run has an artifact whose name or path looks like an SBOM (e.g.
					<span class="font-mono text-xs">sbom.spdx.json</span>), but the API does not yet stream the blob
					into this view. Download it from the run&apos;s artifact list, or store the document under
					<span class="font-mono text-xs">metadata.sbom_json</span> on the artifact row for an inline preview.
				</p>
				<p class="text-xs text-[var(--text-tertiary)]">
					Trivy: <span class="font-mono">trivy fs --format spdx-json --output sbom.spdx.json .</span> then
					upload <span class="font-mono">sbom.spdx.json</span> as a run artifact. Object-store layout (when
					enabled) follows <span class="font-mono">runs/&lt;run_id&gt;/sbom/spdx.json</span>.
				</p>
			</div>
		{:else}
			<p class="text-sm text-[var(--text-secondary)]">No document available for this artifact.</p>
		{/if}
	</div>
</details>
