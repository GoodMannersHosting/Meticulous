<script lang="ts">
	import { getPublicApiBase } from '$lib/public-api-base';
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { onDestroy } from 'svelte';
	import { breadcrumbTrail } from '$lib/stores/breadcrumb-trail';
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
	import YamlCodeBlock from '$lib/components/code/YamlCodeBlock.svelte';
	import { DataTable, EmptyState, Skeleton } from '$components/data';
	import { apiMethods } from '$api/client';
	import type {
		Pipeline,
		PipelineJob,
		PipelineTrigger,
		ProjectVariable,
		ProjectWorkflowsAvailable,
		Run,
		StoredSecret,
		UpdatePipelineInput,
		UpdatePipelineTriggerInput,
		Environment,
		WorkflowDiagnosticItem
	} from '$api/types';
	import {
		getSecretRefFromMetadata,
		isRemoteRefSecretKind,
		kindAllowedByExternalPolicy,
		storedSecretValueFieldLabel,
		storedSecretValueHelpLine,
		storedSecretValuePlaceholder
	} from '$lib/utils/storedSecretUi';
	import { formatRelativeTime, truncateId } from '$utils/format';
	import { generateRandomWebhookSecret } from '$utils/webhookSecret';
	import {
		ArrowLeft,
		Play,
		Settings,
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
		CircleHelp,
		Shield,
		Layers,
		Archive,
		Edit,
		Check,
		ShieldAlert
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
		catalogWorkflowIdForPipelineSourceRow,
		collectPipelineSourceRows,
		githubRepoTreeUrl,
		pipelineGithubBlobRef,
		upstreamLinkForRow,
		type PipelineSourceRow
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
	/** Non-reactive guard — see `lastBootstrappedProjectId` on the project page. */
	let lastBootstrappedPipelineId: string | null = null;

	const pipelineTabIds = new Set([
		'runs',
		'variables',
		'secrets',
		'definition',
		'settings',
		'triggers',
		'access',
		'advanced'
	]);

	function setPipelineTab(tab: string) {
		activeTab = tab;
		const u = new URL($page.url.href);
		u.searchParams.set('tab', tab);
		void goto(`${u.pathname}${u.search}`, { replaceState: true, noScroll: true, keepFocus: true });
	}

	$effect(() => {
		const t = $page.url.searchParams.get('tab');
		if (t && pipelineTabIds.has(t) && t !== activeTab) {
			activeTab = t;
		}
	});
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
	let createEnvironmentId = $state('');
	let pipelineProjectEnvs = $state<Environment[]>([]);
	let projectPipelinesAll = $state<Pipeline[]>([]);
	let storedExternalKindPolicy = $state<Record<string, boolean> | null>(null);
	let secretsFilterEnvId = $state('');
	let ghAppId = $state('');
	let ghInstallationId = $state('');
	let ghPrivateKey = $state('');
	let ghApiBase = $state('');
	let ghExtraJson = $state('');
	let secretActionLoading = $state(false);
	let rotateTarget = $state<StoredSecret | null>(null);
	let rotateValue = $state('');
	let showRotateSecretDialog = $state(false);
	let showEditSecretScopeDialog = $state(false);
	let editScopeTarget = $state<StoredSecret | null>(null);
	let editScopePipelineId = $state('');
	let editScopeEnvironmentId = $state('');
	let editScopeDescription = $state('');
	let editScopePropagate = $state(true);
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

	let projectWorkflowsAvailable = $state<ProjectWorkflowsAvailable | null>(null);
	let projectWorkflowsAvailableLoading = $state(false);

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
	let ntAuthMode = $state('hmac');
	let ntQueryParamName = $state('token');
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
	let etAuthMode = $state('hmac');
	let etQueryParam = $state('token');
	let etNewSecret = $state('');
	let showDeleteTriggerDialog = $state(false);
	let deleteTriggerTarget = $state<PipelineTrigger | null>(null);
	let showWebhookHelpModal = $state(false);

	let workflowDiagItems = $state<WorkflowDiagnosticItem[]>([]);
	let workflowDiagLoading = $state(false);
	let workflowDiagError = $state<string | null>(null);
	let showWorkflowDiagModal = $state(false);
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

	const plSecretEnvironmentOptions = $derived([
		{ value: '', label: 'All environments (default)' },
		...pipelineProjectEnvs.map((e) => ({
			value: e.id,
			label: `${e.display_name} (${e.name})`
		}))
	]);

	const plSecretsEnvFilterOptions = $derived([
		{ value: '', label: 'All environments' },
		...pipelineProjectEnvs.map((e) => ({ value: e.id, label: e.display_name }))
	]);

	const runsPageSizeOptions = [
		{ value: '20', label: '20 per page' },
		{ value: '50', label: '50 per page' },
		{ value: '100', label: '100 per page' }
	];

	const allStoredSecretKindOptions = [
		{ value: 'kv', label: 'Key / value' },
		{ value: 'api_key', label: 'API key' },
		{ value: 'ssh_private_key', label: 'SSH private key (PEM)' },
		{ value: 'github_app', label: 'GitHub App' },
		{ value: 'x509_bundle', label: 'X.509 bundle' },
		{ value: 'registry', label: 'Container registry' },
		{ value: 'aws_sm', label: 'AWS Secrets Manager' },
		{ value: 'vault', label: 'HashiCorp Vault' },
		{ value: 'gcp_sm', label: 'GCP Secret Manager' },
		{ value: 'azure_kv', label: 'Azure Key Vault' },
		{ value: 'kubernetes', label: 'Kubernetes Secret' }
	];

	const kindOptions = $derived(
		allStoredSecretKindOptions.filter((o) =>
			kindAllowedByExternalPolicy(o.value, storedExternalKindPolicy)
		)
	);

	const pipelineSecretScopeEditOptions = $derived([
		{ value: '', label: 'Project-wide (all pipelines)' },
		...projectPipelinesAll.map((pl) => ({ value: pl.id, label: pl.name }))
	]);

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

	async function loadMatrix() {
		if (!pipeline) return;
		matrixLoading = true;
		try {
			matrixData = await apiMethods.pipelines.matrix(pipeline.id);
			if (matrixData.environments.length > 1) {
				runsViewMode = 'matrix';
			}
		} catch {
			matrixData = null;
		} finally {
			matrixLoading = false;
		}
	}

	async function loadPipelineMembers() {
		if (!pipeline) return;
		plMembersLoading = true;
		plMembersError = null;
		try {
			plMembers = await apiMethods.pipelineMembers.list(pipeline.id);
		} catch (e) {
			plMembersError = e instanceof Error ? e.message : 'Failed to load members';
		} finally {
			plMembersLoading = false;
		}
	}

	async function savePipelineAccess(batch: import('$api/types').AccessControlSaveBatch) {
		if (!pipeline) return;
		for (const principalId of batch.removePrincipalIds) {
			await apiMethods.pipelineMembers.remove(pipeline.id, principalId);
		}
		for (const u of batch.roleUpdates) {
			await apiMethods.pipelineMembers.updateRole(pipeline.id, u.principalId, { role: u.role });
		}
		for (const input of batch.adds) {
			await apiMethods.pipelineMembers.add(pipeline.id, input);
		}
		await loadPipelineMembers();
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
		{ id: 'variables', label: 'Variables', icon: Braces },
		{ id: 'secrets', label: 'Secrets', icon: KeyRound },
		{ id: 'definition', label: 'Definition', icon: Layers },
		{ id: 'settings', label: 'Settings', icon: Settings }
	];

	const plSettingsGroupIds = ['settings', 'triggers', 'access', 'advanced'];
	const isPlSettingsGroup = $derived(plSettingsGroupIds.includes(activeTab));

	const plSettingsSubTabs = [
		{ id: 'settings', label: 'General' },
		{ id: 'triggers', label: 'Triggers' },
		{ id: 'access', label: 'Access Controls' },
		{ id: 'advanced', label: 'Advanced' }
	];

	let plMembers = $state<import('$api/types').Member[]>([]);
	let plMembersLoading = $state(false);
	let plMembersError = $state<string | null>(null);

	let runsViewMode = $state<'list' | 'matrix'>('list');
	let matrixData = $state<import('$api/types').MatrixResponse | null>(null);
	let matrixLoading = $state(false);

	function apiPublicOrigin(): string {
		return getPublicApiBase();
	}

	function triggerConfigStr(t: PipelineTrigger, key: string): string | undefined {
		const v = t.config[key];
		return typeof v === 'string' ? v : undefined;
	}

	function triggerIsRepoManaged(t: PipelineTrigger): boolean {
		return triggerConfigStr(t, 'managed_by') === 'repo';
	}

	type TriggerInboundAuth = 'none' | 'hmac' | 'query';

	function triggerResolvedInboundAuth(t: PipelineTrigger): TriggerInboundAuth {
		const a = t.inbound_auth?.toLowerCase().trim();
		if (a === 'none' || a === 'query' || a === 'hmac') return a;
		const raw = triggerConfigStr(t, 'inbound_auth')?.toLowerCase().trim();
		if (raw === 'none' || raw === 'query' || raw === 'hmac') return raw;
		return t.secret_configured ? 'hmac' : 'none';
	}

	function triggerQueryParamName(t: PipelineTrigger): string {
		return (
			(typeof t.inbound_query_param === 'string' && t.inbound_query_param.trim()) ||
			triggerConfigStr(t, 'inbound_query_param') ||
			'token'
		);
	}

	const triggerInboundAuthOptions = [
		{ value: 'hmac', label: 'HMAC header (X-Hub-Signature-256)' },
		{ value: 'query', label: 'Query parameter (secret in URL)' },
		{ value: 'none', label: 'None (open — no verification)' }
	] as const;

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
		ntAuthMode = 'hmac';
		ntQueryParamName = 'token';
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
			config.inbound_auth = ntAuthMode;
			if (ntAuthMode === 'query') {
				config.inbound_query_param = ntQueryParamName.trim() || 'token';
			}
			let generateSecret = false;
			if (ntAuthMode !== 'none') {
				if (ntGenerateSecret) {
					generateSecret = true;
				} else if (ntSecretManual.trim()) {
					config.secret = ntSecretManual.trim();
				} else {
					triggersError =
						'For HMAC or query auth, either check “Generate secret” or paste a webhook secret.';
					return;
				}
			}
			const created = await apiMethods.triggers.create(pipeline.id, {
				kind: 'webhook',
				config,
				description: ntDescription.trim() || undefined,
				generate_webhook_secret: generateSecret
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
		etAuthMode = triggerResolvedInboundAuth(t);
		etQueryParam = triggerQueryParamName(t);
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
			const configPatch: Record<string, unknown> = {};
			const isRepo = triggerIsRepoManaged(editTriggerTarget);
			if (editTriggerTarget.kind === 'webhook' && !isRepo) {
				configPatch.inbound_auth = etAuthMode;
				configPatch.inbound_query_param =
					etAuthMode === 'query' ? etQueryParam.trim() || 'token' : null;
				if (etAuthMode === 'none') {
					configPatch.secret = null;
				} else {
					if (!editTriggerTarget.secret_configured && !etNewSecret.trim()) {
						triggersError =
							'HMAC or query auth requires a secret. Paste one or use Random secret.';
						return;
					}
					if (etNewSecret.trim()) {
						configPatch.secret = etNewSecret.trim();
					}
				}
				body.config_patch = configPatch;
			} else if (isRepo && etNewSecret.trim()) {
				configPatch.secret = etNewSecret.trim();
				body.config_patch = configPatch;
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

	onDestroy(() => {
		breadcrumbTrail.set(null);
	});

	$effect(() => {
		const pipelineId = $page.params.id;
		if (!pipelineId) return;
		if (pipelineId === lastBootstrappedPipelineId) return;
		lastBootstrappedPipelineId = pipelineId;
		void loadPipeline(pipelineId);
	});

	async function loadProjectWorkflowsAvailable() {
		const p = pipeline;
		if (!p) return;
		projectWorkflowsAvailableLoading = true;
		try {
			projectWorkflowsAvailable = await apiMethods.wfCatalog.listAvailableForProject(p.project_id);
		} catch {
			projectWorkflowsAvailable = null;
		} finally {
			projectWorkflowsAvailableLoading = false;
		}
	}

	function sourceFilesRowDisabledTitle(row: PipelineSourceRow): string {
		const p = pipeline;
		if (!p) return '';
		if (row.kind === 'pipeline') {
			if (p.scm_provider !== 'github') return 'Upstream links are only built for GitHub-backed pipelines.';
			return 'No Git ref available to build an upstream URL.';
		}
		if (projectWorkflowsAvailableLoading) return 'Loading catalog…';
		if (row.kind === 'workflow_global') {
			return 'Global workflows are not stored as files in this repository. Sync the workflow to the catalog to open its detail page.';
		}
		return 'Could not match this reference to a catalog workflow version (sync from Git or check the catalog).';
	}

	async function loadWorkflowDiagnostics(pipelineId: string) {
		workflowDiagLoading = true;
		workflowDiagError = null;
		try {
			workflowDiagItems = await apiMethods.pipelines.workflowDiagnostics(pipelineId);
		} catch (e) {
			workflowDiagError = e instanceof Error ? e.message : 'Failed to check linked workflows';
			workflowDiagItems = [];
		} finally {
			workflowDiagLoading = false;
		}
	}

	async function loadPipeline(pipelineId: string) {
		loading = true;
		error = null;
		workflowDiagItems = [];
		workflowDiagError = null;
		showWorkflowDiagModal = false;
		try {
			runsListOffset = 0;
			pipeline = await apiMethods.pipelines.get(pipelineId);
			projectPipelinesAll = [];
			try {
				const proj = await apiMethods.projects.get(pipeline.project_id);
				breadcrumbTrail.set([
					{ label: proj.name, href: `/projects/${proj.id}` },
					{ label: pipeline.name, href: `/pipelines/${pipeline.id}` }
				]);
			} catch {
				breadcrumbTrail.set([{ label: pipeline.name, href: `/pipelines/${pipeline.id}` }]);
			}
			await loadRuns({ offset: 0 });
			await loadTriggers();
			void loadWorkflowDiagnostics(pipelineId);
		} catch (e) {
			breadcrumbTrail.set(null);
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

	async function ensureStoredSecretPolicy() {
		if (storedExternalKindPolicy !== null) return;
		try {
			const pol = await apiMethods.storedSecretPolicy.get();
			storedExternalKindPolicy = pol.stored_secret_external_kinds;
		} catch {
			storedExternalKindPolicy = {};
		}
	}

	async function loadProjectPipelinesForEditScope() {
		const p = pipeline;
		if (!p) return;
		try {
			const acc: Pipeline[] = [];
			let cursor: string | undefined;
			do {
				const r = await apiMethods.pipelines.list({ project_id: p.project_id, per_page: 100, cursor });
				acc.push(...r.data);
				cursor = r.pagination.has_more ? r.pagination.next_cursor : undefined;
			} while (cursor);
			projectPipelinesAll = acc;
		} catch {
			projectPipelinesAll = [];
		}
	}

	async function loadPipelineSecrets() {
		if (!pipeline) return;
		secretsLoading = true;
		secretsError = null;
		try {
			await ensureStoredSecretPolicy();
			pipelineSecrets = await apiMethods.storedSecrets.list(pipeline.project_id, {
				pipeline_id: pipeline.id,
				...(secretsFilterEnvId ? { environment_id: secretsFilterEnvId } : {})
			});
		} catch (e) {
			secretsError = e instanceof Error ? e.message : 'Failed to load secrets';
			pipelineSecrets = [];
		} finally {
			secretsLoading = false;
		}
	}

	async function loadPipelineProjectEnvironments() {
		const p = pipeline;
		if (!p) return;
		try {
			pipelineProjectEnvs = await apiMethods.environments.list(p.project_id);
		} catch {
			pipelineProjectEnvs = [];
		}
	}

	$effect(() => {
		const p = pipeline;
		const _f = secretsFilterEnvId;
		if (activeTab !== 'secrets' || !p || loading) return;
		void (async () => {
			if (pipelineProjectEnvs.length === 0) {
				await loadPipelineProjectEnvironments();
			}
			if (projectPipelinesAll.length === 0) {
				await loadProjectPipelinesForEditScope();
			}
			await loadPipelineSecrets();
		})();
	});

	$effect(() => {
		if (activeTab !== 'definition' || !pipeline || loading) return;
		void loadProjectWorkflowsAvailable();
	});

	$effect(() => {
		if (activeTab === 'access' && pipeline && !loading) {
			void loadPipelineMembers();
		}
	});

	$effect(() => {
		if (activeTab === 'runs' && pipeline && !loading) {
			void loadMatrix();
		}
	});

	function storedSecretScopeLabel(s: StoredSecret): string {
		const p = pipeline;
		const env =
			s.environment_id && pipelineProjectEnvs.length
				? pipelineProjectEnvs.find((e) => e.id === s.environment_id)
				: null;
		const envPart = env ? ` · ${env.display_name}` : s.environment_id ? ' · Environment' : '';
		if (!s.pipeline_id) return 'Project' + envPart;
		if (p && s.pipeline_id === p.id) return 'This pipeline' + envPart;
		const named = projectPipelinesAll.find((x) => x.id === s.pipeline_id);
		if (named) return named.name + envPart;
		return (s.pipeline_id?.slice(0, 8) ?? 'Pipeline') + envPart;
	}

	function openCreateSecret() {
		createPath = '';
		createKind = 'kv';
		createValue = '';
		createDescription = '';
		secScopePipelineId = pipeline?.id ?? '';
		createEnvironmentId = '';
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
				pipeline_id: secScopePipelineId || undefined,
				environment_id: createEnvironmentId || undefined
			});
			showCreateSecret = false;
			await loadPipelineSecrets();
		} catch (e) {
			secretsError = e instanceof Error ? e.message : 'Failed to create secret';
		} finally {
			secretActionLoading = false;
		}
	}

	function openEditSecretScope(s: StoredSecret) {
		editScopeTarget = s;
		editScopePipelineId = s.pipeline_id ?? '';
		editScopeEnvironmentId = s.environment_id ?? '';
		editScopeDescription = s.description ?? '';
		editScopePropagate = s.propagate_to_projects !== false;
		showEditSecretScopeDialog = true;
	}

	async function submitEditSecretScope() {
		if (!editScopeTarget) return;
		secretActionLoading = true;
		secretsError = null;
		try {
			const t = editScopeTarget;
			const body: {
				pipeline_id?: string | null;
				environment_id?: string | null;
				description?: string | null;
				propagate_to_projects?: boolean;
			} = {};

			if (!t.project_id || t.project_id === '') {
				const descNow = (t.description ?? '').trim();
				const descNew = editScopeDescription.trim();
				if (descNew !== descNow) body.description = descNew || null;
				const propNow = t.propagate_to_projects !== false;
				if (editScopePropagate !== propNow) body.propagate_to_projects = editScopePropagate;
			} else {
				const newPip = editScopePipelineId.trim() || null;
				const oldPip = t.pipeline_id ?? null;
				if (newPip !== oldPip) body.pipeline_id = newPip;
				const newEnv = editScopeEnvironmentId.trim() || null;
				const oldEnv = t.environment_id ?? null;
				if (newEnv !== oldEnv) body.environment_id = newEnv;
				const descNow = (t.description ?? '').trim();
				const descNew = editScopeDescription.trim();
				if (descNew !== descNow) body.description = descNew || null;
			}

			if (Object.keys(body).length === 0) {
				showEditSecretScopeDialog = false;
				editScopeTarget = null;
				return;
			}

			await apiMethods.storedSecrets.patch(t.id, body);
			showEditSecretScopeDialog = false;
			editScopeTarget = null;
			await loadPipelineSecrets();
		} catch (e) {
			secretsError = e instanceof Error ? e.message : 'Failed to update secret';
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
				...(ctx.pipeline_id ? { pipeline_id: ctx.pipeline_id } : {}),
				...(ctx.environment_id ? { environment_id: ctx.environment_id } : {}),
				...(!ctx.project_id ? { organization_wide: true } : {})
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
			await loadTriggers();
			void loadWorkflowDiagnostics(pipeline.id);
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

	const rotateSecretDialogTitle = $derived(
		rotateTarget != null && isRemoteRefSecretKind(rotateTarget.kind)
			? 'Update provider reference'
			: 'Rotate secret'
	);

	const workflowDiagProblems = $derived(
		workflowDiagItems.filter((i) => i.blocking || i.status !== 'ok')
	);
	const workflowDiagAllSafe = $derived(
		!workflowDiagLoading &&
			workflowDiagError === null &&
			(workflowDiagItems.length === 0 ||
				workflowDiagItems.every((i) => !i.blocking && i.status === 'ok'))
	);
</script>

<svelte:head>
	<title>{pipeline?.name ?? 'Pipeline'} | Meticulous</title>
</svelte:head>

<div class="space-y-6">
	<div class="flex items-start gap-4">
		<Button
			variant="ghost"
			size="sm"
			href={pipeline ? `/projects/${pipeline.project_id}` : '/projects'}
		>
			<ArrowLeft class="h-4 w-4" />
		</Button>

		{#if loading}
			<div class="flex-1 space-y-2">
				<Skeleton class="h-7 w-48" />
				<Skeleton class="h-4 w-32" />
			</div>
		{:else if pipeline}
			<div class="flex-1">
				<div class="flex flex-wrap items-center gap-3">
					<h1 class="text-2xl font-bold text-[var(--text-primary)]">{pipeline.name}</h1>
					{#if pipeline.enabled}
						<Badge variant="success" size="sm">Active</Badge>
					{:else}
						<Badge variant="secondary" size="sm">Disabled</Badge>
					{/if}
					{#if workflowDiagLoading}
						<span
							class="text-xs text-[var(--text-tertiary)]"
							title="Checking linked workflows against the catalog"
							aria-busy="true">…</span
						>
					{:else if workflowDiagError !== null}
						<button
							type="button"
							class="inline-flex items-center gap-1 text-xs font-medium text-amber-700 underline hover:no-underline dark:text-amber-400"
							onclick={() => (showWorkflowDiagModal = true)}
						>
							<ShieldAlert class="h-3.5 w-3.5 shrink-0" />
							Workflow check unavailable
						</button>
					{:else if !workflowDiagAllSafe}
						<button
							type="button"
							class="inline-flex items-center gap-1 text-xs font-medium text-amber-700 underline hover:no-underline dark:text-amber-400"
							onclick={() => (showWorkflowDiagModal = true)}
						>
							<ShieldAlert class="h-3.5 w-3.5 shrink-0" />
							Workflow issues
						</button>
					{:else if pipeline.enabled}
						<span
							class="inline-flex items-center gap-0.5 text-xs font-medium text-success-600 dark:text-success-500"
							title="All linked workflows resolve and are allowed to run"
						>
							<Check class="h-3.5 w-3.5 shrink-0" />
							ok
						</span>
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
				<Button variant="primary" onclick={triggerPipeline} loading={triggerLoading}>
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
						onclick={() => setPipelineTab('triggers')}
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

		<Tabs items={tabs} value={isPlSettingsGroup ? 'settings' : activeTab} onchange={(v) => setPipelineTab(v)} />

		{#if isPlSettingsGroup}
			<div class="flex gap-1.5 mt-1 mb-4">
				{#each plSettingsSubTabs as sub}
					<button
						class="rounded-md px-3 py-1.5 text-sm font-medium transition-colors {activeTab === sub.id
							? 'bg-[var(--bg-tertiary)] text-[var(--text-primary)] shadow-sm'
							: 'text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-hover)]'}"
						onclick={() => setPipelineTab(sub.id)}
					>
						{sub.label}
					</button>
				{/each}
			</div>
		{/if}

		{#if activeTab === 'runs'}
			{#if matrixData && matrixData.environments.length > 1}
				<div class="flex items-center gap-2 mb-3">
					<button
						class="rounded-md px-3 py-1.5 text-xs font-medium transition-colors {runsViewMode === 'matrix'
							? 'bg-[var(--bg-tertiary)] text-[var(--text-primary)] shadow-sm'
							: 'text-[var(--text-secondary)] hover:text-[var(--text-primary)]'}"
						onclick={() => (runsViewMode = 'matrix')}
					>
						Matrix
					</button>
					<button
						class="rounded-md px-3 py-1.5 text-xs font-medium transition-colors {runsViewMode === 'list'
							? 'bg-[var(--bg-tertiary)] text-[var(--text-primary)] shadow-sm'
							: 'text-[var(--text-secondary)] hover:text-[var(--text-primary)]'}"
						onclick={() => (runsViewMode = 'list')}
					>
						List
					</button>
				</div>
			{/if}

			{#if runsViewMode === 'matrix' && matrixData}
				{#await import('$lib/components/pipeline/RunMatrix.svelte') then mod}
					<svelte:component this={mod.default} data={matrixData} onrefresh={loadMatrix} />
				{/await}
			{:else}
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
						<Button variant="primary" onclick={triggerPipeline} loading={triggerLoading}>
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
			{/if}
		{:else if activeTab === 'settings'}
			{#if pipeline}
			<Card>
				<div class="space-y-4">
					<div>
						<h3 class="text-base font-medium text-[var(--text-primary)]">General</h3>
						<p class="mt-1 text-sm text-[var(--text-secondary)]">
							Pipeline name, description, and configuration.
						</p>
					</div>
					<div class="grid max-w-xl gap-4">
						<div>
							<label class="mb-1 block text-sm font-medium text-[var(--text-primary)]" for="ep-name-g">Name</label>
							<Input id="ep-name-g" bind:value={epName} placeholder="Pipeline display name" />
						</div>
						<div>
							<label class="mb-1 block text-sm font-medium text-[var(--text-primary)]" for="ep-slug-g">Slug</label>
							<Input id="ep-slug-g" value={pipeline.slug} readonly class="bg-[var(--bg-tertiary)]" />
							<p class="mt-1 text-xs text-[var(--text-tertiary)]">Slug is fixed after creation.</p>
						</div>
						<div>
							<label class="mb-1 block text-sm font-medium text-[var(--text-primary)]" for="ep-desc-g">Description</label>
							<textarea
								id="ep-desc-g"
								bind:value={epDescription}
								rows="3"
								class="w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm text-[var(--text-primary)] focus:outline-none focus:ring-2 focus:ring-primary-500"
								placeholder="Optional summary"
							></textarea>
						</div>
						<label class="flex items-center gap-2 text-sm">
							<input type="checkbox" bind:checked={epEnabled} class="rounded border-[var(--border-primary)]" />
							Enabled (allow new runs)
						</label>
						{#if pipeline.scm_provider}
							<div class="space-y-3 rounded-lg border border-[var(--border-primary)] bg-[var(--bg-tertiary)] p-3">
								<p class="text-xs font-medium text-[var(--text-secondary)]">Git source ({pipeline.scm_provider})</p>
								<div>
									<label class="mb-1 block text-xs font-medium" for="ep-scm-repo-g">Repository</label>
									<Input id="ep-scm-repo-g" bind:value={epScmRepository} placeholder="owner/repo" />
								</div>
								<div>
									<label class="mb-1 block text-xs font-medium" for="ep-scm-ref-g">Git ref</label>
									<Input id="ep-scm-ref-g" bind:value={epScmRef} placeholder="branch, tag, or SHA" />
								</div>
								<div>
									<label class="mb-1 block text-xs font-medium" for="ep-scm-path-g">Path to pipeline YAML</label>
									<Input id="ep-scm-path-g" bind:value={epScmPath} placeholder=".stable/pipeline.yaml" />
								</div>
								<div>
									<label class="mb-1 block text-xs font-medium" for="ep-scm-creds-g">Credentials secret path</label>
									<Input id="ep-scm-creds-g" bind:value={epScmCredsPath} placeholder="builtin_secrets path" />
								</div>
							</div>
						{/if}
						<div class="flex justify-end">
							<Button variant="primary" onclick={submitEditPipeline} loading={editPipelineLoading} disabled={!epName.trim()}>
								Save changes
							</Button>
						</div>
					</div>
				</div>
			</Card>
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
								<th class="px-4 py-3 text-left font-medium text-[var(--text-secondary)]">Inbound</th>
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
										{#if t.created_at}
											<p class="mt-1 text-xs text-[var(--text-tertiary)]">
												{#if t.created_by_username}
													{t.created_by_username}
												{:else if triggerIsRepoManaged(t)}
													Git
												{:else}
													—
												{/if}
												· {formatRelativeTime(t.created_at)}
											</p>
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
										{#if t.kind === 'webhook'}
											{@const inbound = triggerResolvedInboundAuth(t)}
											<div class="flex flex-col gap-1">
												{#if inbound === 'hmac'}
													<Badge variant="success" size="sm">HMAC</Badge>
													<span class="text-xs text-[var(--text-tertiary)]">
														<code class="font-mono">X-Hub-Signature-256</code>
													</span>
												{:else if inbound === 'query'}
													<Badge variant="success" size="sm">Query</Badge>
													<span class="text-xs text-[var(--text-tertiary)]">
														<code class="font-mono"
															>?{triggerQueryParamName(t)}=&lt;secret&gt;</code
														>
													</span>
												{:else}
													<Badge variant="secondary" size="sm">Open</Badge>
													<span class="text-xs text-[var(--text-tertiary)]">No verification</span>
												{/if}
											</div>
										{:else}
											<span class="text-[var(--text-tertiary)]">—</span>
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
		{:else if activeTab === 'access'}
			{#await import('$components/ui/access-control-panel.svelte') then mod}
				<svelte:component
					this={mod.default}
					members={plMembers}
					loading={plMembersLoading}
					error={plMembersError}
					showInherited={true}
					onSaveAccess={savePipelineAccess}
				/>
			{/await}
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
				<p class="max-w-3xl text-sm text-[var(--text-secondary)]">
					Project-wide secrets plus pipeline-specific overrides for <strong>{pipeline.name}</strong>. Provider-backed
					kinds store only the resource reference (visible in this list). Reference in YAML with
					<code class="rounded bg-[var(--bg-tertiary)] px-1 font-mono text-xs"
						>stored: &#123; name: MY_TOKEN &#125;</code
					>.
				</p>
				<div class="flex flex-wrap items-center gap-2">
					{#if pipelineProjectEnvs.length > 0}
						<div class="min-w-[11rem]">
							<Select
								id="pl-secrets-env-filter"
								options={plSecretsEnvFilterOptions}
								bind:value={secretsFilterEnvId}
							/>
						</div>
					{/if}
					<Button variant="outline" size="sm" href="/projects/{pipeline.project_id}?tab=secrets">
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
								<th class="px-4 py-3 text-left font-medium text-[var(--text-secondary)]">Reference</th>
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
									<td class="max-w-[14rem] truncate px-4 py-3 font-mono text-xs text-[var(--text-secondary)]">
										{#if isRemoteRefSecretKind(s.kind)}
											{getSecretRefFromMetadata(s.metadata) ?? '—'}
										{:else}
											—
										{/if}
									</td>
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
												title="Change pipeline, environment, or description"
												onclick={() => openEditSecretScope(s)}
											>
												<Edit class="h-4 w-4" />
											</Button>
											<Button
												variant="ghost"
												size="sm"
												disabled={!kindAllowedByExternalPolicy(s.kind, storedExternalKindPolicy)}
												title={!kindAllowedByExternalPolicy(s.kind, storedExternalKindPolicy)
													? 'This kind is disabled by platform administrators'
													: undefined}
												onclick={() => {
													rotateTarget = s;
													rotateValue = isRemoteRefSecretKind(s.kind)
														? (getSecretRefFromMetadata(s.metadata) ?? '')
														: '';
													showRotateSecretDialog = true;
												}}
											>
												{isRemoteRefSecretKind(s.kind) ? 'Edit ref' : 'Rotate'}
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
		{:else if activeTab === 'advanced'}
			<Card>
				<div class="space-y-6">
					<div>
						<h3 class="text-base font-medium text-[var(--text-primary)]">Danger Zone</h3>
						<p class="mt-1 text-sm text-[var(--text-secondary)]">
							Irreversible actions that affect this pipeline.
						</p>
					</div>
					<div class="rounded-lg border border-amber-200 p-4 dark:border-amber-900/60">
						<div class="flex items-center justify-between gap-4">
							<div>
								<p class="font-medium text-amber-900 dark:text-amber-200">Archive pipeline</p>
								<p class="mt-1 text-sm text-[var(--text-secondary)]">
									Hides this pipeline from the project and prevents new runs. An org admin can restore it from
									<a href="/admin/archive" class="text-primary-600 hover:underline dark:text-primary-400">Admin → Archive</a>.
								</p>
							</div>
							<Button
								variant="outline"
								size="sm"
								class="shrink-0 border-amber-300 text-amber-900 hover:bg-amber-50 dark:border-amber-700 dark:text-amber-100 dark:hover:bg-amber-950/40"
								onclick={async () => {
									if (!pipeline) return;
									try {
										await apiMethods.pipelines.archive(pipeline.id);
										window.location.href = `/projects/${pipeline.project_id}`;
									} catch (e) {
										error = e instanceof Error ? e.message : 'Failed to archive';
									}
								}}
							>
								<Archive class="h-4 w-4" />
								Archive
							</Button>
						</div>
					</div>
				</div>
			</Card>
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
											{@const catalogWorkflowId = catalogWorkflowIdForPipelineSourceRow(
												row,
												projectWorkflowsAvailable
											)}
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
													{#if row.kind !== 'pipeline' && catalogWorkflowId}
														<a
															href="/workflows/{catalogWorkflowId}"
															class="inline-flex h-8 items-center gap-1.5 rounded-lg border border-secondary-300 px-3 text-sm font-medium text-secondary-700 hover:bg-secondary-100 dark:border-secondary-600 dark:text-secondary-300 dark:hover:bg-secondary-800"
														>
															View workflow
														</a>
													{:else if upstream}
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
															title={sourceFilesRowDisabledTitle(row)}
														>
															{row.kind !== 'pipeline' ? 'View workflow' : 'View upstream'}
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
							<YamlCodeBlock source={pipelineDefinitionYaml} ariaLabel="Pipeline definition YAML" />
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

<Dialog bind:open={showCreateSecret} title="Add secret">
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
					>{storedSecretValueFieldLabel(createKind)}</label
				>
				<textarea
					id="pl-sec-val"
					bind:value={createValue}
					rows="4"
					class="w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm text-[var(--text-primary)] focus:outline-none focus:ring-2 focus:ring-primary-500"
					placeholder={storedSecretValuePlaceholder(createKind)}
				></textarea>
				<p class="mt-1 text-xs text-[var(--text-tertiary)]">{storedSecretValueHelpLine(createKind)}</p>
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
		<div>
			<label class="mb-1 block text-sm font-medium text-[var(--text-primary)]" for="pl-sec-env"
				>Environment (optional)</label
			>
			<Select id="pl-sec-env" options={plSecretEnvironmentOptions} bind:value={createEnvironmentId} />
			<p class="mt-1 text-xs text-[var(--text-tertiary)]">
				When set, this secret applies only to runs targeting that environment.
			</p>
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
	title={rotateSecretDialogTitle}
	onclose={() => {
		rotateTarget = null;
		rotateValue = '';
	}}
>
	{#if rotateTarget}
		<p class="text-sm text-[var(--text-secondary)]">
			{#if isRemoteRefSecretKind(rotateTarget.kind)}
				Update the provider reference for{' '}
				<span class="font-mono text-[var(--text-primary)]">{rotateTarget.path}</span> (creates a new version).
			{:else}
				New value for <span class="font-mono text-[var(--text-primary)]">{rotateTarget.path}</span> (creates a new
				version).
			{/if}
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
			{#if rotateTarget.kind !== 'github_app'}
				<label class="mb-1 block text-xs font-medium text-[var(--text-secondary)]" for="pl-rotate-sec-val"
					>{storedSecretValueFieldLabel(rotateTarget.kind)}</label
				>
			{/if}
			<textarea
				id="pl-rotate-sec-val"
				bind:value={rotateValue}
				rows={rotateTarget.kind === 'github_app' ? 14 : 4}
				placeholder={storedSecretValuePlaceholder(rotateTarget.kind)}
				class="w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 font-mono text-sm text-[var(--text-primary)] focus:outline-none focus:ring-2 focus:ring-primary-500"
			></textarea>
			{#if rotateTarget.kind !== 'github_app'}
				<p class="mt-1 text-xs text-[var(--text-tertiary)]">
					{storedSecretValueHelpLine(rotateTarget.kind)}
				</p>
			{/if}
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
				{rotateTarget && isRemoteRefSecretKind(rotateTarget.kind) ? 'Save reference' : 'Rotate'}
			</Button>
		</div>
	{/if}
</Dialog>

<Dialog
	bind:open={showEditSecretScopeDialog}
	title="Edit secret scope"
	onclose={() => {
		editScopeTarget = null;
	}}
>
	{#if editScopeTarget}
		<p class="text-sm text-[var(--text-secondary)]">
			Updates apply to the whole secret chain (all versions) for
			<span class="font-mono text-[var(--text-primary)]">{editScopeTarget.path}</span>.
		</p>
		<div class="mt-4 space-y-4">
			{#if !editScopeTarget.project_id || editScopeTarget.project_id === ''}
				<label class="flex cursor-pointer items-start gap-3 rounded-lg border border-[var(--border-primary)] bg-[var(--bg-tertiary)] p-3">
					<input
						type="checkbox"
						class="mt-1 h-4 w-4 rounded border-[var(--border-primary)]"
						bind:checked={editScopePropagate}
					/>
					<span>
						<span class="text-sm font-medium text-[var(--text-primary)]"
							>Expose to all projects and pipelines</span>
						<span class="mt-0.5 block text-xs text-[var(--text-secondary)]">
							When off, the secret is for platform features only, not project secret lists or
							<code class="font-mono">stored:</code> resolution.
						</span>
					</span>
				</label>
			{:else}
				<div>
					<label class="mb-1 block text-sm font-medium text-[var(--text-primary)]" for="pl-edit-sec-pipe"
						>Pipeline scope</label
					>
					<Select
						id="pl-edit-sec-pipe"
						options={pipelineSecretScopeEditOptions}
						bind:value={editScopePipelineId}
					/>
				</div>
				<div>
					<label class="mb-1 block text-sm font-medium text-[var(--text-primary)]" for="pl-edit-sec-env"
						>Environment</label
					>
					<Select id="pl-edit-sec-env" options={plSecretEnvironmentOptions} bind:value={editScopeEnvironmentId} />
					<p class="mt-1 text-xs text-[var(--text-tertiary)]">
						Choose “All environments (default)” to clear environment scoping.
					</p>
				</div>
			{/if}
			<div>
				<label class="mb-1 block text-sm font-medium text-[var(--text-primary)]" for="pl-edit-sec-desc"
					>Description</label
				>
				<Input id="pl-edit-sec-desc" bind:value={editScopeDescription} placeholder="Optional" />
			</div>
		</div>
		<div class="mt-6 flex justify-end gap-2">
			<Button
				variant="outline"
				onclick={() => {
					showEditSecretScopeDialog = false;
					editScopeTarget = null;
				}}
			>
				Cancel
			</Button>
			<Button variant="primary" onclick={submitEditSecretScope} loading={secretActionLoading}>
				Save
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
		<div class="space-y-3 rounded-lg border border-[var(--border-primary)] bg-[var(--bg-tertiary)] p-3">
			<div>
				<label class="mb-1 block text-sm font-medium text-[var(--text-primary)]" for="nt-auth-mode"
					>Inbound authentication</label
				>
				<Select
					id="nt-auth-mode"
					options={triggerInboundAuthOptions.map((o) => ({ value: o.value, label: o.label }))}
					bind:value={ntAuthMode}
				/>
			</div>
			{#if ntAuthMode === 'query'}
				<div>
					<label class="mb-1 block text-sm font-medium text-[var(--text-primary)]" for="nt-qparam"
						>Query parameter name</label
					>
					<Input id="nt-qparam" bind:value={ntQueryParamName} placeholder="token" />
					<p class="mt-1 text-xs text-[var(--text-tertiary)]">
						Callers append
						<code class="font-mono"
							>?{ntQueryParamName.trim() || 'token'}=&lt;secret&gt;</code
						>
						using the webhook secret.
					</p>
				</div>
			{/if}
			{#if ntAuthMode === 'none'}
				<p class="text-xs text-amber-700 dark:text-amber-400">
					Anyone who can reach this URL can trigger runs. Prefer HMAC or a query secret when the caller supports it.
				</p>
			{:else}
				<label class="flex cursor-pointer items-center gap-2 text-sm text-[var(--text-secondary)]">
					<input type="checkbox" bind:checked={ntGenerateSecret} class="rounded border-[var(--border-primary)]" />
					Generate secret (shown once after save)
				</label>
				{#if !ntGenerateSecret}
					<div>
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
		? 'Repo-managed: you can change status, description, and rotate the webhook secret. Inbound auth and filters live in YAML.'
		: 'Update trigger settings, inbound authentication, or rotate the webhook secret.'}
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
			{#if editTriggerTarget.kind === 'webhook' && !triggerIsRepoManaged(editTriggerTarget)}
				<div class="space-y-3 rounded-lg border border-[var(--border-primary)] bg-[var(--bg-tertiary)] p-3">
					<div>
						<label class="mb-1 block text-sm font-medium text-[var(--text-primary)]" for="et-auth-mode"
							>Inbound authentication</label
						>
						<Select
							id="et-auth-mode"
							options={triggerInboundAuthOptions.map((o) => ({ value: o.value, label: o.label }))}
							bind:value={etAuthMode}
						/>
					</div>
					{#if etAuthMode === 'query'}
						<div>
							<label class="mb-1 block text-sm font-medium text-[var(--text-primary)]" for="et-qparam"
								>Query parameter name</label
							>
							<Input id="et-qparam" bind:value={etQueryParam} placeholder="token" />
							<p class="mt-1 text-xs text-[var(--text-tertiary)]">
								Callers append
								<code class="font-mono">?{etQueryParam.trim() || 'token'}=&lt;secret&gt;</code>.
							</p>
						</div>
					{/if}
					{#if etAuthMode === 'none'}
						<p class="text-xs text-[var(--text-tertiary)]">
							Saving clears the stored secret; POSTs will not be verified unless you configure auth again later.
						</p>
					{/if}
				</div>
			{:else if editTriggerTarget.kind === 'webhook'}
				<p class="text-xs text-[var(--text-tertiary)]">
					Git-managed trigger: inbound authentication and filters come from pipeline YAML. You can rotate the secret
					below; to change verification mode, edit the repo.
				</p>
			{/if}
			{#if editTriggerTarget.kind === 'webhook' && (triggerIsRepoManaged(editTriggerTarget) || etAuthMode !== 'none')}
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
						{#if triggerIsRepoManaged(editTriggerTarget)}
							Leave blank to keep the current secret from the last sync.
						{:else}
							Leave blank to keep the current secret. Required when using HMAC or query auth if there is no secret
							yet.
						{/if}
					</p>
				</div>
			{/if}
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
	description="Triggers can verify inbound calls with an HMAC header, a secret in the query string, or no verification (open)."
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
			For <strong>HMAC</strong> mode, with a stored secret, add header
			<code class="rounded bg-secondary-100 px-1 font-mono text-[11px] dark:bg-secondary-800"
				>X-Hub-Signature-256: sha256=&lt;hex&gt;</code
			>
			where
			<code class="font-mono text-xs">&lt;hex&gt;</code>
			is the <strong>lowercase</strong> hex digest of
			<code class="font-mono text-xs">HMAC-SHA256(secret, raw_body_bytes)</code>. In <strong>query</strong> mode, append
			<code class="rounded bg-secondary-100 px-1 font-mono text-[11px] dark:bg-secondary-800"
				>?&lt;param&gt;=&lt;secret&gt;</code
			>
			(see the trigger’s query parameter name in the UI). With <strong>open</strong> inbound auth, omit both.
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

<Dialog
	bind:open={showWorkflowDiagModal}
	title="Linked workflows"
	description="Runs can be blocked until catalog approval, trust, and version resolution are satisfied."
	class="max-w-lg"
	onclose={() => {
		showWorkflowDiagModal = false;
	}}
>
	{#if workflowDiagError !== null}
		<p class="text-sm text-[var(--text-secondary)]">{workflowDiagError}</p>
	{:else if workflowDiagProblems.length === 0}
		<p class="text-sm text-[var(--text-secondary)]">No issues reported for linked workflows.</p>
	{:else}
		<ul class="max-h-[min(60vh,24rem)] space-y-3 overflow-y-auto text-sm">
			{#each workflowDiagProblems as item}
				<li class="rounded-md border border-[var(--border-primary)] bg-[var(--bg-secondary)] p-3">
					<p class="font-medium text-[var(--text-primary)]">
						<span class="font-mono text-xs">{item.invocation_id || '—'}</span>
						{#if item.reference}
							<span class="text-[var(--text-secondary)]"> · </span>
							<span class="font-mono text-xs">{item.reference}</span>
						{/if}
					</p>
					<p class="mt-1 text-[var(--text-secondary)]">
						<span class="font-semibold">{item.status}</span>
						{#if item.blocking}
							<span class="text-amber-700 dark:text-amber-400"> (blocks runs)</span>
						{/if}
					</p>
					{#if item.detail}
						<p class="mt-1 text-[var(--text-tertiary)]">{item.detail}</p>
					{/if}
				</li>
			{/each}
		</ul>
	{/if}
	<div class="mt-4 flex justify-end border-t border-[var(--border-primary)] pt-4">
		<Button variant="outline" onclick={() => (showWorkflowDiagModal = false)}>Close</Button>
	</div>
</Dialog>
