<script lang="ts">
	import { Button, Card, Input, Select } from '$components/ui';
	import { DataTable, EmptyState, Skeleton } from '$components/data';
	import { apiMethods } from '$api/client';
	import type { CatalogWorkflow } from '$api/types';
	import { formatRelativeTime } from '$utils/format';
	import { Plus, Search, Layers } from 'lucide-svelte';
	import type { Column, SortDirection } from '$components/data/DataTable.svelte';
	import { goto } from '$app/navigation';

	let workflows = $state<CatalogWorkflow[]>([]);
	let loading = $state(true);
	let loadingMore = $state(false);
	let error = $state<string | null>(null);
	let searchQuery = $state('');
	let statusFilter = $state<string>('');
	let nextCursor = $state<string | null>(null);
	let sortKey = $state<string | null>('name');
	let sortDirection = $state<SortDirection>('asc');

	const statusOptions = [
		{ value: '', label: 'All statuses' },
		{ value: 'pending', label: 'Pending' },
		{ value: 'approved', label: 'Approved' },
		{ value: 'rejected', label: 'Rejected' }
	];

	$effect(() => {
		statusFilter;
		void reloadCatalog();
	});

	async function reloadCatalog() {
		loading = true;
		error = null;
		nextCursor = null;
		try {
			const res = await apiMethods.wfCatalog.list({
				...(statusFilter ? { status: statusFilter } : {}),
				limit: 50
			});
			workflows = res.data;
			nextCursor = res.pagination.next_cursor ?? null;
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load catalog';
			workflows = [];
		} finally {
			loading = false;
		}
	}

	async function loadMore() {
		if (!nextCursor || loadingMore) return;
		loadingMore = true;
		try {
			const res = await apiMethods.wfCatalog.list({
				...(statusFilter ? { status: statusFilter } : {}),
				limit: 50,
				cursor: nextCursor
			});
			workflows = [...workflows, ...res.data];
			nextCursor = res.pagination.next_cursor ?? null;
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load more';
		} finally {
			loadingMore = false;
		}
	}

	function handleSort(key: string, direction: SortDirection) {
		if (direction === null) {
			sortKey = null;
			sortDirection = null;
		} else {
			sortKey = key;
			sortDirection = direction;
		}
	}

	const filteredWorkflows = $derived.by(() => {
		const q = searchQuery.trim().toLowerCase();
		if (!q) return workflows;
		return workflows.filter(
			(w) =>
				w.name.toLowerCase().includes(q) ||
				w.version.toLowerCase().includes(q) ||
				(w.scm_repository?.toLowerCase().includes(q) ?? false)
		);
	});

	const sortedWorkflows = $derived.by(() => {
		const data = [...filteredWorkflows];
		const key = sortKey;
		const dir = sortDirection;
		if (!key || !dir) return data;
		data.sort((a, b) => {
			const av = a[key as keyof CatalogWorkflow];
			const bv = b[key as keyof CatalogWorkflow];
			const as = av == null ? '' : String(av);
			const bs = bv == null ? '' : String(bv);
			const cmp = as.localeCompare(bs);
			return dir === 'asc' ? cmp : -cmp;
		});
		return data;
	});

	const columns: Column<CatalogWorkflow>[] = [
		{
			key: 'name',
			label: 'Workflow',
			sortable: true,
			render: (_, row) => `
				<div class="flex items-center gap-3">
					<div class="flex h-10 w-10 items-center justify-center rounded-lg bg-[var(--bg-tertiary)]">
						<svg class="h-5 w-5 text-[var(--text-secondary)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
							<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 5a1 1 0 011-1h4a1 1 0 011 1v4a1 1 0 01-1 1H5a1 1 0 01-1-1V5zM14 5a1 1 0 011-1h4a1 1 0 011 1v4a1 1 0 01-1 1h-4a1 1 0 01-1-1V5zM4 15a1 1 0 011-1h4a1 1 0 011 1v4a1 1 0 01-1 1H5a1 1 0 01-1-1v-4zM14 15a1 1 0 011-1h4a1 1 0 011 1v4a1 1 0 01-1 1h-4a1 1 0 01-1-1v-4z"/>
						</svg>
					</div>
					<div>
						<div class="font-medium text-[var(--text-primary)]">${row.name}</div>
						<div class="text-sm text-[var(--text-secondary)]">v${row.version}</div>
					</div>
				</div>
			`
		},
		{
			key: 'submission_status',
			label: 'Review',
			sortable: true,
			render: (value) => {
				const s = String(value ?? '');
				const cls =
					s === 'approved'
						? 'bg-success-100 text-success-800 dark:bg-success-900/30 dark:text-success-300'
						: s === 'rejected'
							? 'bg-error-100 text-error-800 dark:bg-error-900/30 dark:text-error-300'
							: 'bg-secondary-100 text-secondary-800 dark:bg-secondary-800 dark:text-secondary-200';
				return `<span class="inline-flex rounded-full px-2 py-0.5 text-xs font-medium ${cls}">${s || '—'}</span>`;
			}
		},
		{
			key: 'trust_state',
			label: 'Trust',
			sortable: true,
			render: (value) => {
				const s = String(value ?? '');
				const cls =
					s === 'trusted'
						? 'bg-success-100 text-success-800 dark:bg-success-900/30 dark:text-success-300'
						: 'bg-warning-100 text-warning-800 dark:bg-warning-900/30 dark:text-warning-300';
				return `<span class="inline-flex rounded-full px-2 py-0.5 text-xs font-medium ${cls}">${s || '—'}</span>`;
			}
		},
		{
			key: 'scm_repository',
			label: 'Source',
			sortable: true,
			render: (_, row) => {
				const repo = row.scm_repository;
				if (!repo) return '<span class="text-[var(--text-tertiary)]">—</span>';
				const path = row.scm_path ? ` @ ${row.scm_path}` : '';
				return `<span class="font-mono text-sm text-[var(--text-secondary)]">${repo}${path}</span>`;
			}
		},
		{
			key: 'updated_at',
			label: 'Updated',
			sortable: true,
			render: (value) => formatRelativeTime(value as string)
		}
	];

	function handleRowClick(row: CatalogWorkflow) {
		goto(`/workflows/${row.id}`);
	}
</script>

<svelte:head>
	<title>Workflow catalog | Meticulous</title>
</svelte:head>

<div class="space-y-6">
	<div class="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
		<div>
			<h1 class="text-2xl font-bold text-[var(--text-primary)]">Workflow catalog</h1>
			<p class="mt-1 text-[var(--text-secondary)]">
				Reusable workflows for your organization. Import from Git; admins approve and set trust.
			</p>
		</div>

		<Button variant="primary" href="/workflows/new">
			<Plus class="h-4 w-4" />
			Import workflow
		</Button>
	</div>

	<div class="flex flex-wrap gap-4">
		<div class="w-56">
			<Select options={statusOptions} bind:value={statusFilter} />
		</div>
		<div class="relative max-w-md flex-1">
			<Search class="absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-[var(--text-tertiary)]" />
			<Input type="search" placeholder="Filter loaded rows…" class="pl-10" bind:value={searchQuery} />
		</div>
	</div>

	{#if error}
		<div
			class="rounded-lg border border-error-200 bg-error-50 p-4 text-sm text-error-700 dark:border-error-800 dark:bg-error-900/20 dark:text-error-400"
		>
			{error}
		</div>
	{/if}

	{#if loading}
		<Card>
			<div class="space-y-4">
				{#each Array(5) as _, i (i)}
					<div class="flex items-center gap-4">
						<Skeleton class="h-10 w-10 rounded-lg" />
						<div class="flex-1 space-y-2">
							<Skeleton class="h-4 w-48" />
							<Skeleton class="h-3 w-32" />
						</div>
						<Skeleton class="h-4 w-20" />
					</div>
				{/each}
			</div>
		</Card>
	{:else if workflows.length === 0}
		<Card>
			<EmptyState
				icon={Layers}
				title="No catalog workflows yet"
				description="Import a reusable workflow YAML from GitHub to submit it for review."
			>
				<Button variant="primary" href="/workflows/new">
					<Plus class="h-4 w-4" />
					Import workflow
				</Button>
			</EmptyState>
		</Card>
	{:else}
		<DataTable
			{columns}
			data={sortedWorkflows}
			rowKey="id"
			sortKey={sortKey}
			{sortDirection}
			onSort={handleSort}
			onRowClick={handleRowClick}
		/>
		{#if nextCursor}
			<div class="flex justify-center">
				<Button variant="outline" onclick={loadMore} loading={loadingMore}>Load more</Button>
			</div>
		{/if}
	{/if}
</div>
