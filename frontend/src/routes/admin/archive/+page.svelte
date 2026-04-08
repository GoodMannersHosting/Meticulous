<script lang="ts">
	import { Button, Card, Alert } from '$components/ui';
	import { apiMethods } from '$lib/api';
	import { Archive, RotateCcw, Trash2 } from 'lucide-svelte';

	let data = $state<Awaited<ReturnType<typeof apiMethods.admin.archive.list>> | null>(null);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let busyId = $state<string | null>(null);

	$effect(() => {
		void load();
	});

	async function load() {
		loading = true;
		error = null;
		try {
			data = await apiMethods.admin.archive.list();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load archive';
			data = null;
		} finally {
			loading = false;
		}
	}

	async function unarchiveProject(id: string) {
		busyId = `p:${id}`;
		error = null;
		try {
			await apiMethods.admin.archive.unarchiveProject(id);
			await load();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed';
		} finally {
			busyId = null;
		}
	}

	async function unarchivePipeline(id: string) {
		busyId = `pl:${id}`;
		error = null;
		try {
			await apiMethods.admin.archive.unarchivePipeline(id);
			await load();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed';
		} finally {
			busyId = null;
		}
	}

	async function purgePipeline(id: string) {
		if (
			!confirm(
				'Permanently purge this archived pipeline and related data? This cannot be undone.',
			)
		)
			return;
		busyId = `purge:${id}`;
		error = null;
		try {
			await apiMethods.admin.archive.purgePipeline(id);
			await load();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to purge';
		} finally {
			busyId = null;
		}
	}

	async function permanentlyDeleteProject(id: string, name: string) {
		if (
			!confirm(
				`Permanently delete archived project "${name}" and all related data? This cannot be undone.`,
			)
		)
			return;
		busyId = `purge-p:${id}`;
		error = null;
		try {
			await apiMethods.admin.projects.forceDelete(id);
			await load();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to delete project';
		} finally {
			busyId = null;
		}
	}
</script>

<svelte:head>
	<title>Archive | Admin</title>
</svelte:head>

<div class="space-y-6">
	<div class="flex items-center gap-3">
		<div class="flex h-10 w-10 items-center justify-center rounded-lg bg-[var(--bg-tertiary)]">
			<Archive class="h-5 w-5 text-[var(--text-secondary)]" />
		</div>
		<div>
			<h1 class="text-xl font-semibold text-[var(--text-primary)]">Archive</h1>
			<p class="text-sm text-[var(--text-secondary)]">
				Archived projects and pipelines. Organization admins can unarchive or permanently delete from here;
				project administrators archive from the project settings page.
			</p>
		</div>
		<Button variant="outline" size="sm" class="ml-auto" onclick={() => load()} disabled={loading}>
			Refresh
		</Button>
	</div>

	{#if error}
		<Alert variant="error" title="Error">{error}</Alert>
	{/if}

	{#if loading && !data}
		<Card><p class="p-6 text-sm text-[var(--text-secondary)]">Loading…</p></Card>
	{:else if data}
		<Card>
			<h2 class="border-b border-[var(--border-primary)] p-4 font-medium">Archived projects</h2>
			{#if data.projects.length === 0}
				<p class="p-4 text-sm text-[var(--text-secondary)]">None.</p>
			{:else}
				<ul class="divide-y divide-[var(--border-secondary)]">
					{#each data.projects as p (p.id)}
						<li class="flex flex-wrap items-center gap-3 p-4">
							<span class="font-medium">{p.name}</span>
							<span class="text-xs text-[var(--text-tertiary)]">{p.slug}</span>
							<Button
								variant="outline"
								size="sm"
								disabled={busyId === `p:${p.id}` || busyId === `purge-p:${p.id}`}
								onclick={() => unarchiveProject(p.id)}
							>
								<RotateCcw class="h-4 w-4" />
								Unarchive
							</Button>
							<Button
								variant="destructive"
								size="sm"
								disabled={busyId === `p:${p.id}` || busyId === `purge-p:${p.id}`}
								onclick={() => permanentlyDeleteProject(p.id, p.name)}
							>
								<Trash2 class="h-4 w-4" />
								Delete permanently
							</Button>
						</li>
					{/each}
				</ul>
			{/if}
		</Card>

		<Card>
			<h2 class="border-b border-[var(--border-primary)] p-4 font-medium">Archived pipelines</h2>
			{#if data.pipelines.length === 0}
				<p class="p-4 text-sm text-[var(--text-secondary)]">None.</p>
			{:else}
				<ul class="divide-y divide-[var(--border-secondary)]">
					{#each data.pipelines as pl (pl.id)}
						<li class="flex flex-wrap items-center gap-3 p-4">
							<span class="font-medium">{pl.name}</span>
							<span class="text-xs text-[var(--text-tertiary)]">project {pl.project_id}</span>
							<Button
								variant="outline"
								size="sm"
								disabled={busyId === `pl:${pl.id}`}
								onclick={() => unarchivePipeline(pl.id)}
							>
								<RotateCcw class="h-4 w-4" />
								Unarchive
							</Button>
							<Button
								variant="destructive"
								size="sm"
								disabled={busyId === `purge:${pl.id}`}
								onclick={() => purgePipeline(pl.id)}
							>
								<Trash2 class="h-4 w-4" />
								Purge
							</Button>
						</li>
					{/each}
				</ul>
			{/if}
		</Card>
	{/if}
</div>
