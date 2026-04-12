<script lang="ts">
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { auth } from '$stores';
	import { Button, Card, Input, Alert, Badge, Dialog, Select, MarkdownBlock } from '$components/ui';
	import { DataTable, EmptyState, Skeleton } from '$components/data';
	import { apiMethods } from '$api/client';
	import type {
		CatalogUpstreamRefSearchResponse,
		CatalogWorkflow,
		ModerationEvent,
		WorkflowSyncSchedule,
		WorkspaceStoredSecretListItem
	} from '$api/types';
	import { formatRelativeTime } from '$utils/format';
	import {
		catalogWorkflowGitRef,
		catalogWorkflowUpstreamBlobUrl
	} from '$lib/utils/catalogWorkflowSource';
	import YamlCodeBlock from '$lib/components/code/YamlCodeBlock.svelte';
	import { stringify } from 'yaml';
	import {
		ArrowLeft,
		Calendar,
		CheckCircle,
		Clock,
		ExternalLink,
		GitBranch,
		GitCommit,
		RefreshCw,
		Search,
		Shield,
		Tag,
		XCircle
	} from 'lucide-svelte';
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

	// Moderation events
	let moderationEvents = $state<ModerationEvent[]>([]);
	let moderationEventsLoading = $state(false);

	// Moderation action dialog
	let moderationDialogOpen = $state(false);
	let moderationDialogAction = $state<'approve' | 'reject' | 'trust' | 'untrust' | null>(null);
	let moderationNote = $state('');
	let moderationNotePreview = $state(false);
	let moderationDialogError = $state<string | null>(null);
	let moderationDialogLoading = $state(false);

	// Sync schedule
	let syncSchedule = $state<WorkflowSyncSchedule | null>(null);
	let syncScheduleLoading = $state(false);
	let syncScheduleEditing = $state(false);
	let syncScheduleEnabled = $state(false);
	let syncScheduleInterval = $state(60);
	let syncScheduleSaving = $state(false);
	let syncNowLoading = $state(false);
	let syncScheduleError = $state<string | null>(null);

	// Deprecation
	let deprecationDialogOpen = $state(false);
	let deprecationAfterInput = $state('');
	let deprecationNoteInput = $state('');
	let deprecationDialogLoading = $state(false);
	let deprecationDialogError = $state<string | null>(null);

	const isAdmin = $derived(auth.user?.role === 'admin');
	const isSecurityEngineer = $derived(auth.user?.role === 'security_engineer');
	const isModerator = $derived(isAdmin || isSecurityEngineer);

	const canSyncCatalogGit = $derived(
		Boolean(
			workflow &&
				workflow.source === 'git' &&
				workflow.scm_repository?.trim() &&
				workflow.scm_path?.trim()
		)
	);

	let syncDialogOpen = $state(false);
	let syncGitRef = $state('');
	let syncCommitsRef = $state('');
	let syncCredentialsPath = $state('');
	let syncFilterQ = $state('');
	let upstreamRefData = $state<CatalogUpstreamRefSearchResponse | null>(null);
	let refSearchLoading = $state(false);
	let syncDialogError = $state<string | null>(null);
	let syncImportLoading = $state(false);
	let orgSecretsLoading = $state(false);
	let orgSecrets = $state<WorkspaceStoredSecretListItem[]>([]);
	let refSearchTimer: ReturnType<typeof setTimeout> | null = null;

	const syncCredentialOptions = $derived([
		{ value: '', label: 'Select GitHub App secret…' },
		...orgSecrets
			.filter((s) => s.kind === 'github_app')
			.map((s) => ({ value: s.path, label: `${s.path} (github_app)` }))
	]);

	function catalogStoredCredPath(w: CatalogWorkflow): string {
		const p = w.catalog_metadata?.catalog_scm_credentials_path;
		return typeof p === 'string' ? p : '';
	}

	async function ensureOrgSecretsForSync() {
		if (orgSecrets.length > 0 || orgSecretsLoading) return;
		orgSecretsLoading = true;
		syncDialogError = null;
		try {
			const acc: WorkspaceStoredSecretListItem[] = [];
			let cursor: string | undefined;
			do {
				const r = await apiMethods.workspaceConfig.listStoredSecrets({
					scope_level: 'organization',
					per_page: 100,
					...(cursor ? { cursor } : {})
				});
				acc.push(...r.data);
				cursor = r.pagination.has_more ? r.pagination.next_cursor : undefined;
			} while (cursor);
			orgSecrets = acc;
		} catch (e) {
			syncDialogError =
				e instanceof Error ? e.message : 'Failed to load organization stored secrets';
			orgSecrets = [];
		} finally {
			orgSecretsLoading = false;
		}
	}

	async function openSyncDialog() {
		if (!workflow || !canSyncCatalogGit) return;
		await ensureOrgSecretsForSync();
		syncGitRef = workflow.scm_ref?.trim() || 'main';
		syncCommitsRef = workflow.scm_ref?.trim() || 'main';
		syncCredentialsPath = catalogStoredCredPath(workflow) || '';
		syncFilterQ = '';
		upstreamRefData = null;
		syncDialogError = null;
		syncDialogOpen = true;
		if (syncCredentialsPath.trim()) {
			await fetchUpstreamRefs();
		}
	}

	async function fetchUpstreamRefs() {
		if (!workflow?.scm_repository?.trim()) return;
		if (!syncCredentialsPath.trim()) {
			syncDialogError = 'Choose a GitHub App credential to load branches and tags from GitHub.';
			return;
		}
		refSearchLoading = true;
		syncDialogError = null;
		try {
			upstreamRefData = await apiMethods.wfCatalog.upstreamRefSearchOrganization({
				repository: workflow.scm_repository.trim(),
				credentials_path: syncCredentialsPath.trim(),
				q: syncFilterQ.trim() || undefined,
				commits_for_ref: syncCommitsRef.trim() || undefined
			});
		} catch (e) {
			syncDialogError = e instanceof Error ? e.message : 'Failed to load refs from GitHub';
			upstreamRefData = null;
		} finally {
			refSearchLoading = false;
		}
	}

	function scheduleUpstreamRefSearch() {
		if (refSearchTimer) clearTimeout(refSearchTimer);
		refSearchTimer = setTimeout(() => {
			refSearchTimer = null;
			void fetchUpstreamRefs();
		}, 320);
	}

	async function submitSyncNewVersion() {
		if (!workflow?.scm_repository?.trim() || !workflow.scm_path?.trim()) return;
		if (!syncGitRef.trim() || !syncCredentialsPath.trim()) {
			syncDialogError = 'Git ref and credential are required.';
			return;
		}
		syncImportLoading = true;
		syncDialogError = null;
		try {
			const wf = await apiMethods.wfCatalog.importGitOrganization({
				repository: workflow.scm_repository.trim(),
				git_ref: syncGitRef.trim(),
				workflow_path: workflow.scm_path.trim(),
				credentials_path: syncCredentialsPath.trim()
			});
			syncDialogOpen = false;
			await goto(`/workflows/${wf.id}`);
		} catch (e) {
			syncDialogError = e instanceof Error ? e.message : 'Import failed';
		} finally {
			syncImportLoading = false;
		}
	}

	function shortSha(sha: string) {
		return sha.length > 10 ? `${sha.slice(0, 7)}…` : sha;
	}

	const workflowYamlSource = $derived.by(() => {
		if (!workflow?.definition) return '';
		try {
			return stringify(workflow.definition, { indent: 2, lineWidth: 120 });
		} catch {
			return JSON.stringify(workflow.definition, null, 2);
		}
	});

	const sourceViewUrl = $derived(workflow ? catalogWorkflowUpstreamBlobUrl(workflow) : null);

	const sourceRefLabel = $derived.by(() => {
		if (!workflow) return '';
		const sha = workflow.scm_revision?.trim();
		if (sha) return sha.length > 12 ? `${sha.slice(0, 7)}…` : sha;
		const r = workflow.scm_ref?.trim();
		return r ?? '';
	});

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
			await Promise.all([
				loadVersions(id, true),
				loadModerationEvents(id),
				loadSyncSchedule(workflow?.name ?? '')
			]);
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

	async function loadModerationEvents(id: string) {
		if (!isModerator) return;
		moderationEventsLoading = true;
		try {
			const res = await apiMethods.admin.workflows.getModerationEvents(id);
			moderationEvents = res.events;
		} catch {
			moderationEvents = [];
		} finally {
			moderationEventsLoading = false;
		}
	}

	async function loadSyncSchedule(name: string) {
		if (!isAdmin || !name) return;
		syncScheduleLoading = true;
		try {
			const res = await apiMethods.wfCatalog.getWorkflowSyncSchedule(name);
			syncSchedule = res;
			if (res) {
				syncScheduleEnabled = res.enabled;
				syncScheduleInterval = res.interval_minutes;
			}
		} catch {
			syncSchedule = null;
		} finally {
			syncScheduleLoading = false;
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

	function openModerationDialog(op: 'approve' | 'reject' | 'trust' | 'untrust') {
		moderationDialogAction = op;
		moderationNote = '';
		moderationNotePreview = false;
		moderationDialogError = null;
		moderationDialogOpen = true;
	}

	async function submitModerationAction() {
		if (!moderationDialogAction || !workflow) return;
		if (isSecurityEngineer && !moderationNote.trim()) {
			moderationDialogError = 'Security engineers must provide a note for this action.';
			return;
		}
		moderationDialogLoading = true;
		moderationDialogError = null;
		try {
			const note = moderationNote.trim() || undefined;
			const api = apiMethods.admin.workflows;
			const res =
				moderationDialogAction === 'approve'
					? await api.approve(workflow.id, note)
					: moderationDialogAction === 'reject'
						? await api.reject(workflow.id, note)
						: moderationDialogAction === 'trust'
							? await api.trust(workflow.id, note)
							: await api.untrust(workflow.id, note);
			workflow = res.workflow;
			moderationDialogOpen = false;
			await Promise.all([loadVersions(workflowId!, true), loadModerationEvents(workflowId!)]);
		} catch (e) {
			moderationDialogError = e instanceof Error ? e.message : 'Action failed';
		} finally {
			moderationDialogLoading = false;
		}
	}

	async function runDelete() {
		if (!workflow) return;
		if (!confirm('Remove this workflow version from the catalog? This cannot be undone.')) return;
		actionLoading = true;
		error = null;
		try {
			await apiMethods.admin.workflows.delete(workflow.id);
			goto('/workflows');
		} catch (e) {
			error = e instanceof Error ? e.message : 'Delete failed';
			actionLoading = false;
		}
	}

	async function saveSyncSchedule() {
		if (!workflow?.name) return;
		syncScheduleSaving = true;
		syncScheduleError = null;
		try {
			const res = await apiMethods.wfCatalog.putWorkflowSyncSchedule(workflow.name, {
				enabled: syncScheduleEnabled,
				interval_minutes: syncScheduleInterval
			});
			syncSchedule = res;
			syncScheduleEditing = false;
		} catch (e) {
			syncScheduleError = e instanceof Error ? e.message : 'Failed to save sync schedule';
		} finally {
			syncScheduleSaving = false;
		}
	}

	async function triggerSyncNow() {
		if (!workflow?.name) return;
		syncNowLoading = true;
		syncScheduleError = null;
		try {
			await apiMethods.wfCatalog.syncNow(workflow.name);
			await loadSyncSchedule(workflow.name);
		} catch (e) {
			syncScheduleError = e instanceof Error ? e.message : 'Sync failed';
		} finally {
			syncNowLoading = false;
		}
	}

	function openDeprecationDialog() {
		if (!workflow) return;
		deprecationAfterInput = workflow.deprecated_after
			? workflow.deprecated_after.slice(0, 16)
			: '';
		deprecationNoteInput = workflow.deprecation_note ?? '';
		deprecationDialogError = null;
		deprecationDialogOpen = true;
	}

	async function submitDeprecation() {
		if (!workflow) return;
		deprecationDialogLoading = true;
		deprecationDialogError = null;
		try {
			const res = await apiMethods.admin.workflows.setDeprecation(workflow.id, {
				deprecated_after: deprecationAfterInput ? new Date(deprecationAfterInput).toISOString() : null,
				deprecation_note: deprecationNoteInput.trim() || undefined
			});
			workflow = res.workflow;
			deprecationDialogOpen = false;
		} catch (e) {
			deprecationDialogError = e instanceof Error ? e.message : 'Failed to set deprecation';
		} finally {
			deprecationDialogLoading = false;
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

	const deprecationState = $derived.by((): 'none' | 'warning' | 'blocked' => {
		if (!workflow?.deprecated_after) return 'none';
		const d = new Date(workflow.deprecated_after);
		if (isNaN(d.getTime())) return 'none';
		return d <= new Date() ? 'blocked' : 'warning';
	});

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
			key: 'deprecated_after',
			label: 'Deprecation',
			sortable: true,
			render: (value, row) => {
				if (!value) {
					return row.deprecated
						? `<span class="inline-flex rounded-full px-2 py-0.5 text-xs font-medium bg-warning-100 text-warning-800 dark:bg-warning-900/30 dark:text-warning-300">deprecated</span>`
						: '<span class="text-[var(--text-tertiary)]">—</span>';
				}
				const d = new Date(String(value));
				const blocked = d <= new Date();
				const cls = blocked
					? 'bg-error-100 text-error-800 dark:bg-error-900/30 dark:text-error-300'
					: 'bg-warning-100 text-warning-800 dark:bg-warning-900/30 dark:text-warning-300';
				const label = blocked ? 'blocked' : 'deprecating';
				const date = d.toLocaleDateString();
				return `<span class="inline-flex rounded-full px-2 py-0.5 text-xs font-medium ${cls}" title="After ${date}">${label}</span>`;
			}
		},
		{
			key: 'scm_revision',
			label: 'Commit',
			sortable: true,
			render: (v, row) => {
				const s = String(v ?? '');
				if (!s) return '<span class="text-[var(--text-tertiary)]">—</span>';
				const short = s.length > 10 ? `${s.slice(0, 7)}…` : s;
				const url = catalogWorkflowUpstreamBlobUrl(row);
				const ref = catalogWorkflowGitRef(row);
				const titleAttr = ref ? `${s} @ ${ref}` : s;
				if (url) {
					const safeUrl = url.replace(/"/g, '&quot;');
					const safeTitle = titleAttr.replace(/"/g, '&quot;');
					return `<a href="${safeUrl}" target="_blank" rel="noopener noreferrer" onclick="event.stopPropagation()" class="font-mono text-xs text-primary-600 hover:underline dark:text-primary-400" title="${safeTitle}">${short}</a>`;
				}
				return `<span class="font-mono text-xs text-[var(--text-secondary)]" title="${titleAttr.replace(/"/g, '&quot;')}">${short}</span>`;
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

	function moderationActionLabel(action: string) {
		return { approve: 'Approve', reject: 'Reject', trust: 'Trust', untrust: 'Untrust' }[action] ?? action;
	}

	function moderationActionIcon(action: string): 'check' | 'x' | 'shield' | 'minus' {
		return { approve: 'check', reject: 'x', trust: 'shield', untrust: 'minus' }[action] as 'check' | 'x' | 'shield' | 'minus';
	}

	const intervalOptions = [
		{ value: '0', label: 'Disabled' },
		{ value: '15', label: 'Every 15 minutes' },
		{ value: '30', label: 'Every 30 minutes' },
		{ value: '60', label: 'Every hour' },
		{ value: '360', label: 'Every 6 hours' },
		{ value: '720', label: 'Every 12 hours' },
		{ value: '1440', label: 'Daily' }
	];
</script>

<svelte:head>
	<title>{workflow?.name ?? workflowName ?? 'Workflow'} | Meticulous</title>
</svelte:head>

<div class="mx-auto max-w-6xl space-y-6">
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
		<div class="flex flex-wrap items-center gap-2">
			{#if isAdmin && canSyncCatalogGit}
				<Button variant="primary" size="sm" onclick={() => void openSyncDialog()}>
					<GitBranch class="h-4 w-4" />
					Sync new version
				</Button>
			{/if}
			<Button variant="outline" size="sm" onclick={refresh} disabled={loading}>
				<RefreshCw class="h-4 w-4" />
				Refresh
			</Button>
		</div>
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
				{#if deprecationState === 'blocked'}
					<Badge variant="error">Blocked (deprecated)</Badge>
				{:else if deprecationState === 'warning'}
					<Badge variant="warning">Deprecating {formatRelativeTime(workflow.deprecated_after!)}</Badge>
				{:else if workflow.deprecated}
					<Badge variant="warning">Deprecated</Badge>
				{/if}
			</div>

			{#if workflow.deprecation_note && deprecationState !== 'none'}
				<div class="rounded-lg border border-warning-200 bg-warning-50 p-3 dark:border-warning-800 dark:bg-warning-900/20">
					<p class="mb-1 text-xs font-semibold text-warning-800 dark:text-warning-300">Deprecation note</p>
					<MarkdownBlock source={workflow.deprecation_note} />
				</div>
			{/if}

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
				{#if workflow.deprecated_after}
					<div>
						<dt class="text-[var(--text-tertiary)]">Deprecated after</dt>
						<dd class="mt-0.5 text-[var(--text-primary)]">
							{new Date(workflow.deprecated_after).toLocaleString()}
						</dd>
					</div>
				{/if}
			</dl>

			<!-- Moderation panel (admin or security_engineer) -->
			{#if isModerator}
				<div
					class="flex flex-wrap items-center gap-2 border-t border-[var(--border-primary)] pt-4"
				>
					<span class="flex items-center gap-1 text-sm font-medium text-[var(--text-secondary)]">
						<Shield class="h-4 w-4" />
						{isSecurityEngineer ? 'Security' : 'Admin'}
					</span>
					<Button
						size="sm"
						variant="outline"
						disabled={actionLoading || workflow.submission_status === 'approved'}
						onclick={() => openModerationDialog('approve')}
					>
						Approve
					</Button>
					<Button
						size="sm"
						variant="outline"
						disabled={actionLoading || workflow.submission_status === 'rejected'}
						onclick={() => openModerationDialog('reject')}
					>
						Reject
					</Button>
					<Button
						size="sm"
						variant="outline"
						disabled={actionLoading || workflow.trust_state === 'trusted'}
						onclick={() => openModerationDialog('trust')}
					>
						Trust
					</Button>
					<Button
						size="sm"
						variant="outline"
						disabled={actionLoading || workflow.trust_state !== 'trusted'}
						onclick={() => openModerationDialog('untrust')}
					>
						Untrust
					</Button>
					{#if isModerator}
						<Button
							size="sm"
							variant="outline"
							onclick={openDeprecationDialog}
						>
							<Calendar class="h-4 w-4" />
							{workflow.deprecated_after ? 'Edit deprecation' : 'Set deprecation'}
						</Button>
					{/if}
					{#if isAdmin}
						<Button
							size="sm"
							variant="outline"
							class="border-error-300 text-error-700 dark:border-error-800 dark:text-error-400"
							disabled={actionLoading}
							onclick={runDelete}
						>
							Remove from catalog
						</Button>
					{/if}
				</div>
			{/if}
		</Card>

		<!-- Sync schedule panel (admin only) -->
		{#if isAdmin}
			<Card class="space-y-4 p-6">
				<div class="flex items-center justify-between">
					<div>
						<h2 class="text-lg font-semibold text-[var(--text-primary)]">Auto-sync schedule</h2>
						<p class="mt-0.5 text-xs text-[var(--text-tertiary)]">
							Automatically re-import this workflow from source on a schedule.
						</p>
					</div>
					<div class="flex items-center gap-2">
						{#if syncSchedule && !syncScheduleEditing}
							<Button size="sm" variant="outline" onclick={triggerSyncNow} loading={syncNowLoading}>
								<RefreshCw class="h-4 w-4" />
								Sync now
							</Button>
						{/if}
						{#if !syncScheduleEditing}
							<Button size="sm" variant="outline" onclick={() => (syncScheduleEditing = true)}>
								{syncSchedule ? 'Edit schedule' : 'Configure'}
							</Button>
						{/if}
					</div>
				</div>

				{#if syncScheduleError}
					<Alert variant="error" dismissible ondismiss={() => (syncScheduleError = null)}>
						{syncScheduleError}
					</Alert>
				{/if}

				{#if syncScheduleLoading}
					<Skeleton class="h-6 w-48" />
				{:else if syncScheduleEditing}
					<div class="space-y-4 rounded-lg border border-[var(--border-primary)] bg-[var(--bg-tertiary)] p-4">
						<div class="flex items-center gap-3">
							<input
								type="checkbox"
								id="sync-enabled"
								class="h-4 w-4 rounded border-[var(--border-primary)]"
								bind:checked={syncScheduleEnabled}
							/>
							<label for="sync-enabled" class="text-sm font-medium text-[var(--text-primary)]">
								Enable auto-sync
							</label>
						</div>
						{#if syncScheduleEnabled}
							<div>
								<label for="sync-interval" class="block text-sm font-medium text-[var(--text-primary)]">
									Sync interval
								</label>
								<select
									id="sync-interval"
									bind:value={syncScheduleInterval}
									class="mt-1 block w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm text-[var(--text-primary)] focus:outline-none focus:ring-2 focus:ring-primary-500"
								>
									{#each intervalOptions.filter((o) => o.value !== '0') as opt}
										<option value={parseInt(opt.value)}>{opt.label}</option>
									{/each}
								</select>
							</div>
						{/if}
						<div class="flex gap-2 pt-2">
							<Button size="sm" variant="primary" onclick={saveSyncSchedule} loading={syncScheduleSaving}>
								Save
							</Button>
							<Button size="sm" variant="outline" onclick={() => (syncScheduleEditing = false)}>
								Cancel
							</Button>
						</div>
					</div>
				{:else if syncSchedule}
					<dl class="grid gap-3 text-sm sm:grid-cols-3">
						<div>
							<dt class="text-[var(--text-tertiary)]">Status</dt>
							<dd class="mt-0.5">
								<span class="inline-flex items-center gap-1 {syncSchedule.enabled ? 'text-success-700 dark:text-success-400' : 'text-[var(--text-tertiary)]'}">
									{#if syncSchedule.enabled}
										<CheckCircle class="h-3.5 w-3.5" />
										Enabled
									{:else}
										<XCircle class="h-3.5 w-3.5" />
										Disabled
									{/if}
								</span>
							</dd>
						</div>
						<div>
							<dt class="text-[var(--text-tertiary)]">Interval</dt>
							<dd class="mt-0.5 text-[var(--text-primary)]">
								{syncSchedule.interval_minutes === 0 ? 'N/A' : `Every ${syncSchedule.interval_minutes} min`}
							</dd>
						</div>
						<div>
							<dt class="text-[var(--text-tertiary)]">Last synced</dt>
							<dd class="mt-0.5 text-[var(--text-primary)]">
								{syncSchedule.last_synced_at ? formatRelativeTime(syncSchedule.last_synced_at) : '—'}
							</dd>
						</div>
					</dl>
				{:else}
					<p class="text-sm text-[var(--text-tertiary)]">No sync schedule configured.</p>
				{/if}
			</Card>
		{/if}

		<Card class="space-y-4 p-6">
			<div class="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
				<div>
					<h2 class="text-lg font-semibold text-[var(--text-primary)]">Workflow definition</h2>
					<p class="mt-1 text-xs text-[var(--text-tertiary)]">
						Pretty-printed YAML from the catalog. Open the host to see this file at the exact revision and path.
					</p>
				</div>
				{#if sourceViewUrl}
					<a
						href={sourceViewUrl}
						target="_blank"
						rel="noopener noreferrer"
						class="inline-flex h-8 shrink-0 items-center justify-center gap-2 rounded-lg border border-secondary-300 bg-transparent px-3 text-sm font-medium text-secondary-700 transition-colors duration-150 hover:bg-secondary-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-secondary-500 focus-visible:ring-offset-2 dark:border-secondary-600 dark:text-secondary-300 dark:hover:bg-secondary-800"
					>
						<ExternalLink class="h-4 w-4" aria-hidden="true" />
						View source{#if sourceRefLabel}<span class="font-mono text-xs opacity-90"> · {sourceRefLabel}</span>{/if}
					</a>
				{:else if workflow.scm_repository && workflow.scm_path}
					<p class="max-w-sm text-xs text-[var(--text-tertiary)]">
						Source deep link is unavailable (unsupported Git host or missing ref/path).
					</p>
				{/if}
			</div>
			<YamlCodeBlock source={workflowYamlSource} />
		</Card>

		<!-- Moderation events timeline -->
		{#if isModerator}
			<Card class="space-y-4 p-6">
				<h2 class="text-lg font-semibold text-[var(--text-primary)]">Moderation history</h2>
				{#if moderationEventsLoading}
					<div class="space-y-3">
						{#each Array(3) as _, i (i)}
							<Skeleton class="h-12 w-full" />
						{/each}
					</div>
				{:else if moderationEvents.length === 0}
					<p class="text-sm text-[var(--text-tertiary)]">No moderation actions recorded.</p>
				{:else}
					<ol class="relative border-l border-[var(--border-primary)] pl-5 space-y-5">
						{#each moderationEvents as ev (ev.id)}
							{@const isGood = ev.action === 'approve' || ev.action === 'trust'}
							{@const isBad = ev.action === 'reject' || ev.action === 'untrust' || ev.action === 'delete'}
							<li class="relative">
								<span
									class="absolute -left-[1.125rem] flex h-5 w-5 items-center justify-center rounded-full ring-2 ring-[var(--bg-secondary)] {isGood
										? 'bg-success-100 dark:bg-success-900/30'
										: isBad
											? 'bg-error-100 dark:bg-error-900/30'
											: 'bg-secondary-100 dark:bg-secondary-800'}"
								>
									{#if isGood}
										<CheckCircle class="h-3.5 w-3.5 text-success-600 dark:text-success-400" />
									{:else if isBad}
										<XCircle class="h-3.5 w-3.5 text-error-600 dark:text-error-400" />
									{:else}
										<Clock class="h-3.5 w-3.5 text-[var(--text-tertiary)]" />
									{/if}
								</span>
								<div>
									<p class="text-sm font-medium text-[var(--text-primary)]">
										<span class="capitalize">{ev.action}</span>
										<span class="ml-2 text-xs font-normal text-[var(--text-tertiary)]">
											{formatRelativeTime(ev.created_at)}
										</span>
									</p>
									{#if ev.note}
										<div class="mt-1.5 rounded-lg border border-[var(--border-primary)] bg-[var(--bg-tertiary)] px-3 py-2">
											<MarkdownBlock source={ev.note} />
										</div>
									{/if}
								</div>
							</li>
						{/each}
					</ol>
				{/if}
			</Card>
		{/if}
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

<!-- Moderation action dialog -->
{#if isModerator}
	<Dialog
		bind:open={moderationDialogOpen}
		title="{moderationDialogAction ? moderationActionLabel(moderationDialogAction) : ''} workflow"
		description="Optionally add a markdown note explaining your decision. Security engineers must provide a note."
	>
		{#if moderationDialogAction}
			<div class="space-y-4">
				{#if moderationDialogError}
					<Alert variant="error" dismissible ondismiss={() => (moderationDialogError = null)}>
						{moderationDialogError}
					</Alert>
				{/if}

				<div>
					<div class="mb-1.5 flex items-center justify-between">
						<label for="mod-note" class="block text-sm font-medium text-[var(--text-primary)]">
							Note {isSecurityEngineer ? '(required)' : '(optional)'}
						</label>
						<button
							type="button"
							class="text-xs text-primary-600 hover:underline dark:text-primary-400"
							onclick={() => (moderationNotePreview = !moderationNotePreview)}
						>
							{moderationNotePreview ? 'Edit' : 'Preview'}
						</button>
					</div>
					{#if moderationNotePreview}
						<div class="min-h-[6rem] rounded-lg border border-[var(--border-primary)] bg-[var(--bg-tertiary)] p-3">
							{#if moderationNote.trim()}
								<MarkdownBlock source={moderationNote} />
							{:else}
								<p class="text-sm text-[var(--text-tertiary)]">Nothing to preview.</p>
							{/if}
						</div>
					{:else}
						<textarea
							id="mod-note"
							bind:value={moderationNote}
							rows={6}
							placeholder="Write your note in markdown…"
							class="w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 font-mono text-sm text-[var(--text-primary)] placeholder-[var(--text-tertiary)] focus:outline-none focus:ring-2 focus:ring-primary-500"
						></textarea>
						<p class="mt-1 text-xs text-[var(--text-tertiary)]">Supports markdown (bold, lists, code blocks, etc.)</p>
					{/if}
				</div>

				<div class="flex justify-end gap-2 pt-2">
					<Button variant="outline" onclick={() => (moderationDialogOpen = false)}>Cancel</Button>
					<Button
						variant="primary"
						loading={moderationDialogLoading}
						onclick={() => void submitModerationAction()}
					>
						{moderationActionLabel(moderationDialogAction)}
					</Button>
				</div>
			</div>
		{/if}
	</Dialog>
{/if}

<!-- Set deprecation dialog -->
{#if isModerator}
	<Dialog
		bind:open={deprecationDialogOpen}
		title="Set deprecation period"
		description="Pipelines that use this version will receive a warning before the date, and will be blocked after it."
	>
		<div class="space-y-4">
			{#if deprecationDialogError}
				<Alert variant="error" dismissible ondismiss={() => (deprecationDialogError = null)}>
					{deprecationDialogError}
				</Alert>
			{/if}

			<div>
				<label for="deprecated-after" class="block text-sm font-medium text-[var(--text-primary)]">
					Block pipelines after
				</label>
				<input
					id="deprecated-after"
					type="datetime-local"
					bind:value={deprecationAfterInput}
					class="mt-1 block w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm text-[var(--text-primary)] focus:outline-none focus:ring-2 focus:ring-primary-500"
				/>
				<p class="mt-1 text-xs text-[var(--text-tertiary)]">
					Leave blank to remove the deprecation period (or set as a simple deprecated flag only).
				</p>
			</div>

			<div>
				<label for="deprecation-note" class="block text-sm font-medium text-[var(--text-primary)]">
					Deprecation note (optional, markdown)
				</label>
				<textarea
					id="deprecation-note"
					bind:value={deprecationNoteInput}
					rows={4}
					placeholder="Explain why this version is being deprecated and what to migrate to…"
					class="mt-1 w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 font-mono text-sm text-[var(--text-primary)] placeholder-[var(--text-tertiary)] focus:outline-none focus:ring-2 focus:ring-primary-500"
				></textarea>
			</div>

			<div class="flex justify-end gap-2 pt-2">
				<Button variant="outline" onclick={() => (deprecationDialogOpen = false)}>Cancel</Button>
				<Button
					variant="primary"
					loading={deprecationDialogLoading}
					onclick={() => void submitDeprecation()}
				>
					<Calendar class="h-4 w-4" />
					Save deprecation
				</Button>
			</div>
		</div>
	</Dialog>
{/if}

<!-- "Sync new catalog version" dialog (admin only) -->
{#if isAdmin}
	<Dialog
		bind:open={syncDialogOpen}
		title="Sync new catalog version"
		description="Fetch branches, tags, and recent commits from GitHub, pick a ref, then import the same workflow path as a new catalog row (same semver in YAML updates the existing version; a new semver adds another row). Requires an organization GitHub App secret and org admin."
		maxWidthClass="max-w-[min(1320px,calc(100vw-2rem))]"
		class="max-h-[90vh] overflow-hidden"
	>
		{#if workflow && canSyncCatalogGit}
			<div
				class="flex min-h-0 flex-1 flex-col gap-4 text-sm text-[var(--text-secondary)] lg:max-h-[min(78vh,720px)] lg:min-h-[280px]"
			>
				<div
					class="grid min-h-0 flex-1 grid-cols-1 gap-6 lg:grid-cols-[minmax(260px,320px)_minmax(0,1fr)] lg:items-stretch"
				>
					<div class="min-h-0 overflow-y-auto overscroll-y-contain lg:max-h-full lg:pr-1">
						<div class="space-y-4">
							{#if syncDialogError}
								<Alert variant="error" dismissible ondismiss={() => (syncDialogError = null)}>
									{syncDialogError}
								</Alert>
							{/if}

							<div class="space-y-1 rounded-lg border border-[var(--border-primary)] bg-[var(--bg-tertiary)] p-3">
								<p>
									<span class="text-[var(--text-tertiary)]">Repository</span><br />
									<span class="font-mono text-[var(--text-primary)]">{workflow.scm_repository}</span>
								</p>
								<p class="mt-2">
									<span class="text-[var(--text-tertiary)]">Workflow path</span><br />
									<span class="font-mono text-[var(--text-primary)]">{workflow.scm_path}</span>
								</p>
							</div>

							<div>
								<label
									for="sync-dialog-cred"
									class="mb-1 block text-sm font-medium text-[var(--text-primary)]"
									>GitHub App credential</label
								>
								<Select
									id="sync-dialog-cred"
									options={syncCredentialOptions}
									bind:value={syncCredentialsPath}
									disabled={orgSecretsLoading}
									class="w-full"
									onchange={() => void fetchUpstreamRefs()}
								/>
								{#if !orgSecretsLoading && syncCredentialOptions.length <= 1}
									<p class="mt-1 text-xs text-amber-700 dark:text-amber-400">
										Add an organization-scoped GitHub App secret under Secrets &amp; Variables.
									</p>
								{/if}
							</div>

							<div class="space-y-3">
								<div>
									<label
										for="sync-dialog-git-ref"
										class="mb-1 block text-sm font-medium text-[var(--text-primary)]"
										>Git ref to import</label
									>
									<Input
										id="sync-dialog-git-ref"
										bind:value={syncGitRef}
										placeholder="branch, tag, or full SHA"
										class="font-mono"
									/>
									<p class="mt-1 text-xs text-[var(--text-tertiary)]">
										Sent to the import API (resolved to a commit on the server).
									</p>
								</div>
								<div>
									<label
										for="sync-dialog-commits-ref"
										class="mb-1 block text-sm font-medium text-[var(--text-primary)]"
										>Load commits from ref</label
									>
									<Input
										id="sync-dialog-commits-ref"
										bind:value={syncCommitsRef}
										placeholder="e.g. main"
										class="font-mono"
									/>
									<p class="mt-1 text-xs text-[var(--text-tertiary)]">
										Populates the commit list on the right (does not change the import ref unless you pick a
										row).
									</p>
								</div>
							</div>
						</div>
					</div>

					<div
						class="flex min-h-0 min-w-0 flex-col gap-3 border-t border-[var(--border-primary)] pt-4 lg:border-l lg:border-t-0 lg:pl-5 lg:pt-0"
					>
						<div class="shrink-0">
							<label
								for="sync-dialog-filter"
								class="mb-1 block text-sm font-medium text-[var(--text-primary)]"
								>Filter branches, tags, commits</label
							>
							<div class="flex flex-wrap gap-2">
								<Input
									id="sync-dialog-filter"
									bind:value={syncFilterQ}
									placeholder="Type to filter…"
									class="min-w-[10rem] flex-1"
									oninput={() => scheduleUpstreamRefSearch()}
								/>
								<Button
									type="button"
									variant="outline"
									size="sm"
									onclick={() => void fetchUpstreamRefs()}
									loading={refSearchLoading}
									disabled={!syncCredentialsPath.trim()}
								>
									Refresh
								</Button>
							</div>
						</div>

						{#if refSearchLoading && !upstreamRefData}
							<div class="flex min-h-0 flex-1 flex-col space-y-2 py-2">
								{#each Array(3) as _, i (i)}
									<Skeleton class="h-8 w-full shrink-0" />
								{/each}
							</div>
						{:else if upstreamRefData}
							<div
								class="grid min-h-0 min-w-0 flex-1 grid-cols-1 gap-3 sm:grid-cols-3 lg:min-h-[200px]"
							>
								<div class="flex min-h-0 min-w-0 flex-col">
									<h3
										class="mb-2 flex shrink-0 items-center gap-1 text-xs font-semibold uppercase tracking-wide text-[var(--text-tertiary)]"
									>
										<GitBranch class="h-3.5 w-3.5" />
										Branches
									</h3>
									<div
										class="min-h-0 flex-1 space-y-1 overflow-y-auto overscroll-y-contain rounded-lg border border-[var(--border-primary)] bg-[var(--bg-tertiary)]/60 p-1.5"
									>
										{#each upstreamRefData.branches as b (b.name)}
											<button
												type="button"
												class="w-full rounded-md border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-2 py-1.5 text-left text-xs transition-colors hover:bg-[var(--bg-tertiary)]"
												onclick={() => {
													syncGitRef = b.name;
												}}
											>
												<span class="font-mono font-medium text-[var(--text-primary)]">{b.name}</span>
												<span class="ml-1 font-mono text-[var(--text-tertiary)]"
													>{shortSha(b.commit_sha)}</span
												>
											</button>
										{:else}
											<p class="px-1 py-2 text-xs text-[var(--text-tertiary)]">No branches match.</p>
										{/each}
									</div>
								</div>
								<div class="flex min-h-0 min-w-0 flex-col">
									<h3
										class="mb-2 flex shrink-0 items-center gap-1 text-xs font-semibold uppercase tracking-wide text-[var(--text-tertiary)]"
									>
										<Tag class="h-3.5 w-3.5" />
										Tags
									</h3>
									<div
										class="min-h-0 flex-1 space-y-1 overflow-y-auto overscroll-y-contain rounded-lg border border-[var(--border-primary)] bg-[var(--bg-tertiary)]/60 p-1.5"
									>
										{#each upstreamRefData.tags as t (t.name)}
											<button
												type="button"
												class="w-full rounded-md border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-2 py-1.5 text-left text-xs transition-colors hover:bg-[var(--bg-tertiary)]"
												onclick={() => {
													syncGitRef = t.name;
												}}
											>
												<span class="font-mono font-medium text-[var(--text-primary)]">{t.name}</span>
												<span class="ml-1 font-mono text-[var(--text-tertiary)]"
													>{shortSha(t.commit_sha)}</span
												>
											</button>
										{:else}
											<p class="px-1 py-2 text-xs text-[var(--text-tertiary)]">No tags match.</p>
										{/each}
									</div>
								</div>
								<div class="flex min-h-0 min-w-0 flex-col">
									<h3
										class="mb-2 flex shrink-0 items-center gap-1 text-xs font-semibold uppercase tracking-wide text-[var(--text-tertiary)]"
									>
										<GitCommit class="h-3.5 w-3.5" />
										Commits
									</h3>
									<div
										class="min-h-0 flex-1 space-y-1 overflow-y-auto overscroll-y-contain rounded-lg border border-[var(--border-primary)] bg-[var(--bg-tertiary)]/60 p-1.5"
									>
										{#each upstreamRefData.commits as c (c.sha)}
											<button
												type="button"
												class="w-full rounded-md border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-2 py-1.5 text-left text-xs transition-colors hover:bg-[var(--bg-tertiary)]"
												onclick={() => {
													syncGitRef = c.sha;
												}}
											>
												<span class="font-mono text-[var(--text-primary)]">{shortSha(c.sha)}</span>
												<span class="mt-0.5 block truncate text-[var(--text-secondary)]" title={c.title}
													>{c.title || '—'}</span
												>
											</button>
										{:else}
											<p class="px-1 py-2 text-xs text-[var(--text-tertiary)]">
												Set "Load commits from ref" and click Refresh (optional).
											</p>
										{/each}
									</div>
								</div>
							</div>
						{/if}
					</div>
				</div>

				<div class="flex shrink-0 flex-wrap justify-end gap-2 border-t border-[var(--border-primary)] pt-4">
					<Button type="button" variant="outline" onclick={() => (syncDialogOpen = false)}>
						Cancel
					</Button>
					<Button
						type="button"
						variant="primary"
						loading={syncImportLoading}
						disabled={!syncCredentialsPath.trim() || !syncGitRef.trim()}
						onclick={() => void submitSyncNewVersion()}
					>
						Import this ref
					</Button>
				</div>
			</div>
		{/if}
	</Dialog>
{/if}
