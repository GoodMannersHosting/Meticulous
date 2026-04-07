<script lang="ts">
	import { browser } from '$app/environment';
	import { PUBLIC_API_URL } from '$env/static/public';
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { auth } from '$stores';
	import {
		Button,
		Card,
		Badge,
		Tabs,
		Dialog,
		Alert,
		CopyButton,
		Select,
		Input,
		type TabItem
	} from '$components/ui';
	import HighlightedCodeBlock from '$lib/components/code/HighlightedCodeBlock.svelte';
	import { DataTable, EmptyState, Skeleton } from '$components/data';
	import { apiMethods } from '$api/client';
	import type {
		Pipeline,
		PipelineJob,
		PipelineTrigger,
		ProjectVariable,
		Run,
		StoredSecret,
		UpdatePipelineInput,
		UpdatePipelineTriggerInput,
		WorkflowDiagnosticItem
	} from '$api/types';
	import { formatRelativeTime, truncateId } from '$utils/format';
	import { generateRandomWebhookSecret } from '$utils/webhookSecret';
	import {
		ArrowLeft,
		Play,
		Settings,
		Edit,
		Clock,
		GitCommit,
		User,
		MoreVertical,
		RefreshCw,
		ChevronLeft,
		ChevronRight,
		Pause,
		Trash2,
		ExternalLink,
		KeyRound,
		Braces,
		Plus,
		History,
		Webhook,
		Shuffle,
		CircleHelp
	} from 'lucide-svelte';
	import type { Column, SortDirection } from '$components/data/DataTable.svelte';
	import { sortRunList } from '$utils/sortRuns';
	import {
		runNumberHtml,
		runStatusBadgeHtml,
		effectiveRunStatusForBadge,
		runBranchColumnHtml,
		runTriggeredByHtml,
		runDurationHtml,
		runStartedAtHtml
	} from '$utils/runTableCells';
	import DagViewer from '$components/pipeline/DagViewer.svelte';
	import { stringify } from 'yaml';
	import {
		collectPipelineSourceRows,
		githubRepoTreeUrl,
		pipelineGithubBlobRef,
		upstreamLinkForRow
	} from '$utils/pipelineSourceFiles';
	import {
		WEBHOOK_HELP_BASH,
		WEBHOOK_HELP_PYTHON,
		WEBHOOK_HELP_PWSH
	} from '$utils/webhookHelpSnippets';

	const webhookHelpSnippetTabs: TabItem[] = [
		{ id: 'bash', label: 'Bash' },
		{ id: 'powershell', label: 'PowerShell' },
		{ id: 'python', label: 'Python 3' }
	];

	let pipeline = $state<Pipeline | null>(null);
	let runs = $state<Run[]>([]);
	let loading = $state(true);
	let runsLoading = $state(false);
	let error = $state<string | null>(null);
	let activeTab = $state('runs');
	let triggerLoading = $state(false);
	let syncGitLoading = $state(false);
	let pipelineSecrets = $state<StoredSecret[]>([]);
	let secretsLoading = $state(false);
	let secretsError = $state<string | null>(null);
	let showCreateSecret = $state(false);
	let createPath = $state('');
	let createKind = $state('kv');
	let createValue = $state('');
	let createDescription = $state('');
	let secScopePipelineId = $state('');
	let ghAppId = $state('');
	let ghInstallationId = $state('');
	let ghPrivateKey = $state('');
	let ghApiBase = $state('');
	let ghExtraJson = $state('');
	let secretActionLoading = $state(false);
	let rotateTarget = $state<StoredSecret | null>(null);
	let rotateValue = $state('');
	let showRotateSecretDialog = $state(false);
	let deleteTarget = $state<StoredSecret | null>(null);
	let showDeleteSecretDialog = $state(false);
	let showSecretVersionsDialog = $state(false);
	let versionsContext = $state<StoredSecret | null>(null);
	let secretVersionRows = $state<StoredSecret[]>([]);
	let versionsLoading = $state(false);
	let versionsError = $state<string | null>(null);
	let purgeVersionTarget = $state<StoredSecret | null>(null);
	let showPurgeVersionDialog = $state(false);
	let runSortKey = $state<string | null>('created_at');
	let runSortDirection = $state<SortDirection>('desc');
	let runsPerPage = $state('20');
	let runsListOffset = $state(0);
	let runsHasMore = $state(false);

	let workflowDiagnostics = $state<WorkflowDiagnosticItem[]>([]);
	let wfDiagLoading = $state(false);
	let wfDiagError = $state<string | null>(null);

	const workflowTriggerBlocked = $derived(workflowDiagnostics.some((d) => d.blocking));

	let projectVariablesAll = $state<ProjectVariable[]>([]);
	let variablesLoading = $state(false);
	let variablesError = $state<string | null>(null);
	let showCreateVariable = $state(false);
	let cvName = $state('');
	let cvValue = $state('');
	let cvSensitive = $state(false);
	let cvPipelineId = $state('');
	let variableActionLoading = $state(false);
	let editVariableTarget = $state<ProjectVariable | null>(null);
	let evName = $state('');
	let evValue = $state('');
	let evSensitive = $state(false);
	let showEditVariableDialog = $state(false);
	let deleteVariableTarget = $state<ProjectVariable | null>(null);
	let showDeleteVariableDialog = $state(false);

	let showEditPipelineDialog = $state(false);
	let editPipelineLoading = $state(false);

	let triggers = $state<PipelineTrigger[]>([]);
	let triggersLoading = $state(false);
	let triggersError = $state<string | null>(null);
	let triggerActionLoading = $state(false);
	/** Trigger id while PATCH(enabled) is in flight for row UI. */
	let triggerToggleId = $state<string | null>(null);
	let triggersSuccess = $state<string | null>(null);
	let showCreateTriggerDialog = $state(false);
	let ntDescription = $state('');
	let ntGenerateSecret = $state(true);
	let ntSecretManual = $state('');
	let ntBranches = $state('main, master');
	let ntEvents = $state('push');
	let ntFlatten = $state(true);
	let ntRawBodyVar = $state('');
	let createdTriggerSecret = $state<string | null>(null);
	let showEditTriggerDialog = $state(false);
	let editTriggerTarget = $state<PipelineTrigger | null>(null);
	let etEnabled = $state(true);
	let etDescription = $state('');
	let etNewSecret = $state('');
	let showDeleteTriggerDialog = $state(false);
	let deleteTriggerTarget = $state<PipelineTrigger | null>(null);
	let showWebhookHelpModal = $state(false);
	let webhookHelpCodeTab = $state('bash');

	const webhookHelpActiveSnippet = $derived.by(() => {
		switch (webhookHelpCodeTab) {
			case 'bash':
				return { source: WEBHOOK_HELP_BASH, language: 'bash' as const };
			case 'powershell':
				return { source: WEBHOOK_HELP_PWSH, language: 'powershell' as const };
			case 'python':
				return { source: WEBHOOK_HELP_PYTHON, language: 'python' as const };
			default:
				return { source: WEBHOOK_HELP_BASH, language: 'bash' as const };
		}
	});
	let epName = $state('');
	let epDescription = $state('');
	let epEnabled = $state(true);
	let epScmRepository = $state('');
	let epScmRef = $state('');
	let epScmPath = $state('');
	let epScmCredsPath = $state('');

	const pipelineVariablesRelevant = $derived.by(() => {
		const p = pipeline;
		if (!p) return [];
		return projectVariablesAll.filter(
			(v) => !v.pipeline_id || v.pipeline_id === p.id
		);
	});

	const pipelineVarScopeOptions = $derived(
		pipeline
			? [
					{ value: '', label: 'Project-wide (all pipelines)' },
					{ value: pipeline.id, label: `This pipeline (${pipeline.name})` }
				]
			: [{ value: '', label: 'Project-wide' }]
	);

	const runsPageSizeOptions = [
		{ value: '20', label: '20 per page' },
		{ value: '50', label: '50 per page' },
		{ value: '100', label: '100 per page' }
	];

	const kindOptions = [
		{ value: 'kv', label: 'Key / value (kv)' },
		{ value: 'api_key', label: 'API key' },
		{ value: 'ssh_private_key', label: 'SSH private key (PEM)' },
		{ value: 'github_app', label: 'GitHub App' },
		{ value: 'x509_bundle', label: 'X.509 bundle (JSON)' }
	];

	function definitionJobs(def: Pipeline['definition']): PipelineJob[] {
		if (def && typeof def === 'object' && 'jobs' in def) {
			const j = (def as { jobs: unknown }).jobs;
			if (Array.isArray(j)) return j as PipelineJob[];
		}
		return [];
	}

	function definitionAsYaml(def: Pipeline['definition']): string {
		try {
			return stringify(def as object, { lineWidth: 100 });
		} catch {
			return '';
		}
	}

	function shortRev(sha: string | null | undefined): string | null {
		const s = sha?.trim();
		if (!s) return null;
		return s.length > 12 ? `${s.slice(0, 7)}…` : s;
	}

	function openEditPipeline() {
		const p = pipeline;
		if (!p) return;
		epName = p.name;
		epDescription = p.description ?? '';
		epEnabled = p.enabled;
		epScmRepository = p.scm_repository?.trim() ?? '';
		epScmRef = p.scm_ref?.trim() ?? '';
		epScmPath = p.scm_path?.trim() ?? '';
		epScmCredsPath = p.scm_credentials_secret_path?.trim() ?? '';
		showEditPipelineDialog = true;
	}

	async function submitEditPipeline() {
		const p = pipeline;
		if (!p) return;
		const name = epName.trim();
		if (!name) {
			error = 'Name is required';
			return;
		}
		editPipelineLoading = true;
		error = null;
		try {
			const body: UpdatePipelineInput = {
				name,
				description: epDescription.trim(),
				enabled: epEnabled
			};
			if (p.scm_provider) {
				body.scm_repository = epScmRepository.trim();
				body.scm_ref = epScmRef.trim();
				body.scm_path = epScmPath.trim();
				body.scm_credentials_secret_path = epScmCredsPath.trim();
			}
			const updated = await apiMethods.pipelines.update(p.id, body);
			pipeline = updated;
			showEditPipelineDialog = false;
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to update pipeline';
		} finally {
			editPipelineLoading = false;
		}
	}

	const pipelineSourceRows = $derived(pipeline ? collectPipelineSourceRows(pipeline) : []);
	const pipelineDefinitionYaml = $derived(pipeline ? definitionAsYaml(pipeline.definition) : '');
	const pipelineGithubRef = $derived(pipeline ? pipelineGithubBlobRef(pipeline) : null);
	const pipelineGithubTreeUrl = $derived(pipeline ? githubRepoTreeUrl(pipeline) : null);

	const tabs = [
		{ id: 'runs', label: 'Runs', icon: Play },
		{ id: 'triggers', label: 'Triggers', icon: Webhook },
		{ id: 'variables', label: 'Variables', icon: Braces },
		{ id: 'secrets', label: 'Secrets', icon: KeyRound },
		{ id: 'definition', label: 'Definition', icon: Settings }
	];

	function apiPublicOrigin(): string {
		const base = PUBLIC_API_URL?.replace(/\/$/, '') ?? '';
		if (base) return base;
		if (browser) return window.location.origin;
		return '';
	}

	function triggerConfigStr(t: PipelineTrigger, key: string): string | undefined {
		const v = t.config[key];
		return typeof v === 'string' ? v : undefined;
	}

	function triggerIsRepoManaged(t: PipelineTrigger): boolean {
		return triggerConfigStr(t, 'managed_by') === 'repo';
	}

	function webhookPostUrl(triggerId: string): string {
		const org = auth.user?.org_id ?? '{org_id}';
		return `${apiPublicOrigin()}/api/v1/webhooks/${org}/${triggerId}`;
	}

	async function loadTriggers() {
		if (!pipeline) return;
		triggersLoading = true;
		triggersError = null;
		try {
			triggers = await apiMethods.triggers.list(pipeline.id);
		} catch (e) {
			triggersError = e instanceof Error ? e.message : 'Failed to load triggers';
			triggers = [];
		} finally {
			triggersLoading = false;
		}
	}

	function openCreateTrigger() {
		triggersSuccess = null;
		createdTriggerSecret = null;
		ntDescription = '';
		ntGenerateSecret = true;
		ntSecretManual = '';
		ntBranches = 'main, master';
		ntEvents = 'push';
		ntFlatten = true;
		ntRawBodyVar = '';
		showCreateTriggerDialog = true;
	}

	async function submitCreateTrigger() {
		if (!pipeline) return;
		triggerActionLoading = true;
		triggersSuccess = null;
		createdTriggerSecret = null;
		try {
			const branches = ntBranches
				.split(',')
				.map((s) => s.trim())
				.filter(Boolean);
			const events = ntEvents
				.split(',')
				.map((s) => s.trim())
				.filter(Boolean);
			const config: Record<string, unknown> = {
				branches,
				events,
				flatten_top_level: ntFlatten
			};
			if (ntRawBodyVar.trim()) {
				config.include_raw_body_variable = ntRawBodyVar.trim();
			}
			if (!ntGenerateSecret && ntSecretManual.trim()) {
				config.secret = ntSecretManual.trim();
			}
			const created = await apiMethods.triggers.create(pipeline.id, {
				kind: 'webhook',
				config,
				description: ntDescription.trim() || undefined,
				generate_webhook_secret: ntGenerateSecret
			});
			if (created.generated_secret) {
				createdTriggerSecret = created.generated_secret;
				triggersSuccess =
					'Webhook trigger created. Copy the generated secret below now — it will not be shown again.';
			} else {
				triggersSuccess = 'Webhook trigger created.';
			}
			showCreateTriggerDialog = false;
			await loadTriggers();
		} catch (e) {
			triggersError = e instanceof Error ? e.message : 'Failed to create trigger';
		} finally {
			triggerActionLoading = false;
		}
	}

	function openEditTrigger(t: PipelineTrigger) {
		editTriggerTarget = t;
		etEnabled = t.enabled;
		etDescription = t.description ?? '';
		etNewSecret = '';
		triggersError = null;
		showEditTriggerDialog = true;
	}

	function applyRandomWebhookSecret(setter: (v: string) => void) {
		triggersError = null;
		try {
			setter(generateRandomWebhookSecret());
		} catch (e) {
			triggersError = e instanceof Error ? e.message : 'Could not generate secret';
		}
	}

	async function submitEditTrigger() {
		if (!editTriggerTarget) return;
		triggerActionLoading = true;
		triggersError = null;
		try {
			const body: UpdatePipelineTriggerInput = {
				enabled: etEnabled,
				description: etDescription.trim()
			};
			if (etNewSecret.trim()) {
				body.config_patch = { secret: etNewSecret.trim() };
			}
			await apiMethods.triggers.update(editTriggerTarget.id, body);
			showEditTriggerDialog = false;
			editTriggerTarget = null;
			triggersSuccess = 'Trigger updated.';
			await loadTriggers();
		} catch (e) {
			triggersError = e instanceof Error ? e.message : 'Failed to update trigger';
		} finally {
			triggerActionLoading = false;
		}
	}

	function openDeleteTrigger(t: PipelineTrigger) {
		deleteTriggerTarget = t;
		triggersError = null;
		showDeleteTriggerDialog = true;
	}

	async function submitDeleteTrigger() {
		if (!deleteTriggerTarget) return;
		triggerActionLoading = true;
		triggersError = null;
		try {
			await apiMethods.triggers.delete(deleteTriggerTarget.id);
			showDeleteTriggerDialog = false;
			deleteTriggerTarget = null;
			triggersSuccess = 'Trigger deleted.';
			await loadTriggers();
		} catch (e) {
			triggersError = e instanceof Error ? e.message : 'Failed to delete trigger';
		} finally {
			triggerActionLoading = false;
		}
	}

	async function toggleTriggerEnabled(t: PipelineTrigger, enabled: boolean) {
		if (triggerToggleId === t.id) return;
		triggerToggleId = t.id;
		triggersError = null;
		try {
			await apiMethods.triggers.update(t.id, { enabled });
			triggersSuccess = enabled ? 'Webhook enabled.' : 'Webhook disabled.';
			await loadTriggers();
		} catch (e) {
			triggersError = e instanceof Error ? e.message : 'Failed to update trigger';
			await loadTriggers();
		} finally {
			triggerToggleId = null;
		}
	}

	$effect(() => {
		loadPipeline();
	});

	async function loadWorkflowDiagnostics() {
		const p = pipeline;
		if (!p) return;
		wfDiagLoading = true;
		wfDiagError = null;
		try {
			workflowDiagnostics = await apiMethods.pipelines.workflowDiagnostics(p.id);
		} catch (e) {
			wfDiagError =
				e instanceof Error ? e.message : 'Could not load reusable workflow diagnostics';
			workflowDiagnostics = [];
		} finally {
			wfDiagLoading = false;
		}
	}

	async function loadPipeline() {
		loading = true;
		error = null;
		try {
			const pipelineId = $page.params.id!;
			runsListOffset = 0;
			pipeline = await apiMethods.pipelines.get(pipelineId);
			await loadRuns({ offset: 0 });
			await loadWorkflowDiagnostics();
			await loadTriggers();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load pipeline';
		} finally {
			loading = false;
		}
	}

	async function loadRuns(opts?: { offset?: number }) {
		if (!pipeline) return;
		const offset = opts?.offset ?? runsListOffset;
		runsLoading = true;
		try {
			const response = await apiMethods.runs.list({
				pipeline_id: pipeline.id,
				per_page: Number(runsPerPage),
				cursor: offset > 0 ? String(offset) : undefined
			});
			runs = response.data;
			runsHasMore = response.pagination.has_more;
			runsListOffset = offset;
		} catch (e) {
			console.error('Failed to load runs:', e);
		} finally {
			runsLoading = false;
		}
	}

	function handleRunsPerPageChange() {
		runsListOffset = 0;
		void loadRuns({ offset: 0 });
	}

	function runsPrevPage() {
		const step = Number(runsPerPage);
		if (runsListOffset <= 0) return;
		void loadRuns({ offset: Math.max(0, runsListOffset - step) });
	}

	function runsNextPage() {
		if (!runsHasMore) return;
		void loadRuns({ offset: runsListOffset + runs.length });
	}

	const runsPageLabel = $derived.by(() => {
		if (runs.length === 0) return null;
		const from = runsListOffset + 1;
		const to = runsListOffset + runs.length;
		return `${from}–${to}`;
	});

	async function loadPipelineSecrets() {
		if (!pipeline) return;
		secretsLoading = true;
		secretsError = null;
		try {
			pipelineSecrets = await apiMethods.storedSecrets.list(pipeline.project_id, {
				pipeline_id: pipeline.id
			});
		} catch (e) {
			secretsError = e instanceof Error ? e.message : 'Failed to load secrets';
			pipelineSecrets = [];
		} finally {
			secretsLoading = false;
		}
	}

	$effect(() => {
		if (activeTab !== 'secrets' || !pipeline || loading) return;
		void loadPipelineSecrets();
	});

	function storedSecretScopeLabel(s: StoredSecret): string {
		const p = pipeline;
		if (!s.pipeline_id) return 'Project';
		if (p && s.pipeline_id === p.id) return 'This pipeline';
		return s.pipeline_id.slice(0, 8);
	}

	function openCreateSecret() {
		createPath = '';
		createKind = 'kv';
		createValue = '';
		createDescription = '';
		secScopePipelineId = pipeline?.id ?? '';
		ghAppId = '';
		ghInstallationId = '';
		ghPrivateKey = '';
		ghApiBase = '';
		ghExtraJson = '';
		showCreateSecret = true;
	}

	function createSecretValid(): boolean {
		if (!createPath.trim()) return false;
		if (createKind === 'github_app') {
			return !!(ghAppId.trim() && ghInstallationId.trim() && ghPrivateKey.trim());
		}
		return !!createValue.trim();
	}

	async function submitCreateSecret() {
		const p = pipeline;
		if (!p) return;
		secretActionLoading = true;
		secretsError = null;
		try {
			let value: string;
			if (createKind === 'github_app') {
				if (!ghAppId.trim() || !ghInstallationId.trim() || !ghPrivateKey.trim()) {
					secretsError = 'GitHub App: App ID, Installation ID, and private key are required';
					return;
				}
				const app_id = Number(ghAppId);
				const installation_id = Number(ghInstallationId);
				if (!Number.isFinite(app_id) || !Number.isFinite(installation_id)) {
					secretsError = 'GitHub App: App ID and Installation ID must be numeric';
					return;
				}
				let extraFields: Record<string, unknown> = {};
				if (ghExtraJson.trim()) {
					try {
						const parsed = JSON.parse(ghExtraJson) as unknown;
						if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) {
							secretsError = 'GitHub App: Additional fields must be a JSON object';
							return;
						}
						extraFields = parsed as Record<string, unknown>;
					} catch {
						secretsError = 'GitHub App: Additional fields are not valid JSON';
						return;
					}
				}
				value = JSON.stringify({
					app_id,
					installation_id,
					private_key_pem: ghPrivateKey.trim(),
					...(ghApiBase.trim() ? { github_api_base: ghApiBase.trim() } : {}),
					...extraFields
				});
			} else {
				value = createValue;
			}

			await apiMethods.storedSecrets.create(p.project_id, {
				path: createPath.trim(),
				kind: createKind,
				value,
				description: createDescription.trim() || undefined,
				pipeline_id: secScopePipelineId || undefined
			});
			showCreateSecret = false;
			await loadPipelineSecrets();
		} catch (e) {
			secretsError = e instanceof Error ? e.message : 'Failed to create secret';
		} finally {
			secretActionLoading = false;
		}
	}

	async function submitRotateSecret() {
		if (!rotateTarget || !pipeline) return;
		secretActionLoading = true;
		secretsError = null;
		try {
			await apiMethods.storedSecrets.rotate(rotateTarget.id, rotateValue);
			showRotateSecretDialog = false;
			rotateTarget = null;
			rotateValue = '';
			await loadPipelineSecrets();
		} catch (e) {
			secretsError = e instanceof Error ? e.message : 'Failed to rotate secret';
		} finally {
			secretActionLoading = false;
		}
	}

	async function submitDeleteSecret() {
		if (!deleteTarget) return;
		secretActionLoading = true;
		secretsError = null;
		try {
			await apiMethods.storedSecrets.delete(deleteTarget.id);
			showDeleteSecretDialog = false;
			deleteTarget = null;
			await loadPipelineSecrets();
		} catch (e) {
			secretsError = e instanceof Error ? e.message : 'Failed to delete secret';
		} finally {
			secretActionLoading = false;
		}
	}

	function openSecretVersions(s: StoredSecret) {
		versionsContext = s;
		versionsError = null;
		secretVersionRows = [];
		showSecretVersionsDialog = true;
		void refreshSecretVersions();
	}

	async function refreshSecretVersions() {
		const ctx = versionsContext;
		const p = pipeline;
		if (!ctx || !p) return;
		versionsLoading = true;
		versionsError = null;
		try {
			secretVersionRows = await apiMethods.storedSecrets.listVersions(p.project_id, {
				path: ctx.path,
				...(ctx.pipeline_id ? { pipeline_id: ctx.pipeline_id } : {})
			});
		} catch (e) {
			versionsError = e instanceof Error ? e.message : 'Failed to load versions';
			secretVersionRows = [];
		} finally {
			versionsLoading = false;
		}
	}

	async function submitActivateSecretVersion(row: StoredSecret) {
		secretActionLoading = true;
		versionsError = null;
		secretsError = null;
		try {
			await apiMethods.storedSecrets.activateVersion(row.id);
			await loadPipelineSecrets();
			await refreshSecretVersions();
		} catch (e) {
			versionsError = e instanceof Error ? e.message : 'Failed to roll back';
		} finally {
			secretActionLoading = false;
		}
	}

	async function submitPurgeSecretVersion() {
		if (!purgeVersionTarget) return;
		secretActionLoading = true;
		versionsError = null;
		secretsError = null;
		try {
			await apiMethods.storedSecrets.purgeVersionPermanent(purgeVersionTarget.id);
			showPurgeVersionDialog = false;
			purgeVersionTarget = null;
			await loadPipelineSecrets();
			await refreshSecretVersions();
		} catch (e) {
			versionsError = e instanceof Error ? e.message : 'Failed to purge version';
		} finally {
			secretActionLoading = false;
		}
	}

	async function loadPipelineVariables() {
		if (!pipeline) return;
		variablesLoading = true;
		variablesError = null;
		try {
			const res = await apiMethods.variables.list(pipeline.project_id);
			projectVariablesAll = res.data;
		} catch (e) {
			variablesError = e instanceof Error ? e.message : 'Failed to load variables';
			projectVariablesAll = [];
		} finally {
			variablesLoading = false;
		}
	}

	$effect(() => {
		if (activeTab !== 'variables' || !pipeline || loading) return;
		void loadPipelineVariables();
	});

	function variableScopeLabel(v: ProjectVariable): string {
		if (!v.pipeline_id) return 'Project';
		if (pipeline && v.pipeline_id === pipeline.id) return 'This pipeline';
		return v.pipeline_id.slice(0, 8);
	}

	function openCreateVariable() {
		cvName = '';
		cvValue = '';
		cvSensitive = false;
		cvPipelineId = pipeline?.id ?? '';
		showCreateVariable = true;
	}

	async function submitCreateVariable() {
		if (!pipeline) return;
		variableActionLoading = true;
		variablesError = null;
		try {
			await apiMethods.variables.create(pipeline.project_id, {
				name: cvName.trim(),
				value: cvValue,
				is_sensitive: cvSensitive,
				pipeline_id: cvPipelineId || undefined
			});
			showCreateVariable = false;
			await loadPipelineVariables();
		} catch (e) {
			variablesError = e instanceof Error ? e.message : 'Failed to create variable';
		} finally {
			variableActionLoading = false;
		}
	}

	function openEditVariable(v: ProjectVariable) {
		editVariableTarget = v;
		evName = v.name;
		evValue = v.value ?? '';
		evSensitive = v.is_sensitive;
		showEditVariableDialog = true;
	}

	async function submitEditVariable() {
		if (!editVariableTarget) return;
		variableActionLoading = true;
		variablesError = null;
		try {
			await apiMethods.variables.update(editVariableTarget.id, {
				name: evName.trim(),
				...(evValue !== '' ? { value: evValue } : {}),
				is_sensitive: evSensitive
			});
			showEditVariableDialog = false;
			editVariableTarget = null;
			await loadPipelineVariables();
		} catch (e) {
			variablesError = e instanceof Error ? e.message : 'Failed to update variable';
		} finally {
			variableActionLoading = false;
		}
	}

	async function submitDeleteVariable() {
		if (!deleteVariableTarget) return;
		variableActionLoading = true;
		variablesError = null;
		try {
			await apiMethods.variables.delete(deleteVariableTarget.id);
			showDeleteVariableDialog = false;
			deleteVariableTarget = null;
			await loadPipelineVariables();
		} catch (e) {
			variablesError = e instanceof Error ? e.message : 'Failed to delete variable';
		} finally {
			variableActionLoading = false;
		}
	}

	async function triggerPipeline() {
		if (!pipeline) return;
		triggerLoading = true;
		try {
			const result = await apiMethods.pipelines.trigger(pipeline.id);
			goto(`/runs/${result.run_id}`);
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to trigger pipeline';
		} finally {
			triggerLoading = false;
		}
	}

	async function syncFromGit() {
		if (!pipeline) return;
		syncGitLoading = true;
		error = null;
		try {
			const updated = await apiMethods.pipelines.syncFromGit(pipeline.id, {});
			pipeline = updated;
			await loadWorkflowDiagnostics();
			await loadTriggers();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to sync from source code';
		} finally {
			syncGitLoading = false;
		}
	}

	const sortedRuns = $derived(sortRunList(runs, runSortKey, runSortDirection));

	const runColumns: Column<Run>[] = [
		{
			key: 'run_number',
			label: 'Run',
			width: '100px',
			sortable: true,
			render: (value, row) => runNumberHtml(value, row)
		},
		{
			key: 'status',
			label: 'Status',
			width: '140px',
			sortable: true,
			render: (_v, row) => runStatusBadgeHtml(effectiveRunStatusForBadge(row))
		},
		{
			key: 'branch',
			label: 'Branch',
			sortable: true,
			render: (value, row) => runBranchColumnHtml(value, row)
		},
		{
			key: 'triggered_by',
			label: 'Triggered By',
			sortable: true,
			render: (value, row) => runTriggeredByHtml(value, row)
		},
		{
			key: 'duration_ms',
			label: 'Duration',
			sortable: true,
			render: (value) => runDurationHtml(value)
		},
		{
			key: 'created_at',
			label: 'Started',
			sortable: true,
			render: (_value, row) => runStartedAtHtml(_value, row)
		}
	];

	function handleRunSort(key: string, direction: SortDirection) {
		if (direction === null) {
			runSortKey = null;
			runSortDirection = null;
		} else {
			runSortKey = key;
			runSortDirection = direction;
		}
	}

	function handleRunClick(run: Run) {
		goto(`/runs/${run.id}`);
	}
</script>

<svelte:head>
	<title>{pipeline?.name ?? 'Pipeline'} | Meticulous</title>
</svelte:head>

<div class="space-y-6">
	<div class="flex items-start gap-4">
		<Button variant="ghost" size="sm" href="/pipelines">
			<ArrowLeft class="h-4 w-4" />
		</Button>

		{#if loading}
			<div class="flex-1 space-y-2">
				<Skeleton class="h-7 w-48" />
				<Skeleton class="h-4 w-32" />
			</div>
		{:else if pipeline}
			<div class="flex-1">
				<div class="flex items-center gap-3">
					<h1 class="text-2xl font-bold text-[var(--text-primary)]">{pipeline.name}</h1>
					{#if pipeline.enabled}
						<Badge variant="success" size="sm">Active</Badge>
					{:else}
						<Badge variant="secondary" size="sm">Disabled</Badge>
					{/if}
				</div>
				{#if pipeline.description}
					<p class="mt-1 text-[var(--text-secondary)]">{pipeline.description}</p>
				{/if}
				<div class="mt-2 flex items-center gap-4 text-sm text-[var(--text-tertiary)]">
					<span class="flex items-center gap-1">
						<Clock class="h-4 w-4" />
						Updated {formatRelativeTime(pipeline.updated_at)}
					</span>
					<span class="flex items-center gap-1 font-mono">
						{truncateId(pipeline.id)}
						<CopyButton text={pipeline.id} size="sm" />
					</span>
				</div>
			</div>

			<div class="flex flex-wrap items-center gap-2">
				{#if pipeline.scm_provider === 'github'}
					<Button variant="outline" size="sm" onclick={syncFromGit} loading={syncGitLoading}>
						<GitCommit class="h-4 w-4" />
						Sync from Git
					</Button>
				{/if}
				<Button variant="outline" size="sm" onclick={openEditPipeline}>
					<Edit class="h-4 w-4" />
					Edit
				</Button>
				<Button
					variant="primary"
					onclick={triggerPipeline}
					loading={triggerLoading}
					disabled={workflowTriggerBlocked || wfDiagLoading}
					title={workflowTriggerBlocked
						? 'Fix workflow catalog issues before running'
						: undefined}
				>
					<Play class="h-4 w-4" />
					Run Pipeline
				</Button>
			</div>
		{/if}
	</div>

	{#if error}
		<Alert variant="error" title="Error" dismissible ondismiss={() => (error = null)}>
			{error}
		</Alert>
	{/if}

	{#if !loading && pipeline && (wfDiagError || (!wfDiagLoading && workflowDiagnostics.length > 0))}
		{#if wfDiagError}
			<Alert variant="warning" title="Workflow diagnostics unavailable">
				{wfDiagError}
			</Alert>
		{:else if workflowTriggerBlocked}
			<Alert variant="warning" title="Reusable workflows block runs" dismissible={false}>
				<p class="mb-2 text-sm">
					Resolve catalog approval, trust, or missing versions (or adjust org policy) before
					triggering.
				</p>
				<ul class="list-inside list-disc space-y-1 text-sm text-[var(--text-secondary)]">
					{#each workflowDiagnostics.filter((d) => d.blocking) as d}
						<li>
							<strong class="text-[var(--text-primary)]">{d.invocation_id}</strong>
							({d.reference}): {d.status}{#if d.detail} — {d.detail}{/if}
						</li>
					{/each}
				</ul>
				<p class="mt-2 text-sm">
					<a href="/workflows" class="text-primary-600 underline hover:no-underline"
						>Open workflow catalog</a
					>
				</p>
			</Alert>
		{:else}
			<Alert variant="success" title="Reusable workflows OK">
				All <code class="rounded bg-[var(--bg-tertiary)] px-1">{workflowDiagnostics.length}</code> workflow
				reference(s) in this pipeline resolve for the current org policy.
			</Alert>
		{/if}
	{/if}

	{#if !loading && pipeline}
		{#if triggers.length > 0}
			{@const webhookTriggers = triggers.filter((t) => t.kind === 'webhook')}
			{#if webhookTriggers.length > 0}
				<div
					class="flex flex-col gap-2 rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-4 py-3 text-sm sm:flex-row sm:flex-wrap sm:items-baseline sm:gap-x-3"
				>
					<span class="font-medium text-[var(--text-primary)]">HTTP webhooks</span>
					<span class="text-[var(--text-secondary)]">
						{webhookTriggers.length} trigger{webhookTriggers.length === 1 ? '' : 's'}
					</span>
					<button
						type="button"
						class="text-left text-primary-600 underline hover:no-underline sm:text-left"
						onclick={() => (activeTab = 'triggers')}
					>
						View URLs & settings
					</button>
					<p class="w-full text-[var(--text-secondary)]">
						{#each webhookTriggers as t, i}
							{#if i > 0}<span class="text-[var(--text-tertiary)]"> · </span>{/if}
							{#if t.description?.trim()}
								<span title={t.id}>{t.description}</span>
							{:else}
								<span class="font-mono text-xs text-[var(--text-tertiary)]" title={t.id}
									>{truncateId(t.id)}</span
								>
							{/if}
						{/each}
					</p>
				</div>
			{/if}
		{/if}

		<Tabs items={tabs} bind:value={activeTab} />

		{#if activeTab === 'runs'}
			<div class="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
				<p class="text-sm text-[var(--text-secondary)]">
					{#if runs.length === 0 && runsListOffset === 0 && !runsLoading}
						0 runs in this pipeline
					{:else if runs.length === 0 && runsListOffset > 0 && !runsLoading}
						No runs on this page — try previous page
					{:else if runsPageLabel}
						Showing runs {runsPageLabel}
						{#if runsHasMore}
							<span class="text-[var(--text-tertiary)]"> (more available)</span>
						{/if}
					{:else if runsLoading}
						Loading…
					{:else}
						—
					{/if}
				</p>
				<div class="flex flex-wrap items-center gap-2">
					<Select
						options={runsPageSizeOptions}
						bind:value={runsPerPage}
						size="sm"
						class="w-36"
						onchange={handleRunsPerPageChange}
					/>
					<div class="flex items-center gap-1">
						<Button
							variant="outline"
							size="sm"
							disabled={runsListOffset <= 0 || runsLoading}
							onclick={runsPrevPage}
							title="Previous page"
						>
							<ChevronLeft class="h-4 w-4" />
						</Button>
						<Button
							variant="outline"
							size="sm"
							disabled={!runsHasMore || runsLoading}
							onclick={runsNextPage}
							title="Next page"
						>
							<ChevronRight class="h-4 w-4" />
						</Button>
					</div>
					<Button variant="ghost" size="sm" onclick={() => loadRuns()} loading={runsLoading}>
						<RefreshCw class="h-4 w-4" />
						Refresh
					</Button>
				</div>
			</div>

			{#if runsLoading && runs.length === 0}
				<Card>
					<div class="space-y-4">
						{#each Array(5) as _, i (i)}
							<div class="flex items-center gap-4">
								<Skeleton class="h-5 w-16" />
								<Skeleton class="h-6 w-24 rounded-full" />
								<Skeleton class="h-5 w-32" />
								<div class="flex-1"></div>
								<Skeleton class="h-5 w-20" />
								<Skeleton class="h-5 w-24" />
							</div>
						{/each}
					</div>
				</Card>
			{:else if runs.length === 0}
				<Card>
					<EmptyState
						title="No runs yet"
						description="Trigger this pipeline to start your first run."
					>
						<Button
							variant="primary"
							onclick={triggerPipeline}
							loading={triggerLoading}
							disabled={workflowTriggerBlocked || wfDiagLoading}
							title={workflowTriggerBlocked
								? 'Fix workflow catalog issues before running'
								: undefined}
						>
							<Play class="h-4 w-4" />
							Run Pipeline
						</Button>
					</EmptyState>
				</Card>
			{:else}
				<DataTable
					columns={runColumns}
					data={sortedRuns}
					rowKey="id"
					sortKey={runSortKey}
					sortDirection={runSortDirection}
					onSort={handleRunSort}
					onRowClick={handleRunClick}
					loading={runsLoading && runs.length === 0}
				/>
			{/if}
		{:else if activeTab === 'triggers'}
			<div class="flex flex-wrap items-center justify-between gap-3">
				<div class="max-w-3xl text-sm text-[var(--text-secondary)]">
					<p>
						<strong class="text-[var(--text-primary)]">HTTP webhooks</strong> start a run when called with
						<code class="rounded bg-[var(--bg-tertiary)] px-1 font-mono text-xs">POST</code>
						and optional
						<code class="rounded bg-[var(--bg-tertiary)] px-1 font-mono text-xs">X-Hub-Signature-256</code>
						when a secret is set. URL path uses your org id and the trigger id.
					</p>
					<p class="mt-2 text-xs text-[var(--text-tertiary)]">
						Triggers with <strong>Git</strong> are managed from pipeline YAML (<code class="font-mono">sync-key</code>);
						remove them in the repo or run Sync from Git. Declarative filters are not editable here.
					</p>
				</div>
				<div class="flex flex-wrap gap-2">
					<Button
						variant="outline"
						size="sm"
						onclick={() => {
							webhookHelpCodeTab = 'bash';
							showWebhookHelpModal = true;
						}}
						title="Sign requests with your webhook secret"
					>
						<CircleHelp class="h-4 w-4" aria-hidden="true" />
						Signing help
					</Button>
					<Button variant="ghost" size="sm" onclick={loadTriggers} loading={triggersLoading}>
						<RefreshCw class="h-4 w-4" />
						Refresh
					</Button>
					<Button variant="primary" size="sm" onclick={openCreateTrigger}>
						<Plus class="h-4 w-4" />
						Add webhook trigger
					</Button>
				</div>
			</div>
			{#if triggersSuccess}
				<Alert
					variant="success"
					title="Triggers"
					dismissible
					ondismiss={() => {
						triggersSuccess = null;
						createdTriggerSecret = null;
					}}
				>
					<p>{triggersSuccess}</p>
					{#if createdTriggerSecret}
						<p class="mt-2 font-mono text-sm text-[var(--text-primary)]">{createdTriggerSecret}</p>
						<div class="mt-2 flex items-center gap-2">
							<CopyButton text={createdTriggerSecret} size="sm" />
							<span class="text-xs text-[var(--text-secondary)]">Copy secret</span>
						</div>
					{/if}
				</Alert>
			{/if}
			{#if triggersError}
				<Alert variant="error" title="Triggers" dismissible ondismiss={() => (triggersError = null)}>
					{triggersError}
				</Alert>
			{/if}
			{#if triggersLoading && triggers.length === 0}
				<Card>
					<div class="space-y-3 p-4">
						{#each Array(3) as _, i (i)}
							<Skeleton class="h-10 w-full" />
						{/each}
					</div>
				</Card>
			{:else if triggers.length === 0}
				<Card>
					<EmptyState
						title="No triggers yet"
						description="Add a webhook or define triggers.webhook with sync-key in YAML and sync from Git."
					>
						<Button variant="primary" onclick={openCreateTrigger}>
							<Plus class="h-4 w-4" />
							Add webhook trigger
						</Button>
					</EmptyState>
				</Card>
			{:else}
				<div class="overflow-hidden rounded-lg border border-[var(--border-primary)]">
					<table class="w-full text-sm">
						<thead class="bg-[var(--bg-tertiary)]">
							<tr>
								<th class="px-4 py-3 text-left font-medium text-[var(--text-secondary)]">Kind</th>
								<th class="min-w-[8rem] max-w-md px-4 py-3 text-left font-medium text-[var(--text-secondary)]"
									>Description</th
								>
								<th class="px-4 py-3 text-left font-medium text-[var(--text-secondary)]">Source</th>
								<th class="px-4 py-3 text-left font-medium text-[var(--text-secondary)]">HMAC</th>
								<th class="px-4 py-3 text-left font-medium text-[var(--text-secondary)]">Enabled</th>
								<th class="px-4 py-3 text-left font-medium text-[var(--text-secondary)]">Webhook URL</th>
								<th class="px-4 py-3 text-right font-medium text-[var(--text-secondary)]">Actions</th>
							</tr>
						</thead>
						<tbody class="divide-y divide-[var(--border-secondary)]">
							{#each triggers as t (t.id)}
								<tr class="bg-[var(--bg-secondary)]">
									<td class="px-4 py-3 capitalize">{t.kind.replace(/_/g, ' ')}</td>
									<td class="max-w-md px-4 py-3">
										{#if t.description?.trim()}
											<p class="text-sm text-[var(--text-primary)]" title={t.description}>
												{t.description}
											</p>
										{:else}
											<span class="text-xs text-[var(--text-tertiary)]">—</span>
										{/if}
									</td>
									<td class="px-4 py-3">
										{#if triggerIsRepoManaged(t)}
											<Badge variant="secondary" size="sm">Git</Badge>
											{#if triggerConfigStr(t, 'sync_key')}
												<span class="ml-1 font-mono text-xs text-[var(--text-tertiary)]">
													{triggerConfigStr(t, 'sync_key')}
												</span>
											{/if}
										{:else}
											<span class="text-[var(--text-secondary)]">API</span>
										{/if}
									</td>
									<td class="px-4 py-3">
										{#if t.secret_configured}
											<Badge variant="success" size="sm">Required</Badge>
										{:else}
											<Badge variant="secondary" size="sm">Optional</Badge>
										{/if}
									</td>
									<td class="px-4 py-3">
										<label class="inline-flex cursor-pointer items-center gap-2 text-sm text-[var(--text-secondary)]">
											<input
												type="checkbox"
												class="h-4 w-4 rounded border-[var(--border-primary)] text-primary-600 focus:ring-primary-500"
												checked={t.enabled}
												disabled={triggerToggleId === t.id}
												onchange={(e) => {
													const el = e.currentTarget as HTMLInputElement;
													void toggleTriggerEnabled(t, el.checked);
												}}
											/>
											<span>{t.enabled ? 'On' : 'Off'}</span>
										</label>
									</td>
									<td class="px-4 py-3">
										<div class="flex max-w-md flex-col gap-1">
											<code
												class="block truncate rounded bg-[var(--bg-tertiary)] px-2 py-1 text-xs text-[var(--text-primary)]"
												title={webhookPostUrl(t.id)}
											>
												{webhookPostUrl(t.id)}
											</code>
											<div class="flex items-center gap-2">
												<CopyButton text={webhookPostUrl(t.id)} size="sm" />
												<span class="text-xs text-[var(--text-secondary)]">Copy URL</span>
											</div>
										</div>
									</td>
									<td class="px-4 py-3 text-right">
										<div class="flex justify-end gap-2">
											<Button variant="ghost" size="sm" onclick={() => openEditTrigger(t)}>
												Edit
											</Button>
											<Button
												variant="ghost"
												size="sm"
												disabled={triggerIsRepoManaged(t)}
												title={triggerIsRepoManaged(t)
													? 'Delete via YAML or sync — repo-managed'
													: 'Delete trigger'}
												onclick={() => openDeleteTrigger(t)}
											>
												<Trash2 class="h-4 w-4" />
											</Button>
										</div>
									</td>
								</tr>
							{/each}
						</tbody>
					</table>
				</div>
			{/if}
		{:else if activeTab === 'variables'}
			<div class="flex flex-wrap items-center justify-between gap-3">
				<p class="text-sm text-[var(--text-secondary)]">
					Variables that apply to this pipeline: <strong>project-wide</strong> entries plus any
					<strong>pipeline-only</strong> overrides. YAML and trigger-time variables still override for the same name.
				</p>
				<div class="flex gap-2">
					<Button variant="outline" size="sm" href="/projects/{pipeline.project_id}">
						All project variables
					</Button>
					<Button variant="ghost" size="sm" onclick={loadPipelineVariables} loading={variablesLoading}>
						<RefreshCw class="h-4 w-4" />
						Refresh
					</Button>
					<Button variant="primary" size="sm" onclick={openCreateVariable}>
						<Plus class="h-4 w-4" />
						Add variable
					</Button>
				</div>
			</div>
			{#if variablesError}
				<Alert variant="error" title="Variables" dismissible ondismiss={() => (variablesError = null)}>
					{variablesError}
				</Alert>
			{/if}
			{#if variablesLoading && pipelineVariablesRelevant.length === 0}
				<Card>
					<div class="space-y-3 p-4">
						{#each Array(3) as _, i (i)}
							<Skeleton class="h-10 w-full" />
						{/each}
					</div>
				</Card>
			{:else if pipelineVariablesRelevant.length === 0}
				<Card>
					<EmptyState
						title="No variables yet"
						description="Add project-wide defaults or pipeline-specific values."
					>
						<Button variant="primary" onclick={openCreateVariable}>
							<Plus class="h-4 w-4" />
							Add variable
						</Button>
					</EmptyState>
				</Card>
			{:else}
				<div class="overflow-hidden rounded-lg border border-[var(--border-primary)]">
					<table class="w-full text-sm">
						<thead class="bg-[var(--bg-tertiary)]">
							<tr>
								<th class="px-4 py-3 text-left font-medium text-[var(--text-secondary)]">Name</th>
								<th class="px-4 py-3 text-left font-medium text-[var(--text-secondary)]">Scope</th>
								<th class="px-4 py-3 text-left font-medium text-[var(--text-secondary)]">Value</th>
								<th class="px-4 py-3 text-left font-medium text-[var(--text-secondary)]">Sensitive</th>
								<th class="px-4 py-3 text-right font-medium text-[var(--text-secondary)]">Actions</th>
							</tr>
						</thead>
						<tbody class="divide-y divide-[var(--border-secondary)]">
							{#each pipelineVariablesRelevant as v (v.id)}
								<tr class="bg-[var(--bg-secondary)]">
									<td class="px-4 py-3 font-mono text-sm">{v.name}</td>
									<td class="px-4 py-3">{variableScopeLabel(v)}</td>
									<td class="px-4 py-3 text-[var(--text-secondary)]">
										{#if v.is_sensitive}
											<span class="italic">hidden</span>
										{:else}
											{v.value ?? '—'}
										{/if}
									</td>
									<td class="px-4 py-3">{v.is_sensitive ? 'Yes' : 'No'}</td>
									<td class="px-4 py-3 text-right">
										<div class="flex justify-end gap-2">
											<Button variant="ghost" size="sm" onclick={() => openEditVariable(v)}>
												Edit
											</Button>
											<Button
												variant="ghost"
												size="sm"
												onclick={() => {
													deleteVariableTarget = v;
													showDeleteVariableDialog = true;
												}}
											>
												<Trash2 class="h-4 w-4" />
											</Button>
										</div>
									</td>
								</tr>
							{/each}
						</tbody>
					</table>
				</div>
			{/if}
		{:else if activeTab === 'secrets'}
			<div class="flex flex-wrap items-center justify-between gap-3">
				<p class="text-sm text-[var(--text-secondary)]">
					Project-wide secrets plus pipeline-specific overrides for <strong>{pipeline.name}</strong>. Reference in
					YAML with
					<code class="rounded bg-[var(--bg-tertiary)] px-1 font-mono text-xs"
						>stored: &#123; name: MY_TOKEN &#125;</code
					>.
				</p>
				<div class="flex flex-wrap gap-2">
					<Button variant="outline" size="sm" href="/projects/{pipeline.project_id}">
						Project secrets
					</Button>
					<Button variant="ghost" size="sm" onclick={loadPipelineSecrets} loading={secretsLoading}>
						<RefreshCw class="h-4 w-4" />
						Refresh
					</Button>
					<Button variant="primary" size="sm" onclick={openCreateSecret}>
						<Plus class="h-4 w-4" />
						Add secret
					</Button>
				</div>
			</div>
			{#if secretsError}
				<Alert variant="error" title="Secrets" dismissible ondismiss={() => (secretsError = null)}>
					{secretsError}
				</Alert>
			{/if}
			{#if secretsLoading && pipelineSecrets.length === 0}
				<Card>
					<div class="space-y-3 p-4">
						{#each Array(3) as _, i (i)}
							<Skeleton class="h-10 w-full" />
						{/each}
					</div>
				</Card>
			{:else if pipelineSecrets.length === 0}
				<Card>
					<EmptyState
						title="No secrets for this pipeline yet"
						description="Add a project-wide secret or one scoped to this pipeline. Pipeline-scoped values override the same name at project scope."
					>
						<Button variant="primary" onclick={openCreateSecret}>
							<Plus class="h-4 w-4" />
							Add secret
						</Button>
					</EmptyState>
				</Card>
			{:else}
				<div class="overflow-hidden rounded-lg border border-[var(--border-primary)]">
					<table class="w-full text-sm">
						<thead class="bg-[var(--bg-tertiary)]">
							<tr>
								<th class="px-4 py-3 text-left font-medium text-[var(--text-secondary)]">Name</th>
								<th class="px-4 py-3 text-left font-medium text-[var(--text-secondary)]">Kind</th>
								<th class="px-4 py-3 text-left font-medium text-[var(--text-secondary)]">Scope</th>
								<th class="px-4 py-3 text-left font-medium text-[var(--text-secondary)]">Version</th>
								<th class="px-4 py-3 text-left font-medium text-[var(--text-secondary)]">Updated</th>
								<th class="px-4 py-3 text-right font-medium text-[var(--text-secondary)]">Actions</th>
							</tr>
						</thead>
						<tbody class="divide-y divide-[var(--border-secondary)]">
							{#each pipelineSecrets as s (s.id)}
								<tr class="bg-[var(--bg-secondary)]">
									<td class="px-4 py-3 font-mono text-sm">{s.path}</td>
									<td class="px-4 py-3">{s.kind}</td>
									<td class="px-4 py-3">{storedSecretScopeLabel(s)}</td>
									<td class="px-4 py-3 font-mono">
										<button
											type="button"
											class="text-primary-600 hover:underline dark:text-primary-400"
											onclick={() => openSecretVersions(s)}
										>
											v{s.version}
										</button>
									</td>
									<td class="px-4 py-3 text-[var(--text-secondary)]">
										{formatRelativeTime(s.updated_at)}
									</td>
									<td class="px-4 py-3 text-right">
										<div class="flex justify-end gap-2">
											<Button
												variant="ghost"
												size="sm"
												title="Versions, roll back, purge"
												onclick={() => openSecretVersions(s)}
											>
												<History class="h-4 w-4" />
											</Button>
											<Button
												variant="ghost"
												size="sm"
												onclick={() => {
													rotateTarget = s;
													rotateValue = '';
													showRotateSecretDialog = true;
												}}
											>
												Rotate
											</Button>
											<Button
												variant="ghost"
												size="sm"
												onclick={() => {
													deleteTarget = s;
													showDeleteSecretDialog = true;
												}}
											>
												<Trash2 class="h-4 w-4" />
											</Button>
										</div>
									</td>
								</tr>
							{/each}
						</tbody>
					</table>
				</div>
			{/if}
		{:else if activeTab === 'definition'}
			<Card>
				<div class="space-y-4">
					<div class="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
						<h3 class="font-medium text-[var(--text-primary)]">Pipeline Definition</h3>
						{#if pipeline.definition_path}
							<span class="text-sm text-[var(--text-secondary)]">
								Imported path: <code class="font-mono">{pipeline.definition_path}</code>
							</span>
						{/if}
					</div>

					{#if pipeline.scm_provider === 'github' && pipeline.scm_repository?.trim()}
						<div
							class="rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm text-[var(--text-secondary)]"
						>
							<p class="font-medium text-[var(--text-primary)]">Git source</p>
							<p class="mt-1 font-mono text-xs text-[var(--text-tertiary)]">
								{pipeline.scm_repository.trim()}
							</p>
							<p class="mt-2 flex flex-wrap gap-x-4 gap-y-1">
								{#if pipeline.scm_ref?.trim()}
									<span>
										Branch / tag:
										<code class="font-mono text-[var(--text-primary)]">{pipeline.scm_ref.trim()}</code>
									</span>
								{/if}
								{#if pipeline.scm_revision?.trim()}
									<span>
										Revision:
										<code class="font-mono text-[var(--text-primary)]">{shortRev(pipeline.scm_revision)}</code>
										<span class="sr-only">({pipeline.scm_revision.trim()})</span>
									</span>
								{/if}
							</p>
							{#if pipelineGithubTreeUrl}
								<p class="mt-2">
									<a
										href={pipelineGithubTreeUrl}
										target="_blank"
										rel="noopener noreferrer"
										class="inline-flex items-center gap-1 text-sm font-medium text-primary-600 hover:underline dark:text-primary-400"
									>
										<ExternalLink class="h-3.5 w-3.5" />
										Browse repository at this ref
									</a>
								</p>
							{:else if !pipelineGithubRef}
								<p class="mt-2 text-xs text-amber-600 dark:text-amber-400">
									Upstream links need a stored Git revision or ref on this pipeline.
								</p>
							{/if}
						</div>
					{/if}

					{#if pipelineSourceRows.length > 0}
						<div>
							<h4 class="mb-2 text-sm font-medium text-[var(--text-primary)]">Source files</h4>
							<div class="overflow-x-auto rounded-lg border border-[var(--border-primary)]">
								<table class="w-full min-w-[28rem] text-left text-sm">
									<thead class="border-b border-[var(--border-primary)] bg-[var(--bg-tertiary)] text-[var(--text-secondary)]">
										<tr>
											<th class="px-3 py-2 font-medium">File</th>
											<th class="px-3 py-2 font-medium">Path in repo</th>
											<th class="px-3 py-2 font-medium text-right">Upstream</th>
										</tr>
									</thead>
									<tbody class="divide-y divide-[var(--border-primary)]">
										{#each pipelineSourceRows as row, i (row.kind + (row.repoPath ?? row.workflowRef ?? '') + row.label + String(i))}
											{@const upstream = upstreamLinkForRow(pipeline, row)}
											<tr class="bg-[var(--bg-primary)]">
												<td class="px-3 py-2 text-[var(--text-primary)]">
													<div class="max-w-[16rem] truncate font-medium sm:max-w-md" title={row.label}>
														{row.label}
													</div>
													<div class="mt-0.5 text-xs text-[var(--text-tertiary)]">
														{row.kind === 'pipeline'
															? 'Root pipeline'
															: row.kind === 'workflow_project'
																? 'Reusable workflow (project scope)'
																: 'Reusable workflow (global scope)'}
													</div>
												</td>
												<td class="px-3 py-2 font-mono text-xs text-[var(--text-secondary)]">
													{#if row.repoPath}
														{row.repoPath}
													{:else}
														<span class="text-[var(--text-tertiary)]">—</span>
													{/if}
												</td>
												<td class="px-3 py-2 text-right">
													{#if upstream}
														<a
															href={upstream}
															target="_blank"
															rel="noopener noreferrer"
															class="inline-flex h-8 items-center gap-1.5 rounded-lg border border-secondary-300 px-3 text-sm font-medium text-secondary-700 hover:bg-secondary-100 dark:border-secondary-600 dark:text-secondary-300 dark:hover:bg-secondary-800"
														>
															<ExternalLink class="h-3.5 w-3.5 shrink-0" />
															View upstream
														</a>
													{:else}
														<Button
															variant="outline"
															size="sm"
															disabled
															title={row.kind === 'workflow_global'
																? 'Global workflows are not stored as files in this repository.'
																: pipeline.scm_provider !== 'github'
																	? 'Upstream links are only built for GitHub-backed pipelines.'
																	: 'No Git ref available to build an upstream URL.'}
														>
															View upstream
														</Button>
													{/if}
												</td>
											</tr>
										{/each}
									</tbody>
								</table>
							</div>
						</div>
					{/if}

					<div>
						<h4 class="mb-2 text-sm font-medium text-[var(--text-primary)]">Definition (YAML)</h4>
						{#if pipelineDefinitionYaml}
							<pre class="overflow-x-auto rounded-lg bg-[var(--bg-tertiary)] p-4 text-sm"><code
								>{pipelineDefinitionYaml}</code></pre>
						{:else}
							<p class="text-sm text-[var(--text-secondary)]">Could not render definition as YAML.</p>
						{/if}
					</div>
				</div>
			</Card>

			{#if definitionJobs(pipeline.definition).length > 0}
				<Card>
					<h3 class="mb-4 font-medium text-[var(--text-primary)]">Job Graph</h3>
					<DagViewer jobs={definitionJobs(pipeline.definition)} />
				</Card>
			{/if}
		{/if}
	{/if}
</div>

<Dialog
	bind:open={showEditPipelineDialog}
	title="Edit pipeline"
	class="max-w-lg"
	onclose={() => {
		editPipelineLoading = false;
	}}
>
	{#if pipeline}
		<div class="space-y-4">
			<p class="text-sm text-[var(--text-secondary)]">
				<strong>Owners and groups</strong> apply to the whole project, not individual pipelines. Manage them on the
				<Button variant="ghost" size="sm" class="h-auto px-1 py-0 text-primary-600" href="/projects/{pipeline.project_id}">
					project page
				</Button>
				.
			</p>
			<div>
				<label class="mb-1 block text-sm font-medium" for="ep-name">Name</label>
				<Input id="ep-name" bind:value={epName} placeholder="Pipeline display name" />
			</div>
			<div>
				<label class="mb-1 block text-sm font-medium" for="ep-slug">Slug</label>
				<Input id="ep-slug" value={pipeline.slug} readonly class="bg-[var(--bg-tertiary)]" />
				<p class="mt-1 text-xs text-[var(--text-tertiary)]">Slug is fixed after creation (used in URLs and APIs).</p>
			</div>
			<div>
				<label class="mb-1 block text-sm font-medium" for="ep-desc">Description</label>
				<textarea
					id="ep-desc"
					bind:value={epDescription}
					rows="3"
					class="w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm text-[var(--text-primary)] focus:outline-none focus:ring-2 focus:ring-primary-500"
					placeholder="Optional summary shown on the pipeline page"
				></textarea>
			</div>
			<label class="flex items-center gap-2 text-sm">
				<input type="checkbox" bind:checked={epEnabled} class="rounded border-[var(--border-primary)]" />
				Enabled (allow new runs)
			</label>
			{#if pipeline.scm_provider}
				<div class="space-y-3 rounded-lg border border-[var(--border-primary)] bg-[var(--bg-tertiary)] p-3">
					<p class="text-xs font-medium text-[var(--text-secondary)]">
						Git source ({pipeline.scm_provider})
					</p>
					<p class="text-xs text-[var(--text-tertiary)]">
						Last synced revision is not edited here — use &quot;Sync from source code&quot; after changing ref or path.
					</p>
					<div>
						<label class="mb-1 block text-xs font-medium" for="ep-scm-repo">Repository</label>
						<Input id="ep-scm-repo" bind:value={epScmRepository} placeholder="owner/repo" />
					</div>
					<div>
						<label class="mb-1 block text-xs font-medium" for="ep-scm-ref">Git ref</label>
						<Input id="ep-scm-ref" bind:value={epScmRef} placeholder="branch, tag, or SHA" />
					</div>
					<div>
						<label class="mb-1 block text-xs font-medium" for="ep-scm-path">Path to pipeline YAML</label>
						<Input id="ep-scm-path" bind:value={epScmPath} placeholder=".stable/pipeline.yaml" />
					</div>
					<div>
						<label class="mb-1 block text-xs font-medium" for="ep-scm-creds"
							>Credentials secret path</label
						>
						<Input
							id="ep-scm-creds"
							bind:value={epScmCredsPath}
							placeholder="builtin_secrets path for github_app"
						/>
					</div>
				</div>
			{/if}
			<div class="flex justify-end gap-2 pt-2">
				<Button variant="outline" onclick={() => (showEditPipelineDialog = false)}>Cancel</Button>
				<Button variant="primary" onclick={submitEditPipeline} loading={editPipelineLoading} disabled={!epName.trim()}>
					Save
				</Button>
			</div>
		</div>
	{/if}
</Dialog>

<Dialog bind:open={showCreateVariable} title="Add environment variable">
	<div class="space-y-4">
		<div>
			<label class="mb-1 block text-sm font-medium" for="pv-name">Name</label>
			<Input id="pv-name" bind:value={cvName} placeholder="e.g. NODE_VERSION" />
		</div>
		<div>
			<label class="mb-1 block text-sm font-medium" for="pv-val">Value</label>
			<Input id="pv-val" bind:value={cvValue} />
		</div>
		<label class="flex items-center gap-2 text-sm">
			<input type="checkbox" bind:checked={cvSensitive} class="rounded border-[var(--border-primary)]" />
			Mask value in API responses (sensitive)
		</label>
		<div>
			<label class="mb-1 block text-sm font-medium" for="pv-scope">Scope</label>
			<Select id="pv-scope" options={pipelineVarScopeOptions} bind:value={cvPipelineId} />
		</div>
		<div class="flex justify-end gap-2 pt-2">
			<Button variant="outline" onclick={() => (showCreateVariable = false)}>Cancel</Button>
			<Button
				variant="primary"
				onclick={submitCreateVariable}
				loading={variableActionLoading}
				disabled={!cvName.trim()}
			>
				Save
			</Button>
		</div>
	</div>
</Dialog>

<Dialog
	bind:open={showEditVariableDialog}
	title="Edit variable"
	onclose={() => {
		editVariableTarget = null;
	}}
>
	{#if editVariableTarget}
		<div class="space-y-4">
			<div>
				<label class="mb-1 block text-sm font-medium" for="pev-name">Name</label>
				<Input id="pev-name" bind:value={evName} />
			</div>
			<div>
				<label class="mb-1 block text-sm font-medium" for="pev-val">New value</label>
				<Input
					id="pev-val"
					bind:value={evValue}
					placeholder={editVariableTarget.is_sensitive ? 'Leave blank to keep current value' : ''}
				/>
			</div>
			<label class="flex items-center gap-2 text-sm">
				<input type="checkbox" bind:checked={evSensitive} class="rounded border-[var(--border-primary)]" />
				Mask value in API responses
			</label>
			<div class="flex justify-end gap-2 pt-2">
				<Button variant="outline" onclick={() => (showEditVariableDialog = false)}>Cancel</Button>
				<Button variant="primary" onclick={submitEditVariable} loading={variableActionLoading}>
					Save
				</Button>
			</div>
		</div>
	{/if}
</Dialog>

<Dialog
	bind:open={showDeleteVariableDialog}
	title="Delete variable?"
	onclose={() => {
		deleteVariableTarget = null;
	}}
>
	{#if deleteVariableTarget}
		<p class="text-sm text-[var(--text-secondary)]">
			Delete <span class="font-mono">{deleteVariableTarget.name}</span>?
		</p>
		<div class="mt-6 flex justify-end gap-2">
			<Button variant="outline" onclick={() => (showDeleteVariableDialog = false)}>Cancel</Button>
			<Button
				variant="primary"
				class="bg-red-600 hover:bg-red-700"
				onclick={submitDeleteVariable}
				loading={variableActionLoading}
			>
				Delete
			</Button>
		</div>
	{/if}
</Dialog>

<Dialog bind:open={showCreateSecret} title="Add stored secret">
	<div class="space-y-4">
		<div>
			<label class="mb-1 block text-sm font-medium text-[var(--text-primary)]" for="pl-sec-path"
				>Logical name</label
			>
			<Input id="pl-sec-path" bind:value={createPath} placeholder="e.g. MY_API_TOKEN" />
		</div>
		<div>
			<label class="mb-1 block text-sm font-medium text-[var(--text-primary)]" for="pl-sec-kind">Kind</label
			>
			<Select id="pl-sec-kind" options={kindOptions} bind:value={createKind} />
		</div>
		{#if createKind === 'github_app'}
			<div class="space-y-3 rounded-lg border border-[var(--border-primary)] bg-[var(--bg-tertiary)] p-3">
				<p class="text-xs text-[var(--text-secondary)]">
					Paste GitHub App credentials. Values are encrypted; the private key is only used to mint short-lived tokens.
				</p>
				<div class="grid gap-3 sm:grid-cols-2">
					<div>
						<label class="mb-1 block text-xs font-medium" for="pl-gh-app">App ID</label>
						<Input id="pl-gh-app" bind:value={ghAppId} placeholder="123456" />
					</div>
					<div>
						<label class="mb-1 block text-xs font-medium" for="pl-gh-inst">Installation ID</label>
						<Input id="pl-gh-inst" bind:value={ghInstallationId} placeholder="78901234" />
					</div>
				</div>
				<div>
					<label class="mb-1 block text-xs font-medium" for="pl-gh-pem">Private key (PEM)</label>
					<textarea
						id="pl-gh-pem"
						bind:value={ghPrivateKey}
						rows="6"
						class="w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 font-mono text-xs text-[var(--text-primary)] focus:outline-none focus:ring-2 focus:ring-primary-500"
						placeholder="-----BEGIN RSA PRIVATE KEY----- ..."
					></textarea>
				</div>
				<div>
					<label class="mb-1 block text-xs font-medium" for="pl-gh-base">GitHub API base (optional)</label>
					<Input id="pl-gh-base" bind:value={ghApiBase} placeholder="https://api.github.com (default)" />
				</div>
				<div>
					<label class="mb-1 block text-xs font-medium" for="pl-gh-extra">Additional fields (optional JSON)</label
					>
					<textarea
						id="pl-gh-extra"
						bind:value={ghExtraJson}
						rows="3"
						class="w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 font-mono text-xs text-[var(--text-primary)] focus:outline-none focus:ring-2 focus:ring-primary-500"
						placeholder={`{\n  "client_id": "..."\n}`}
					></textarea>
				</div>
			</div>
		{:else}
			<div>
				<label class="mb-1 block text-sm font-medium text-[var(--text-primary)]" for="pl-sec-val"
					>Value (one-time)</label
				>
				<textarea
					id="pl-sec-val"
					bind:value={createValue}
					rows="4"
					class="w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm text-[var(--text-primary)] focus:outline-none focus:ring-2 focus:ring-primary-500"
					placeholder="Secret value or PEM / JSON payload"
				></textarea>
			</div>
		{/if}
		<div>
			<label class="mb-1 block text-sm font-medium text-[var(--text-primary)]" for="pl-sec-desc"
				>Description (optional)</label
			>
			<Input id="pl-sec-desc" bind:value={createDescription} />
		</div>
		<div>
			<label class="mb-1 block text-sm font-medium text-[var(--text-primary)]" for="pl-sec-scope">Scope</label>
			<Select id="pl-sec-scope" options={pipelineVarScopeOptions} bind:value={secScopePipelineId} />
		</div>
		<div class="flex justify-end gap-2 pt-2">
			<Button variant="outline" onclick={() => (showCreateSecret = false)}>Cancel</Button>
			<Button
				variant="primary"
				onclick={submitCreateSecret}
				loading={secretActionLoading}
				disabled={!createSecretValid()}
			>
				Save
			</Button>
		</div>
	</div>
</Dialog>

<Dialog
	bind:open={showRotateSecretDialog}
	title="Rotate secret"
	onclose={() => {
		rotateTarget = null;
		rotateValue = '';
	}}
>
	{#if rotateTarget}
		<p class="text-sm text-[var(--text-secondary)]">
			New value for <span class="font-mono text-[var(--text-primary)]">{rotateTarget.path}</span> (creates a new
			version).
		</p>
		{#if rotateTarget.kind === 'github_app'}
			<p class="mt-2 text-xs text-[var(--text-secondary)]">
				Use a single JSON object with <code class="font-mono">app_id</code>, <code class="font-mono"
					>installation_id</code
				>, <code class="font-mono">private_key_pem</code>, optional <code class="font-mono"
					>github_api_base</code
				>, and any other fields you need preserved.
			</p>
		{/if}
		<div class="mt-4">
			<textarea
				bind:value={rotateValue}
				rows={rotateTarget.kind === 'github_app' ? 14 : 4}
				class="w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 font-mono text-sm text-[var(--text-primary)] focus:outline-none focus:ring-2 focus:ring-primary-500"
			></textarea>
		</div>
		<div class="mt-6 flex justify-end gap-2">
			<Button
				variant="outline"
				onclick={() => {
					showRotateSecretDialog = false;
					rotateTarget = null;
					rotateValue = '';
				}}
			>
				Cancel
			</Button>
			<Button
				variant="primary"
				onclick={submitRotateSecret}
				loading={secretActionLoading}
				disabled={!rotateValue?.trim()}
			>
				Rotate
			</Button>
		</div>
	{/if}
</Dialog>

<Dialog
	bind:open={showDeleteSecretDialog}
	title="Delete secret?"
	onclose={() => {
		deleteTarget = null;
	}}
>
	{#if deleteTarget}
		<p class="text-sm text-[var(--text-secondary)]">
			Soft-delete <span class="font-mono">{deleteTarget.path}</span>? Runs that still need it may fail validation.
		</p>
		<div class="mt-6 flex justify-end gap-2">
			<Button variant="outline" onclick={() => (showDeleteSecretDialog = false)}>Cancel</Button>
			<Button
				variant="primary"
				class="bg-red-600 hover:bg-red-700"
				onclick={submitDeleteSecret}
				loading={secretActionLoading}
			>
				Delete
			</Button>
		</div>
	{/if}
</Dialog>

<Dialog
	bind:open={showSecretVersionsDialog}
	title="Secret versions"
	onclose={() => {
		versionsContext = null;
		secretVersionRows = [];
	}}
>
	{#if versionsContext}
		<p class="text-sm text-[var(--text-secondary)]">
			<span class="font-mono text-[var(--text-primary)]">{versionsContext.path}</span>
			· {storedSecretScopeLabel(versionsContext)}
		</p>
		<p class="mt-2 text-xs text-[var(--text-tertiary)]">
			Roll back soft-deletes newer ciphertext rows so jobs resolve this version. Purge permanently removes one row from
			the database (including soft-deleted rows).
		</p>
		{#if versionsError}
			<div class="mt-3 rounded-lg border border-red-200 bg-red-50 p-2 text-sm text-red-700 dark:border-red-900 dark:bg-red-950/50 dark:text-red-400">
				{versionsError}
			</div>
		{/if}
		<div class="mt-3 flex justify-end">
			<Button variant="ghost" size="sm" onclick={refreshSecretVersions} loading={versionsLoading}>
				<RefreshCw class="h-4 w-4" />
				Refresh
			</Button>
		</div>
		{#if versionsLoading && secretVersionRows.length === 0}
			<div class="mt-4 space-y-2">
				{#each Array(3) as _, i (i)}
					<Skeleton class="h-10 w-full" />
				{/each}
			</div>
		{:else if secretVersionRows.length === 0}
			<p class="mt-4 text-sm text-[var(--text-secondary)]">No versions found.</p>
		{:else}
			<div class="mt-4 max-h-80 overflow-auto rounded-lg border border-[var(--border-primary)]">
				<table class="w-full text-sm">
					<thead class="sticky top-0 bg-[var(--bg-tertiary)]">
						<tr>
							<th class="px-3 py-2 text-left font-medium text-[var(--text-secondary)]">Ver</th>
							<th class="px-3 py-2 text-left font-medium text-[var(--text-secondary)]">Updated</th>
							<th class="px-3 py-2 text-left font-medium text-[var(--text-secondary)]">Row</th>
							<th class="px-3 py-2 text-right font-medium text-[var(--text-secondary)]">Actions</th>
						</tr>
					</thead>
					<tbody class="divide-y divide-[var(--border-secondary)]">
						{#each secretVersionRows as row, idx (row.id)}
							<tr class="bg-[var(--bg-secondary)]">
								<td class="px-3 py-2 font-mono">
									v{row.version}
									{#if idx === 0}
										<Badge variant="success" size="sm" class="ml-2">Current</Badge>
									{/if}
								</td>
								<td class="px-3 py-2 text-[var(--text-secondary)]">
									{formatRelativeTime(row.updated_at)}
								</td>
								<td class="px-3 py-2 font-mono text-xs text-[var(--text-tertiary)]">
									{row.id.slice(0, 8)}…
								</td>
								<td class="px-3 py-2 text-right">
									<div class="flex flex-wrap justify-end gap-1">
										{#if idx > 0}
											<Button
												variant="outline"
												size="sm"
												onclick={() => submitActivateSecretVersion(row)}
												disabled={secretActionLoading}
											>
												Roll back here
											</Button>
										{/if}
										<Button
											variant="ghost"
											size="sm"
											class="text-red-600 hover:bg-red-50 dark:text-red-400 dark:hover:bg-red-950/40"
											onclick={() => {
												purgeVersionTarget = row;
												showPurgeVersionDialog = true;
											}}
											disabled={secretActionLoading}
										>
											Purge
										</Button>
									</div>
								</td>
							</tr>
						{/each}
					</tbody>
				</table>
			</div>
		{/if}
		<div class="mt-4 flex justify-end">
			<Button variant="outline" onclick={() => (showSecretVersionsDialog = false)}>Close</Button>
		</div>
	{/if}
</Dialog>

<Dialog
	bind:open={showPurgeVersionDialog}
	title="Purge version permanently?"
	onclose={() => {
		purgeVersionTarget = null;
	}}
>
	{#if purgeVersionTarget}
		<p class="text-sm text-[var(--text-secondary)]">
			Remove version <span class="font-mono">v{purgeVersionTarget.version}</span> row
			<span class="font-mono text-xs">{purgeVersionTarget.id}</span> from the database? This cannot be undone.
		</p>
		<div class="mt-6 flex justify-end gap-2">
			<Button variant="outline" onclick={() => (showPurgeVersionDialog = false)}>Cancel</Button>
			<Button
				variant="primary"
				class="bg-red-600 hover:bg-red-700"
				onclick={submitPurgeSecretVersion}
				loading={secretActionLoading}
			>
				Purge permanently
			</Button>
		</div>
	{/if}
</Dialog>

<Dialog
	bind:open={showCreateTriggerDialog}
	title="Add webhook trigger"
	description="Creates a pipeline trigger. Webhook payload fields are mapped to run variables (see API docs)."
	onclose={() => {
		createdTriggerSecret = null;
	}}
>
	<div class="space-y-4">
		<div>
			<label class="mb-1 block text-sm font-medium text-[var(--text-primary)]" for="nt-desc"
				>Description (optional)</label
			>
			<Input id="nt-desc" bind:value={ntDescription} placeholder="e.g. Deploy hook from CI" />
		</div>
		<div class="grid gap-4 sm:grid-cols-2">
			<div>
				<label class="mb-1 block text-sm font-medium text-[var(--text-primary)]" for="nt-branches"
					>Branches (comma-separated)</label
				>
				<Input id="nt-branches" bind:value={ntBranches} />
			</div>
			<div>
				<label class="mb-1 block text-sm font-medium text-[var(--text-primary)]" for="nt-events"
					>Events (comma-separated)</label
				>
				<Input id="nt-events" bind:value={ntEvents} />
			</div>
		</div>
		<label class="flex cursor-pointer items-center gap-2 text-sm text-[var(--text-secondary)]">
			<input type="checkbox" bind:checked={ntFlatten} class="rounded border-[var(--border-primary)]" />
			Flatten top-level JSON keys into variables
		</label>
		<div>
			<label class="mb-1 block text-sm font-medium text-[var(--text-primary)]" for="nt-raw"
				>Raw body variable name (optional)</label
			>
			<Input
				id="nt-raw"
				bind:value={ntRawBodyVar}
				placeholder="e.g. WEBHOOK_JSON"
				class="font-mono text-sm"
			/>
		</div>
		<div class="rounded-lg border border-[var(--border-primary)] bg-[var(--bg-tertiary)] p-3">
			<label class="flex cursor-pointer items-center gap-2 text-sm font-medium text-[var(--text-primary)]">
				<input type="checkbox" bind:checked={ntGenerateSecret} class="rounded border-[var(--border-primary)]" />
				Generate webhook secret (shown once after save)
			</label>
			{#if !ntGenerateSecret}
				<div class="mt-3">
					<label class="mb-1 block text-xs text-[var(--text-secondary)]" for="nt-sec">Webhook secret</label>
					<div class="flex flex-wrap items-stretch gap-2">
						<Input
							id="nt-sec"
							type="password"
							bind:value={ntSecretManual}
							autocomplete="off"
							class="min-w-0 flex-1"
						/>
						<Button
							type="button"
							variant="outline"
							size="sm"
							class="shrink-0"
							onclick={() => applyRandomWebhookSecret((v) => (ntSecretManual = v))}
						>
							<Shuffle class="h-4 w-4" aria-hidden="true" />
							Random secret
						</Button>
					</div>
				</div>
			{/if}
		</div>
		<div class="flex justify-end gap-2 pt-2">
			<Button variant="outline" onclick={() => (showCreateTriggerDialog = false)}>Cancel</Button>
			<Button variant="primary" onclick={submitCreateTrigger} loading={triggerActionLoading}>
				Create
			</Button>
		</div>
	</div>
</Dialog>

<Dialog
	bind:open={showEditTriggerDialog}
	title="Edit trigger"
	description={editTriggerTarget && triggerIsRepoManaged(editTriggerTarget)
		? 'Repo-managed: you can change status, description, and HMAC secret. Filters live in YAML.'
		: 'Update trigger settings or rotate the webhook secret.'}
	onclose={() => {
		editTriggerTarget = null;
	}}
>
	{#if editTriggerTarget}
		<div class="space-y-4">
			<label class="flex cursor-pointer items-center gap-2 text-sm text-[var(--text-secondary)]">
				<input type="checkbox" bind:checked={etEnabled} class="rounded border-[var(--border-primary)]" />
				Enabled
			</label>
			<div>
				<label class="mb-1 block text-sm font-medium text-[var(--text-primary)]" for="et-desc"
					>Description</label
				>
				<Input id="et-desc" bind:value={etDescription} />
			</div>
			<div>
				<label class="mb-1 block text-sm font-medium text-[var(--text-primary)]" for="et-rosecret"
					>New webhook secret (optional)</label
				>
				<div class="flex flex-wrap items-stretch gap-2">
					<Input
						id="et-rosecret"
						type="password"
						bind:value={etNewSecret}
						autocomplete="off"
						class="min-w-0 flex-1"
					/>
					<Button
						type="button"
						variant="outline"
						size="sm"
						class="shrink-0"
						onclick={() => applyRandomWebhookSecret((v) => (etNewSecret = v))}
					>
						<Shuffle class="h-4 w-4" aria-hidden="true" />
						Random secret
					</Button>
				</div>
				<p class="mt-1 text-xs text-[var(--text-tertiary)]">
					Leaving blank keeps the current secret. Use a strong random value (compatible with GitHub-style HMAC).
				</p>
			</div>
			<div class="flex justify-end gap-2 pt-2">
				<Button variant="outline" onclick={() => (showEditTriggerDialog = false)}>Cancel</Button>
				<Button variant="primary" onclick={submitEditTrigger} loading={triggerActionLoading}>
					Save
				</Button>
			</div>
		</div>
	{/if}
</Dialog>

<Dialog
	bind:open={showDeleteTriggerDialog}
	title="Delete trigger?"
	onclose={() => {
		deleteTriggerTarget = null;
	}}
>
	{#if deleteTriggerTarget}
		<p class="text-sm text-[var(--text-secondary)]">
			Delete this <span class="font-mono">{deleteTriggerTarget.kind}</span> trigger
			<span class="font-mono text-xs">{deleteTriggerTarget.id}</span>? External callers using its URL will get 404.
		</p>
		<div class="mt-6 flex justify-end gap-2">
			<Button variant="outline" onclick={() => (showDeleteTriggerDialog = false)}>Cancel</Button>
			<Button
				variant="primary"
				class="bg-red-600 hover:bg-red-700"
				onclick={submitDeleteTrigger}
				loading={triggerActionLoading}
			>
				Delete
			</Button>
		</div>
	{/if}
</Dialog>

<Dialog
	bind:open={showWebhookHelpModal}
	title="Authenticated webhook requests"
	description="When a trigger has a secret, Meticulous expects a GitHub-style HMAC over the raw POST body."
	class="max-h-[90vh] max-w-3xl overflow-hidden sm:max-w-3xl"
	onclose={() => {}}
>
	<div
		class="max-h-[min(70vh,640px)] space-y-5 overflow-y-auto pr-1 text-sm text-secondary-600 dark:text-secondary-400"
	>
		<p>
			Send
			<code class="rounded bg-secondary-100 px-1 font-mono text-xs dark:bg-secondary-800">POST</code>
			to your webhook URL
			<code class="rounded bg-secondary-100 px-1 font-mono text-[11px] dark:bg-secondary-800"
				>/api/v1/webhooks/&lt;org_id&gt;/&lt;trigger_id&gt;</code
			>. Use the org id and trigger id shown in this app (same values as in the URL column). The body should be
			<strong>JSON</strong>; keys can become run variables when
			<em>flatten top-level</em>
			is enabled on the trigger.
		</p>
		<p>
			If the trigger has a <strong>secret</strong>, add header
			<code class="rounded bg-secondary-100 px-1 font-mono text-[11px] dark:bg-secondary-800"
				>X-Hub-Signature-256: sha256=&lt;hex&gt;</code
			>
			where
			<code class="font-mono text-xs">&lt;hex&gt;</code>
			is the <strong>lowercase</strong> hex digest of
			<code class="font-mono text-xs">HMAC-SHA256(secret, raw_body_bytes)</code>. If there is no secret, omit the header.
		</p>
		<p class="text-xs text-secondary-500 dark:text-secondary-500">
			The signed bytes must match the request body exactly (watch for extra newlines when piping into curl).
		</p>

		<div
			class="overflow-hidden rounded-lg border border-secondary-200 bg-secondary-50/30 dark:border-secondary-700 dark:bg-secondary-950/40"
		>
			<div class="px-3 pt-1">
				<Tabs items={webhookHelpSnippetTabs} bind:value={webhookHelpCodeTab} class="!border-b-0" />
			</div>
			<div class="border-t border-secondary-200 p-3 dark:border-secondary-700">
				{#key webhookHelpCodeTab}
					<HighlightedCodeBlock
						source={webhookHelpActiveSnippet.source}
						language={webhookHelpActiveSnippet.language}
						ariaLabel={`Webhook signing example (${webhookHelpCodeTab})`}
					/>
				{/key}
			</div>
		</div>
	</div>
	<div class="mt-4 border-t border-secondary-200 pt-4 dark:border-secondary-700">
		<Button variant="outline" class="w-full sm:w-auto" onclick={() => (showWebhookHelpModal = false)}>
			Close
		</Button>
	</div>
</Dialog>
