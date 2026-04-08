<script lang="ts">
	import { Button, Card, Input, Alert, Badge } from '$components/ui';
	import { apiMethods } from '$lib/api';
	import { Search, Radar } from 'lucide-svelte';

	let q = $state('');
	let loading = $state(false);
	let error = $state<string | null>(null);
	let hits = $state<{ kind: string; id: string; project_id?: string; detail?: string }[]>([]);
	let lastQuery = $state('');

	function hitHref(h: { kind: string; id: string; project_id?: string }): string | null {
		switch (h.kind) {
			case 'run':
				return `/runs/${h.id}`;
			case 'pipeline':
				return `/pipelines/${h.id}`;
			case 'project':
				return `/projects/${h.id}`;
			default:
				return null;
		}
	}

	async function search() {
		const term = q.trim();
		if (!term) return;
		loading = true;
		error = null;
		try {
			const res = await apiMethods.security.blastRadius(term);
			lastQuery = res.query;
			hits = res.hits ?? [];
		} catch (e) {
			error = e instanceof Error ? e.message : 'Search failed';
			hits = [];
		} finally {
			loading = false;
		}
	}
</script>

<svelte:head>
	<title>Blast radius | Meticulous</title>
</svelte:head>

<div class="mx-auto max-w-4xl space-y-6">
	<div class="flex items-center gap-3">
		<div class="flex h-10 w-10 items-center justify-center rounded-lg bg-[var(--bg-tertiary)]">
			<Radar class="h-5 w-5 text-[var(--text-secondary)]" />
		</div>
		<div>
			<h1 class="text-2xl font-bold text-[var(--text-primary)]">Blast radius</h1>
			<p class="text-sm text-[var(--text-secondary)]">
				Search runs, pipelines, projects, and agents by id, name, slug, or commit substring. You can also match runs by
				observed <strong>binary path</strong> or <strong>SHA256</strong>, or by outbound <strong>egress IP / CIDR</strong>
				(containment on stored destinations). Text queries must be at least <strong>three characters</strong> unless the
				query is a valid IP or CIDR. Results respect your project access.
			</p>
		</div>
	</div>

	<Card>
		<form
			class="flex flex-col gap-3 p-4 sm:flex-row sm:items-end"
			onsubmit={(e) => {
				e.preventDefault();
				void search();
			}}
		>
			<div class="flex-1">
				<label for="q" class="mb-1 block text-sm font-medium text-[var(--text-primary)]">Query</label>
				<Input
					id="q"
					bind:value={q}
					placeholder="Name, slug, run id, commit, binary path, sha256, or egress IP/CIDR…"
					class="w-full"
				/>
			</div>
			<Button variant="primary" type="submit" loading={loading} disabled={!q.trim()}>
				<Search class="h-4 w-4" />
				Search
			</Button>
		</form>
	</Card>

	{#if error}
		<Alert variant="error" title="Error">{error}</Alert>
	{/if}

	{#if lastQuery}
		<Card>
			<div class="border-b border-[var(--border-primary)] p-4">
				<p class="text-sm text-[var(--text-secondary)]">
					Results for <code class="rounded bg-[var(--bg-tertiary)] px-1">{lastQuery}</code>
					· {hits.length} hit{hits.length === 1 ? '' : 's'}
				</p>
			</div>
			{#if hits.length === 0}
				<p class="p-6 text-sm text-[var(--text-secondary)]">No matches.</p>
			{:else}
				<ul class="divide-y divide-[var(--border-secondary)]">
					{#each hits as h (h.kind + h.id)}
						<li class="flex flex-wrap items-start gap-3 p-4">
							<Badge variant="outline" size="sm">{h.kind}</Badge>
							<div class="min-w-0 flex-1">
								{#if hitHref(h)}
									<a
										href={hitHref(h)!}
										class="break-all text-sm font-mono text-primary-600 hover:underline dark:text-primary-400"
									>
										{h.id}
									</a>
								{:else}
									<code class="break-all text-sm font-mono text-[var(--text-primary)]">{h.id}</code>
								{/if}
								{#if h.project_id}
									<p class="mt-1 text-xs text-[var(--text-tertiary)]">
										Project{' '}
										<a
											href={`/projects/${h.project_id}`}
											class="text-primary-600 hover:underline dark:text-primary-400"
										>{h.project_id}</a>
									</p>
								{/if}
								{#if h.detail}
									<p class="mt-1 text-sm text-[var(--text-secondary)]">{h.detail}</p>
								{/if}
							</div>
						</li>
					{/each}
				</ul>
			{/if}
		</Card>
	{/if}
</div>
