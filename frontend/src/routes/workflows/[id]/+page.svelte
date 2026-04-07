<script lang="ts">
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { auth } from '$stores';
	import { Button, Card, Input, Alert, Badge } from '$components/ui';
	import { DataTable, EmptyState, Skeleton } from '$components/data';
	import { apiMethods } from '$api/client';
	import type { CatalogWorkflow } from '$api/types';
	import { formatRelativeTime } from '$utils/format';
	import { ArrowLeft, RefreshCw, Search, Shield } from 'lucide-svelte';
	import type { Column, SortDirection } from '$components/data/DataTable.svelte';

	const workflowId = $derived($page.params.id);

	let workflow = $state<CatalogWorkflow | null>(null);
	let versions = $state<CatalogWorkflow[]>([]);
	let workflowName = $state<string>('');
	let loading = $state(true);
	let versionsLoading = $state(true);
	let versionsLoadingMore = $state(false);
	let actionLoading = $state(false);
	let error = $state<string | null>(null);
	let versionSearch = $state('');
	let versionSearchApplied = $state('');
	let versionsNextCursor = $state<string | null>(null);
	let sortKey = $state<string | null>('created_at');
	let sortDirection = $state<SortDirection>('desc');

	const isAdmin = $derived(auth.user?.role === 'admin');

	$effect(() => {
		const id = workflowId;
		if (id) void loadAll(id);
	});

	async function loadAll(id: string) {
		loading = true;
		versionsLoading = true;
		error = null;
		try {
			workflow = await apiMethods.wfCatalog.get(id);
			await loadVersions(id, true);
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load workflow';
			workflow = null;
			versions = [];
		} finally {
			loading = false;
			versionsLoading = false;
		}
	}

	async function loadVersions(id: string, reset: boolean) {
		if (reset) {
			versionsLoading = true;
			versionsNextCursor = null;
		} else {
			versionsLoadingMore = true;
		}
		try {
			const res = await apiMethods.wfCatalog.catalogVersions(id, {
				q: versionSearchApplied.trim() || undefined,
				per_page: 40,
				...(reset ? {} : { cursor: versionsNextCursor ?? undefined })
			});
			workflowName = res.workflow_name;
			if (reset) {
				versions = res.versions;
			} else {
				versions = [...versions, ...res.versions];
			}
			versionsNextCursor = res.next_cursor ?? null;
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load versions';
		} finally {
			versionsLoading = false;
			versionsLoadingMore = false;
		}
	}

	function applyVersionSearch() {
		versionSearchApplied = versionSearch;
		if (workflowId) void loadVersions(workflowId, true);
	}

	async function refresh() {
		if (!workflowId) return;
		await loadAll(workflowId);
	}

	async function runAdmin(
		op: 'approve' | 'reject' | 'trust' | 'untrust' | 'delete',
		id: string
	) {
		actionLoading = true;
		error = null;
		try {
			if (op === 'delete') {
				await apiMethods.admin.workflows.delete(id);
				goto('/workflows');
				return;
			}
			const api = apiMethods.admin.workflows;
			const res =
				op === 'approve'
					? await api.approve(id)
					: op === 'reject'
						? await api.reject(id)
						: op === 'trust'
							? await api.trust(id)
							: await api.untrust(id);
			if (workflowId === id) {
				workflow = res.workflow;
			}
			await loadVersions(workflowId!, true);
		} catch (e) {
			error = e instanceof Error ? e.message : 'Action failed';
		} finally {
			actionLoading = false;
		}
	}

	const sortedVersions = $derived.by(() => {
		const data = [...versions];
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

	function handleSort(key: string, direction: SortDirection) {
		if (direction === null) {
			sortKey = null;
			sortDirection = null;
		} else {
			sortKey = key;
			sortDirection = direction;
		}
	}

	const versionColumns: Column<CatalogWorkflow>[] = [
		{
			key: 'version',
			label: 'Version',
			sortable: true,
			render: (v) =>
				`<span class="font-mono text-sm font-medium text-[var(--text-primary)]">${String(v ?? '')}</span>`
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
			key: 'scm_revision',
			label: 'Commit',
			sortable: true,
			render: (v) => {
				const s = String(v ?? '');
				if (!s) return '<span class="text-[var(--text-tertiary)]">—</span>';
				const short = s.length > 10 ? `${s.slice(0, 7)}…` : s;
				return `<span class="font-mono text-xs text-[var(--text-secondary)]" title="${s}">${short}</span>`;
			}
		},
		{
			key: 'created_at',
			label: 'Created',
			sortable: true,
			render: (value) => formatRelativeTime(value as string)
		}
	];

	function handleVersionRowClick(row: CatalogWorkflow) {
		goto(`/workflows/${row.id}`);
	}
</script>

<svelte:head>
	<title>{workflow?.name ?? workflowName ?? 'Workflow'} | Meticulous</title>
</svelte:head>

<div class="mx-auto max-w-5xl space-y-6">
	<div class="flex flex-col gap-4 sm:flex-row sm:items-start sm:justify-between">
		<div class="flex items-start gap-4">
			<Button variant="ghost" size="sm" href="/workflows">
				<ArrowLeft class="h-4 w-4" />
			</Button>
			<div>
				{#if loading && !workflow}
					<Skeleton class="h-8 w-64" />
					<Skeleton class="mt-2 h-4 w-48" />
				{:else if workflow}
					<h1 class="text-2xl font-bold text-[var(--text-primary)]">{workflow.name}</h1>
					<p class="mt-1 text-[var(--text-secondary)]">
						Catalog workflow · v<span class="font-mono">{workflow.version}</span>
						{#if workflow.scm_repository}
							· <span class="font-mono text-sm">{workflow.scm_repository}</span>
							{#if workflow.scm_path}<span class="text-[var(--text-tertiary)]">@{workflow.scm_path}</span>{/if}
						{/if}
					</p>
				{:else}
					<h1 class="text-2xl font-bold text-[var(--text-primary)]">Workflow</h1>
				{/if}
			</div>
		</div>
		<Button variant="outline" size="sm" onclick={refresh} disabled={loading}>
			<RefreshCw class="h-4 w-4" />
			Refresh
		</Button>
	</div>

	{#if error}
		<Alert variant="error" dismissible ondismiss={() => (error = null)}>
			{error}
		</Alert>
	{/if}

	{#if workflow}
		<Card class="space-y-4 p-6">
			<div class="flex flex-wrap gap-2">
				<Badge variant="secondary">Review: {workflow.submission_status}</Badge>
				<Badge variant="secondary">Trust: {workflow.trust_state}</Badge>
				{#if workflow.deprecated}
					<Badge variant="warning">Deprecated</Badge>
				{/if}
			</div>
			{#if workflow.description}
				<p class="text-sm text-[var(--text-secondary)]">{workflow.description}</p>
			{/if}
			<dl class="grid gap-3 text-sm sm:grid-cols-2">
				<div>
					<dt class="text-[var(--text-tertiary)]">Workflow ID</dt>
					<dd class="mt-0.5 font-mono text-[var(--text-primary)]">{workflow.id}</dd>
				</div>
				<div>
					<dt class="text-[var(--text-tertiary)]">Source</dt>
					<dd class="mt-0.5 text-[var(--text-primary)]">{workflow.source}</dd>
				</div>
				<div>
					<dt class="text-[var(--text-tertiary)]">Updated</dt>
					<dd class="mt-0.5 text-[var(--text-primary)]">{formatRelativeTime(workflow.updated_at)}</dd>
				</div>
			</dl>

			{#if isAdmin}
				<div
					class="flex flex-wrap items-center gap-2 border-t border-[var(--border-primary)] pt-4"
				>
					<span class="flex items-center gap-1 text-sm font-medium text-[var(--text-secondary)]">
						<Shield class="h-4 w-4" />
						Admin
					</span>
					<Button
						size="sm"
						variant="outline"
						disabled={actionLoading || workflow.submission_status === 'approved'}
						onclick={() => runAdmin('approve', workflow!.id)}
					>
						Approve
					</Button>
					<Button
						size="sm"
						variant="outline"
						disabled={actionLoading || workflow.submission_status === 'rejected'}
						onclick={() => runAdmin('reject', workflow!.id)}
					>
						Reject
					</Button>
					<Button
						size="sm"
						variant="outline"
						disabled={actionLoading || workflow.trust_state === 'trusted'}
						onclick={() => runAdmin('trust', workflow!.id)}
					>
						Trust
					</Button>
					<Button
						size="sm"
						variant="outline"
						disabled={actionLoading || workflow.trust_state !== 'trusted'}
						onclick={() => runAdmin('untrust', workflow!.id)}
					>
						Untrust
					</Button>
					<Button
						size="sm"
						variant="outline"
						class="border-error-300 text-error-700 dark:border-error-800 dark:text-error-400"
						disabled={actionLoading}
						onclick={() => runAdmin('delete', workflow!.id)}
					>
						Remove from catalog
					</Button>
				</div>
			{/if}
		</Card>
	{/if}

	<div>
		<h2 class="mb-3 text-lg font-semibold text-[var(--text-primary)]">All versions</h2>
		<p class="mb-4 text-sm text-[var(--text-secondary)]">
			Search versions by version string, commit SHA, or description. Click a row to open that version.
		</p>
		<form
			class="mb-4 flex flex-wrap gap-2"
			onsubmit={(e) => {
				e.preventDefault();
				applyVersionSearch();
			}}
		>
			<div class="relative max-w-md flex-1">
				<Search class="absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-[var(--text-tertiary)]" />
				<Input
					type="search"
					placeholder="Search versions…"
					class="pl-10"
					bind:value={versionSearch}
				/>
			</div>
			<Button variant="outline" size="sm" type="submit">Search</Button>
		</form>

		{#if versionsLoading && versions.length === 0}
			<Card>
				<div class="space-y-3 p-4">
					{#each Array(4) as _, i (i)}
						<Skeleton class="h-10 w-full" />
					{/each}
				</div>
			</Card>
		{:else if versions.length === 0}
			<Card>
				<EmptyState title="No versions" description="Try clearing the search filter." />
			</Card>
		{:else}
			<DataTable
				columns={versionColumns}
				data={sortedVersions}
				rowKey="id"
				sortKey={sortKey}
				{sortDirection}
				onSort={handleSort}
				onRowClick={handleVersionRowClick}
			/>
			{#if versionsNextCursor}
				<div class="mt-4 flex justify-center">
					<Button
						variant="outline"
						onclick={() => workflowId && loadVersions(workflowId, false)}
						loading={versionsLoadingMore}
					>
						Load more versions
					</Button>
				</div>
			{/if}
		{/if}
	</div>
</div>
