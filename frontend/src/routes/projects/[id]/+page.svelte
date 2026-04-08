<script lang="ts">
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { getPublicApiBase } from '$lib/public-api-base';
	import {
		Button,
		Card,
		Badge,
		Tabs,
		Dialog,
		Input,
		Alert,
		Select,
		CopyButton
	} from '$components/ui';
	import { DataTable, EmptyState, Skeleton } from '$components/data';
	import { apiMethods } from '$api/client';
	import type {
		Project,
		Pipeline,
		ProjectVariable,
		StoredSecret,
		CatalogWorkflow,
		PatchProjectWebhookInput,
		ProjectWebhookRegistration,
		WebhookRegistrationTargetRow
	} from '$api/types';
	import type {
		MeticulousAppCatalogEntry,
		ProjectMeticulousInstallationRow
	} from '$lib/api/client';
	import { formatRelativeTime } from '$utils/format';
	import {
		ArrowLeft,
		Plus,
		GitBranch,
		Play,
		Settings,
		Trash2,
		Edit,
		KeyRound,
		RefreshCw,
		Braces,
		History,
		Layers,
		ExternalLink,
		Webhook,
		Puzzle,
		Archive
	} from 'lucide-svelte';
	import type { Column } from '$components/data/DataTable.svelte';

	let project = $state<Project | null>(null);
	let pipelines = $state<Pipeline[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let activeTab = $state('pipelines');

	let secrets = $state<StoredSecret[]>([]);
	let secretsLoading = $state(false);
	let secretsError = $state<string | null>(null);
	let showCreateSecret = $state(false);
	let createPath = $state('');
	let createKind = $state('kv');
	let createValue = $state('');
	let createDescription = $state('');
	let createPipelineId = $state('');
	let ghAppId = $state('');
	let ghInstallationId = $state('');
	let ghPrivateKey = $state('');
	let ghApiBase = $state('');
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

	let variables = $state<ProjectVariable[]>([]);
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
	let ghExtraJson = $state('');

	let wfGlobal = $state<CatalogWorkflow[]>([]);
	let wfProject = $state<CatalogWorkflow[]>([]);
	let wfLoading = $state(false);
	let wfError = $state<string | null>(null);

	let createOrgWideSecret = $state(false);
	/** When creating an org-wide secret: if true, appears in project/pipeline secret lists and `stored:` resolution. */
	let orgWidePropagateToProjects = $state(true);

	let settingsName = $state('');
	let settingsSlug = $state('');
	let settingsDescription = $state('');
	let settingsSaving = $state(false);
	let settingsError = $state<string | null>(null);

	let showArchiveProjectDialog = $state(false);
	let archiveProjectLoading = $state(false);
	let archiveProjectError = $state<string | null>(null);

	let projectWebhooks = $state<ProjectWebhookRegistration[]>([]);
	let pwLoading = $state(false);
	let pwError = $state<string | null>(null);
	let pwTargetsRegistrationId = $state<string | null>(null);
	let pwTargets = $state<WebhookRegistrationTargetRow[]>([]);
	let pwTargetsLoading = $state(false);
	let showCreatePw = $state(false);
	let pwCreatePipelineIds = $state<string[]>([]);
	let pwCreateLoading = $state(false);
	let pwLastSigningSecret = $state<string | null>(null);
	let pwAddPipelineId = $state('');
	let pwAuthMode = $state('hmac');
	let pwQueryParamName = $state('token');
	let pwDescription = $state('');

	let showEditPw = $state(false);
	let editPwTarget = $state<ProjectWebhookRegistration | null>(null);
	let epwDescription = $state('');
	let epwAuthMode = $state('hmac');
	let epwQueryParam = $state('token');
	let pwEditLoading = $state(false);
	let pwRotatingId = $state<string | null>(null);
	let pwDeletingRegistrationId = $state<string | null>(null);
	let showClearPwInboundDialog = $state(false);
	let clearPwTarget = $state<ProjectWebhookRegistration | null>(null);
	let clearPwLoading = $state(false);

	let appCatalog = $state<MeticulousAppCatalogEntry[]>([]);
	let appInstallations = $state<ProjectMeticulousInstallationRow[]>([]);
	let appsLoading = $state(false);
	let appsError = $state<string | null>(null);
	let installApplicationId = $state('');
	let permRead = $state(true);
	let permJoinCreate = $state(false);
	let permJoinRevoke = $state(false);
	let permAgentsDelete = $state(false);
	let installAppLoading = $state(false);

	const kindOptions = [
		{ value: 'kv', label: 'Key / value (kv)' },
		{ value: 'api_key', label: 'API key' },
		{ value: 'ssh_private_key', label: 'SSH private key (PEM)' },
		{ value: 'github_app', label: 'GitHub App' },
		{ value: 'x509_bundle', label: 'X.509 bundle (JSON)' }
	];

	const tabs = [
		{ id: 'pipelines', label: 'Pipelines', icon: GitBranch },
		{ id: 'triggers', label: 'Triggers', icon: Webhook },
		{ id: 'workflows', label: 'Workflows', icon: Layers },
		{ id: 'runs', label: 'Recent Runs', icon: Play },
		{ id: 'variables', label: 'Variables', icon: Braces },
		{ id: 'secrets', label: 'Secrets', icon: KeyRound },
		{ id: 'apps', label: 'Meticulous Apps', icon: Puzzle },
		{ id: 'settings', label: 'Settings', icon: Settings }
	];

	$effect(() => {
		loadProject();
	});

	$effect(() => {
		if (activeTab !== 'settings' || !project || loading) return;
		settingsName = project.name;
		settingsSlug = project.slug;
		settingsDescription = project.description ?? '';
		settingsError = null;
	});

	$effect(() => {
		if (activeTab !== 'apps' || !project?.id || loading) return;
		loadMeticulousAppsTab();
	});

	async function loadMeticulousAppsTab() {
		if (!project?.id) return;
		appsLoading = true;
		appsError = null;
		try {
			const [catalog, installations] = await Promise.all([
				apiMethods.projects.availableMeticulousApps(project.id),
				apiMethods.projects.listMeticulousInstallations(project.id)
			]);
			appCatalog = catalog;
			appInstallations = installations;
			if (!installApplicationId && catalog.length > 0) {
				installApplicationId = catalog[0].application_id;
			}
		} catch (e) {
			appsError = e instanceof Error ? e.message : 'Failed to load Meticulous Apps';
		} finally {
			appsLoading = false;
		}
	}

	async function installMeticulousAppOnProject() {
		if (!project?.id || !installApplicationId.trim()) return;
		const permissions: string[] = [];
		if (permRead) permissions.push('read');
		if (permJoinCreate) permissions.push('join_tokens:create');
		if (permJoinRevoke) permissions.push('join_tokens:revoke');
		if (permAgentsDelete) permissions.push('agents:delete');
		installAppLoading = true;
		appsError = null;
		try {
			await apiMethods.projects.installMeticulousApp(project.id, {
				application_id: installApplicationId.trim(),
				permissions: permissions.length > 0 ? permissions : ['read']
			});
			await loadMeticulousAppsTab();
		} catch (e) {
			appsError = e instanceof Error ? e.message : 'Install failed';
		} finally {
			installAppLoading = false;
		}
	}

	async function loadProject() {
		loading = true;
		error = null;
		try {
			const projectId = $page.params.id!;
			project = await apiMethods.projects.get(projectId);
			const pipelinesResponse = await apiMethods.pipelines.list({ project_id: projectId });
			pipelines = pipelinesResponse.data;
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load project';
		} finally {
			loading = false;
		}
	}

	const pipelineColumns: Column<Pipeline>[] = [
		{
			key: 'name',
			label: 'Pipeline',
			sortable: true,
			render: (_, row) => `
				<div>
					<div class="font-medium text-[var(--text-primary)]">${row.name}</div>
					<div class="text-sm text-[var(--text-secondary)]">${row.slug}</div>
				</div>
			`
		},
		{
			key: 'enabled',
			label: 'Status',
			render: (value) =>
				value
					? '<span class="inline-flex items-center gap-1.5 text-sm text-success-600 dark:text-success-400"><span class="h-2 w-2 rounded-full bg-success-500"></span>Active</span>'
					: '<span class="inline-flex items-center gap-1.5 text-sm text-secondary-500"><span class="h-2 w-2 rounded-full bg-secondary-400"></span>Disabled</span>'
		},
		{
			key: 'updated_at',
			label: 'Last Updated',
			sortable: true,
			render: (value) => formatRelativeTime(value as string)
		}
	];

	function handlePipelineClick(pipeline: Pipeline) {
		goto(`/pipelines/${pipeline.id}`);
	}

	function pipelineLabel(id: string | null | undefined): string {
		if (!id) return '—';
		const p = pipelines.find((x) => x.id === id);
		return p ? p.name : id.slice(0, 8);
	}

	function apiPublicOrigin(): string {
		return getPublicApiBase();
	}

	function projectWebhookFullUrl(inboundPath: string): string {
		return `${apiPublicOrigin()}${inboundPath}`;
	}

	async function loadProjectWebhooks() {
		if (!project) return;
		pwLoading = true;
		pwError = null;
		try {
			projectWebhooks = await apiMethods.projectWebhooks.list(project.id);
		} catch (e) {
			pwError = e instanceof Error ? e.message : 'Failed to load webhooks';
			projectWebhooks = [];
		} finally {
			pwLoading = false;
		}
	}

	$effect(() => {
		if (activeTab !== 'triggers' || !project?.id || loading) return;
		void loadProjectWebhooks();
	});

	async function loadPwTargets(registrationId: string) {
		if (!project) return;
		pwTargetsLoading = true;
		try {
			pwTargets = await apiMethods.projectWebhooks.listTargets(project.id, registrationId);
		} catch {
			pwTargets = [];
		} finally {
			pwTargetsLoading = false;
		}
	}

	$effect(() => {
		const rid = pwTargetsRegistrationId;
		if (!rid || !project || activeTab !== 'triggers') return;
		void loadPwTargets(rid);
	});

	function togglePwCreatePipeline(id: string) {
		if (pwCreatePipelineIds.includes(id)) {
			pwCreatePipelineIds = pwCreatePipelineIds.filter((x) => x !== id);
		} else {
			pwCreatePipelineIds = [...pwCreatePipelineIds, id];
		}
	}

	function openCreateProjectWebhook() {
		pwLastSigningSecret = null;
		pwAuthMode = 'hmac';
		pwQueryParamName = 'token';
		pwDescription = '';
		pwCreatePipelineIds = pipelines.length > 0 ? [pipelines[0].id] : [];
		showCreatePw = true;
	}

	function openEditProjectWebhook(wh: ProjectWebhookRegistration) {
		if (wh.provider !== 'generic') return;
		editPwTarget = wh;
		epwDescription = wh.description?.trim() ?? '';
		epwAuthMode = (wh.generic_inbound_auth ?? 'hmac').toLowerCase();
		if (epwAuthMode !== 'none' && epwAuthMode !== 'hmac' && epwAuthMode !== 'query') {
			epwAuthMode = 'hmac';
		}
		epwQueryParam = (wh.generic_query_param_name?.trim() || 'token') as string;
		pwError = null;
		showEditPw = true;
	}

	async function submitEditProjectWebhook() {
		if (!project || !editPwTarget || editPwTarget.provider !== 'generic') return;
		if (epwAuthMode === 'query' && !epwQueryParam.trim()) {
			pwError = 'Query parameter name is required for query authentication';
			return;
		}
		pwEditLoading = true;
		pwError = null;
		try {
			const body: PatchProjectWebhookInput = {
				description: epwDescription.trim(),
				generic_inbound_auth: epwAuthMode as 'none' | 'hmac' | 'query'
			};
			if (epwAuthMode === 'query') {
				body.generic_query_param_name = epwQueryParam.trim() || 'token';
			}
			const res = await apiMethods.projectWebhooks.patch(project.id, editPwTarget.id, body);
			if (res.signing_secret) {
				pwLastSigningSecret = res.signing_secret;
			}
			showEditPw = false;
			editPwTarget = null;
			await loadProjectWebhooks();
		} catch (e) {
			pwError = e instanceof Error ? e.message : 'Failed to update webhook';
		} finally {
			pwEditLoading = false;
		}
	}

	async function rotateProjectWebhookSecret(wh: ProjectWebhookRegistration) {
		if (!project) return;
		pwRotatingId = wh.id;
		pwError = null;
		try {
			const r = await apiMethods.projectWebhooks.rotateInboundSecret(project.id, wh.id);
			pwLastSigningSecret = r.signing_secret;
			await loadProjectWebhooks();
		} catch (e) {
			pwError = e instanceof Error ? e.message : 'Failed to rotate secret';
		} finally {
			pwRotatingId = null;
		}
	}

	async function submitClearPwInbound() {
		if (!project || !clearPwTarget) return;
		clearPwLoading = true;
		pwError = null;
		try {
			await apiMethods.projectWebhooks.clearInboundSecret(project.id, clearPwTarget.id);
			showClearPwInboundDialog = false;
			clearPwTarget = null;
			await loadProjectWebhooks();
		} catch (e) {
			pwError = e instanceof Error ? e.message : 'Failed to clear verification';
		} finally {
			clearPwLoading = false;
		}
	}

	async function submitCreateProjectWebhook() {
		if (!project) return;
		if (pwAuthMode === 'query' && !pwQueryParamName.trim()) {
			pwError = 'Query parameter name is required for query authentication';
			return;
		}
		pwCreateLoading = true;
		pwError = null;
		try {
			const targets = pwCreatePipelineIds.map((pipeline_id) => ({
				pipeline_id,
				filter_config: {}
			}));
			const desc = pwDescription.trim();
			const res = await apiMethods.projectWebhooks.setup(project.id, {
				provider: 'generic',
				events: [],
				targets,
				payload_mapping: { flatten_top_level: true },
				generic_inbound_auth: pwAuthMode as 'none' | 'hmac' | 'query',
				...(pwAuthMode === 'query'
					? { generic_query_param_name: pwQueryParamName.trim() }
					: {}),
				...(desc ? { description: desc } : {})
			});
			pwLastSigningSecret = res.signing_secret ?? null;
			showCreatePw = false;
			await loadProjectWebhooks();
		} catch (e) {
			pwError = e instanceof Error ? e.message : 'Failed to create webhook';
		} finally {
			pwCreateLoading = false;
		}
	}

	async function deleteProjectWebhookRegistration(registrationId: string) {
		if (!project) return;
		if (
			!confirm(
				'Delete this webhook registration and all pipeline targets? SCM registrations may be restricted by the server.'
			)
		)
			return;
		pwDeletingRegistrationId = registrationId;
		pwError = null;
		try {
			await apiMethods.projectWebhooks.deleteRegistration(project.id, registrationId);
			if (pwTargetsRegistrationId === registrationId) {
				pwTargetsRegistrationId = null;
			}
			await loadProjectWebhooks();
		} catch (e) {
			pwError = e instanceof Error ? e.message : 'Failed to delete registration';
		} finally {
			pwDeletingRegistrationId = null;
		}
	}

	async function removePwTarget(registrationId: string, targetId: string) {
		if (!project) return;
		try {
			await apiMethods.projectWebhooks.deleteTarget(project.id, registrationId, targetId);
			await loadPwTargets(registrationId);
			await loadProjectWebhooks();
		} catch (e) {
			pwError = e instanceof Error ? e.message : 'Failed to remove target';
		}
	}

	async function addPwTarget(registrationId: string) {
		if (!project || !pwAddPipelineId.trim()) return;
		try {
			await apiMethods.projectWebhooks.addTarget(project.id, registrationId, {
				pipeline_id: pwAddPipelineId.trim(),
				enabled: true,
				filter_config: {}
			});
			pwAddPipelineId = '';
			await loadPwTargets(registrationId);
			await loadProjectWebhooks();
		} catch (e) {
			pwError = e instanceof Error ? e.message : 'Failed to add target';
		}
	}

	async function loadWorkflowsAvailable() {
		if (!project) return;
		wfLoading = true;
		wfError = null;
		try {
			const res = await apiMethods.wfCatalog.listAvailableForProject(project.id);
			wfGlobal = res.global_workflows;
			wfProject = res.project_workflows;
		} catch (e) {
			wfError = e instanceof Error ? e.message : 'Failed to load workflows';
			wfGlobal = [];
			wfProject = [];
		} finally {
			wfLoading = false;
		}
	}

	$effect(() => {
		const pid = project?.id;
		if (activeTab !== 'workflows' || !pid || loading) return;
		void loadWorkflowsAvailable();
	});

	async function saveProjectSettings() {
		if (!project) return;
		settingsSaving = true;
		settingsError = null;
		try {
			const updated = await apiMethods.projects.update(project.id, {
				name: settingsName.trim(),
				slug: settingsSlug.trim(),
				description: settingsDescription.trim() || null
			});
			project = updated;
		} catch (e) {
			settingsError = e instanceof Error ? e.message : 'Failed to save project';
		} finally {
			settingsSaving = false;
		}
	}

	async function confirmArchiveProject() {
		if (!project) return;
		archiveProjectLoading = true;
		archiveProjectError = null;
		try {
			await apiMethods.projects.archive(project.id);
			showArchiveProjectDialog = false;
			goto('/projects');
		} catch (e) {
			archiveProjectError = e instanceof Error ? e.message : 'Failed to archive project';
		} finally {
			archiveProjectLoading = false;
		}
	}

	async function loadSecrets() {
		if (!project) return;
		secretsLoading = true;
		secretsError = null;
		try {
			secrets = await apiMethods.storedSecrets.list(project.id);
		} catch (e) {
			secretsError = e instanceof Error ? e.message : 'Failed to load secrets';
			secrets = [];
		} finally {
			secretsLoading = false;
		}
	}

	$effect(() => {
		const pid = project?.id;
		if (activeTab !== 'secrets' || !pid || loading) return;
		void loadSecrets();
	});

	async function loadVariables() {
		if (!project) return;
		variablesLoading = true;
		variablesError = null;
		try {
			const res = await apiMethods.variables.list(project.id);
			variables = res.data;
		} catch (e) {
			variablesError = e instanceof Error ? e.message : 'Failed to load variables';
			variables = [];
		} finally {
			variablesLoading = false;
		}
	}

	$effect(() => {
		const pid = project?.id;
		if (activeTab !== 'variables' || !pid || loading) return;
		void loadVariables();
	});

	$effect(() => {
		if (createOrgWideSecret) createPipelineId = '';
	});

	function openCreateSecret() {
		createPath = '';
		createKind = 'kv';
		createValue = '';
		createDescription = '';
		createPipelineId = '';
		createOrgWideSecret = false;
		orgWidePropagateToProjects = true;
		ghAppId = '';
		ghInstallationId = '';
		ghPrivateKey = '';
		ghApiBase = '';
		ghExtraJson = '';
		showCreateSecret = true;
	}

	async function submitCreateSecret() {
		if (!project) return;
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
						if (
							typeof parsed !== 'object' ||
							parsed === null ||
							Array.isArray(parsed)
						) {
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

			await apiMethods.storedSecrets.create(project.id, {
				path: createPath.trim(),
				kind: createKind,
				value,
				description: createDescription.trim() || undefined,
				pipeline_id: createOrgWideSecret ? undefined : createPipelineId || undefined,
				...(createOrgWideSecret
					? { scope: 'organization', propagate_to_projects: orgWidePropagateToProjects }
					: {})
			});
			showCreateSecret = false;
			await loadSecrets();
		} catch (e) {
			secretsError = e instanceof Error ? e.message : 'Failed to create secret';
		} finally {
			secretActionLoading = false;
		}
	}

	function createSecretValid(): boolean {
		if (!createPath.trim()) return false;
		if (createKind === 'github_app') {
			return !!(ghAppId.trim() && ghInstallationId.trim() && ghPrivateKey.trim());
		}
		return !!createValue.trim();
	}

	async function submitRotateSecret() {
		if (!rotateTarget) return;
		secretActionLoading = true;
		secretsError = null;
		try {
			await apiMethods.storedSecrets.rotate(rotateTarget.id, rotateValue);
			showRotateSecretDialog = false;
			rotateTarget = null;
			rotateValue = '';
			await loadSecrets();
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
			await loadSecrets();
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
		const proj = project;
		if (!ctx || !proj) return;
		versionsLoading = true;
		versionsError = null;
		try {
			secretVersionRows = await apiMethods.storedSecrets.listVersions(proj.id, {
				path: ctx.path,
				...(ctx.pipeline_id ? { pipeline_id: ctx.pipeline_id } : {}),
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
			await loadSecrets();
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
			await loadSecrets();
			await refreshSecretVersions();
		} catch (e) {
			versionsError = e instanceof Error ? e.message : 'Failed to purge version';
		} finally {
			secretActionLoading = false;
		}
	}

	const pipelineScopeOptions = $derived([
		{ value: '', label: 'Project-wide (all pipelines)' },
		...pipelines.map((p) => ({ value: p.id, label: p.name }))
	]);

	function openCreateVariable() {
		cvName = '';
		cvValue = '';
		cvSensitive = false;
		cvPipelineId = '';
		showCreateVariable = true;
	}

	async function submitCreateVariable() {
		if (!project) return;
		variableActionLoading = true;
		variablesError = null;
		try {
			await apiMethods.variables.create(project.id, {
				name: cvName.trim(),
				value: cvValue,
				is_sensitive: cvSensitive,
				pipeline_id: cvPipelineId || undefined
			});
			showCreateVariable = false;
			await loadVariables();
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
			await loadVariables();
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
			await loadVariables();
		} catch (e) {
			variablesError = e instanceof Error ? e.message : 'Failed to delete variable';
		} finally {
			variableActionLoading = false;
		}
	}

	function variableScopeLabel(v: ProjectVariable): string {
		if (!v.pipeline_id) return 'Project';
		return pipelineLabel(v.pipeline_id);
	}

	function storedSecretScopeLabel(s: StoredSecret): string {
		if (s.project_id == null || s.project_id === '') return 'Organization';
		if (s.pipeline_id) return pipelineLabel(s.pipeline_id);
		return 'Project';
	}

	const settingsDirty = $derived(
		!!project &&
			(settingsName.trim() !== project.name ||
				settingsSlug.trim() !== project.slug ||
				settingsDescription.trim() !== (project.description ?? '').trim())
	);

	const settingsSaveDisabled = $derived(
		!settingsDirty || !settingsName.trim() || !settingsSlug.trim() || settingsSaving
	);
</script>

<svelte:head>
	<title>{project?.name ?? 'Project'} | Meticulous</title>
</svelte:head>

<div class="space-y-6">
	<div class="flex items-center gap-4">
		<Button variant="ghost" size="sm" href="/projects">
			<ArrowLeft class="h-4 w-4" />
		</Button>

		{#if loading}
			<div class="space-y-2">
				<Skeleton class="h-7 w-48" />
				<Skeleton class="h-4 w-32" />
			</div>
		{:else if project}
			<div class="flex-1">
				<div class="flex items-center gap-3">
					<h1 class="text-2xl font-bold text-[var(--text-primary)]">{project.name}</h1>
				</div>
				{#if project.description}
					<p class="mt-1 text-[var(--text-secondary)]">{project.description}</p>
				{/if}
			</div>

			<div class="flex items-center gap-2">
				<Button variant="outline" size="sm" onclick={() => (activeTab = 'settings')}>
					<Edit class="h-4 w-4" />
					Edit
				</Button>
				<Button variant="primary" href="/pipelines/new?project={project.id}">
					<Plus class="h-4 w-4" />
					New Pipeline
				</Button>
			</div>
		{/if}
	</div>

	{#if error}
		<Alert variant="error" title="Error">
			{error}
		</Alert>
	{/if}

	{#if !loading && project}
		<Tabs items={tabs} bind:value={activeTab} />

		{#if activeTab === 'pipelines'}
			{#if pipelines.length === 0}
				<Card>
					<EmptyState
						title="No pipelines yet"
						description="Create your first pipeline to start automating your builds."
					>
						<Button variant="primary" href="/pipelines/new?project={project.id}">
							<Plus class="h-4 w-4" />
							Create Pipeline
						</Button>
					</EmptyState>
				</Card>
			{:else}
				<DataTable
					columns={pipelineColumns}
					data={pipelines}
					rowKey="id"
					onRowClick={handlePipelineClick}
				/>
			{/if}
		{:else if activeTab === 'triggers'}
			<div class="flex flex-wrap items-center justify-between gap-3">
				<p class="text-sm text-[var(--text-secondary)]">
					<strong>Project webhooks</strong> receive one HTTP POST and can start multiple pipelines in this project.
					<code class="rounded bg-[var(--bg-tertiary)] px-1 font-mono text-xs">generic</code> URLs use
					<code class="rounded bg-[var(--bg-tertiary)] px-1 font-mono text-xs">/api/v1/webhooks/&lt;org&gt;/&lt;id&gt;</code>
					with GitHub-style 					<code class="rounded bg-[var(--bg-tertiary)] px-1 font-mono text-xs">X-Hub-Signature-256</code>,
					a shared secret in a query parameter, or no verification (open URL — use only on trusted networks).
					Per-target filters match SCM webhooks.
				</p>
				<div class="flex gap-2">
					<Button variant="outline" size="sm" onclick={loadProjectWebhooks} loading={pwLoading}>
						<RefreshCw class="h-4 w-4" />
						Refresh
					</Button>
					<Button
						variant="primary"
						size="sm"
						onclick={openCreateProjectWebhook}
						disabled={pipelines.length === 0}
					>
						<Plus class="h-4 w-4" />
						New project webhook
					</Button>
				</div>
			</div>
			{#if pwLastSigningSecret}
				<Alert
					variant="info"
					title="Signing secret"
					dismissible
					ondismiss={() => (pwLastSigningSecret = null)}
				>
					<p class="mb-2 text-sm">
						Copy this value now; it is not shown again. Use it for
						<code class="font-mono text-xs">X-Hub-Signature-256</code> (HMAC mode) or as the query parameter value
						(query mode). Shown after create, after enabling auth from an open URL, or after rotating the secret.
					</p>
					<div class="flex flex-wrap items-center gap-2">
						<code class="max-w-full break-all rounded bg-[var(--bg-tertiary)] px-2 py-1 text-xs">{pwLastSigningSecret}</code>
						<CopyButton text={pwLastSigningSecret} size="sm" />
					</div>
				</Alert>
			{/if}
			{#if pwError}
				<Alert variant="error" title="Webhooks" dismissible ondismiss={() => (pwError = null)}>
					{pwError}
				</Alert>
			{/if}
			{#if pipelines.length === 0}
				<Card>
					<EmptyState
						title="Add a pipeline first"
						description="Project webhooks route to one or more pipelines in this project."
					>
						<Button variant="primary" href="/pipelines/new?project={project.id}">
							<Plus class="h-4 w-4" />
							Create Pipeline
						</Button>
					</EmptyState>
				</Card>
			{:else if pwLoading && projectWebhooks.length === 0}
				<Card>
					<div class="space-y-3 p-4">
						{#each Array(3) as _, i (i)}
							<Skeleton class="h-12 w-full" />
						{/each}
					</div>
				</Card>
			{:else if projectWebhooks.length === 0}
				<Card>
					<EmptyState
						title="No project webhooks yet"
						description="Create a generic webhook to map JSON POST bodies to variables and fan out runs to selected pipelines."
					>
						<Button variant="primary" onclick={openCreateProjectWebhook}>
							<Plus class="h-4 w-4" />
							New project webhook
						</Button>
					</EmptyState>
				</Card>
			{:else}
				<div class="space-y-4">
					{#each projectWebhooks as wh (wh.id)}
						<Card>
							<div class="space-y-3 p-4">
								<div class="flex flex-wrap items-start justify-between gap-3">
									<div>
										<div class="flex flex-wrap items-center gap-2">
											<h3 class="text-lg font-medium text-[var(--text-primary)]">
												{wh.provider === 'generic' ? 'Generic (multi-pipeline)' : wh.provider}
											</h3>
											{#if !wh.active}
												<Badge variant="secondary">Inactive</Badge>
											{/if}
										</div>
										<p class="mt-1 font-mono text-xs text-[var(--text-secondary)] break-all">
											{projectWebhookFullUrl(wh.inbound_path)}
										</p>
										{#if wh.description?.trim()}
											<p class="mt-1 text-sm text-[var(--text-secondary)]">{wh.description}</p>
										{/if}
										<p class="mt-1 text-xs text-[var(--text-tertiary)]">
											Created {formatRelativeTime(wh.created_at)}
											{#if wh.created_by_username}
												· {wh.created_by_username}
											{/if}
										</p>
										{#if wh.generic_inbound_auth}
											<p class="mt-1 text-xs text-[var(--text-tertiary)]">
												Auth:
												<span class="font-mono text-[var(--text-secondary)]">{wh.generic_inbound_auth}</span>
												{#if wh.generic_inbound_auth === 'query' && wh.generic_query_param_name}
													· append
													<span class="font-mono"
														>?{wh.generic_query_param_name}=&lt;secret&gt;</span
													>
												{/if}
												{#if wh.generic_inbound_auth === 'none'}
													· no verification (any client can POST)
												{/if}
											</p>
										{/if}
										{#if wh.events.length > 0}
											<p class="mt-1 text-xs text-[var(--text-tertiary)]">
												Registration events: {wh.events.join(', ')}
											</p>
										{/if}
									</div>
									<div class="flex flex-wrap gap-2">
										<CopyButton text={projectWebhookFullUrl(wh.inbound_path)} size="sm" />
										{#if wh.provider === 'generic'}
											<Button variant="outline" size="sm" onclick={() => openEditProjectWebhook(wh)}>
												<Edit class="h-4 w-4" aria-hidden="true" />
												Edit
											</Button>
										{/if}
										{#if wh.inbound_secret_configured && (wh.provider !== 'generic' || wh.generic_inbound_auth !== 'none')}
											<Button
												variant="outline"
												size="sm"
												title="Generate a new signing secret (updates HMAC key / query value)"
												disabled={pwRotatingId === wh.id}
												onclick={() => rotateProjectWebhookSecret(wh)}
											>
												<KeyRound class="h-4 w-4" aria-hidden="true" />
												{pwRotatingId === wh.id ? 'Rotating…' : 'Rotate secret'}
											</Button>
										{/if}
										{#if wh.provider === 'generic' && wh.generic_inbound_auth && wh.generic_inbound_auth !== 'none'}
											<Button
												variant="outline"
												size="sm"
												class="border-amber-600/50 text-amber-800 hover:bg-amber-50 dark:text-amber-300 dark:hover:bg-amber-950/40"
												onclick={() => {
													clearPwTarget = wh;
													showClearPwInboundDialog = true;
												}}
											>
												Open URL
											</Button>
										{/if}
										<Button
											variant={pwTargetsRegistrationId === wh.id ? 'primary' : 'outline'}
											size="sm"
											onclick={() => {
												pwTargetsRegistrationId = pwTargetsRegistrationId === wh.id ? null : wh.id;
												pwAddPipelineId = '';
											}}
										>
											{pwTargetsRegistrationId === wh.id ? 'Hide targets' : 'Pipelines'}
										</Button>
										<Button
											variant="ghost"
											size="sm"
											class="text-error-600 dark:text-error-400"
											disabled={pwDeletingRegistrationId === wh.id}
											title="Delete entire registration"
											onclick={(e) => {
												e.stopPropagation();
												void deleteProjectWebhookRegistration(wh.id);
											}}
										>
											<Trash2 class="h-4 w-4" />
											Remove
										</Button>
									</div>
								</div>
								{#if pwTargetsRegistrationId === wh.id}
									<div
										class="rounded-lg border border-[var(--border-secondary)] bg-[var(--bg-tertiary)]/40 p-3"
									>
										<p class="mb-2 text-sm font-medium text-[var(--text-primary)]">
											Pipelines that receive a run
										</p>
										{#if pwTargetsLoading}
											<Skeleton class="h-8 w-full" />
										{:else if pwTargets.length === 0}
											<p class="text-sm text-[var(--text-secondary)]">No targets yet. Add a pipeline below.</p>
										{:else}
											<ul class="divide-y divide-[var(--border-secondary)] text-sm">
												{#each pwTargets as t (t.id)}
													<li
														class="flex flex-wrap items-center justify-between gap-2 py-2"
													>
														<span class="font-medium text-[var(--text-primary)]"
															>{pipelineLabel(t.pipeline_id)}</span
														>
														<Button
															variant="ghost"
															size="sm"
															onclick={() => removePwTarget(wh.id, t.id)}
														>
															<Trash2 class="h-4 w-4" />
														</Button>
													</li>
												{/each}
											</ul>
										{/if}
										<div class="mt-3 flex flex-wrap items-end gap-2">
											<div class="min-w-[200px] flex-1">
												<label class="mb-1 block text-xs text-[var(--text-secondary)]"
													>Add pipeline</label
												>
												<Select
													bind:value={pwAddPipelineId}
													options={pipelines.map((p) => ({
														value: p.id,
														label: p.name
													}))}
													placeholder="Select pipeline…"
												/>
											</div>
											<Button
												variant="outline"
												size="sm"
												onclick={() => addPwTarget(wh.id)}
												disabled={!pwAddPipelineId.trim()}
											>
												Add
											</Button>
										</div>
									</div>
								{/if}
							</div>
						</Card>
					{/each}
				</div>
			{/if}
		{:else if activeTab === 'variables'}
			<div class="flex flex-wrap items-center justify-between gap-3">
				<p class="text-sm text-[var(--text-secondary)]">
					Non-secret configuration merged into runs: <strong>project</strong> variables apply to all pipelines;
					<strong>pipeline</strong> rows override for that pipeline. Pipeline YAML <code
						class="rounded bg-[var(--bg-tertiary)] px-1 font-mono text-xs">variables:</code
					>
					and trigger payloads override these for the same name.
				</p>
				<div class="flex gap-2">
					<Button variant="outline" size="sm" onclick={loadVariables} loading={variablesLoading}>
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
			{#if variablesLoading && variables.length === 0}
				<Card>
					<div class="space-y-3 p-4">
						{#each Array(4) as _, i (i)}
							<Skeleton class="h-10 w-full" />
						{/each}
					</div>
				</Card>
			{:else if variables.length === 0}
				<Card>
					<EmptyState title="No variables" description="Add project or pipeline-scoped values for use in pipelines.">
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
							{#each variables as v (v.id)}
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
		{:else if activeTab === 'workflows'}
			<div class="flex flex-wrap items-center justify-between gap-3">
				<p class="text-sm text-[var(--text-secondary)]">
					<strong>Organization</strong> entries are global catalog workflows you can invoke from any project you can
					access. <strong>This project</strong> lists workflows submitted or mapped to this project only.
				</p>
				<div class="flex flex-wrap gap-2">
					<Button variant="outline" size="sm" href="/workflows">
						<ExternalLink class="h-4 w-4" />
						Browse catalog
					</Button>
					<Button variant="outline" size="sm" onclick={loadWorkflowsAvailable} loading={wfLoading}>
						<RefreshCw class="h-4 w-4" />
						Refresh
					</Button>
				</div>
			</div>
			{#if wfError}
				<Alert variant="error" title="Workflows" dismissible ondismiss={() => (wfError = null)}>
					{wfError}
				</Alert>
			{/if}
			{#if wfLoading && wfGlobal.length === 0 && wfProject.length === 0}
				<Card>
					<div class="space-y-3 p-4">
						{#each Array(4) as _, i (i)}
							<Skeleton class="h-10 w-full" />
						{/each}
					</div>
				</Card>
			{:else}
				<div class="space-y-8">
					<Card>
						<div class="space-y-4 p-4">
							<div>
								<h3 class="text-lg font-medium text-[var(--text-primary)]">Organization catalog</h3>
								<p class="mt-1 text-sm text-[var(--text-secondary)]">
									Global reusable workflows for this organization.
								</p>
							</div>
							{#if wfGlobal.length === 0}
								<EmptyState
									title="No global workflows"
									description="Import or submit workflows to the organization catalog to see them here."
								/>
							{:else}
								<div class="overflow-hidden rounded-lg border border-[var(--border-primary)]">
									<table class="w-full text-sm">
										<thead class="bg-[var(--bg-tertiary)]">
											<tr>
												<th class="px-4 py-3 text-left font-medium text-[var(--text-secondary)]">Name</th>
												<th class="px-4 py-3 text-left font-medium text-[var(--text-secondary)]">Version</th>
												<th class="px-4 py-3 text-left font-medium text-[var(--text-secondary)]">Trust</th>
												<th class="px-4 py-3 text-right font-medium text-[var(--text-secondary)]">Actions</th>
											</tr>
										</thead>
										<tbody class="divide-y divide-[var(--border-secondary)]">
											{#each wfGlobal as w (w.id)}
												<tr class="bg-[var(--bg-secondary)]">
													<td class="px-4 py-3">
														<div class="font-medium text-[var(--text-primary)]">{w.name}</div>
														{#if w.description}
															<div class="mt-0.5 text-xs text-[var(--text-secondary)]">{w.description}</div>
														{/if}
														{#if w.deprecated}
															<Badge variant="warning" class="mt-1">Deprecated</Badge>
														{/if}
													</td>
													<td class="px-4 py-3 font-mono text-xs">{w.version}</td>
													<td class="px-4 py-3">
														<Badge variant="secondary">{w.trust_state}</Badge>
													</td>
													<td class="px-4 py-3 text-right">
														<Button variant="ghost" size="sm" href="/workflows/{w.id}">
															View
															<ExternalLink class="h-3.5 w-3.5 opacity-70" />
														</Button>
													</td>
												</tr>
											{/each}
										</tbody>
									</table>
								</div>
							{/if}
						</div>
					</Card>
					<Card>
						<div class="space-y-4 p-4">
							<div>
								<h3 class="text-lg font-medium text-[var(--text-primary)]">This project</h3>
								<p class="mt-1 text-sm text-[var(--text-secondary)]">
									Workflow versions scoped to this project (in addition to the global catalog).
								</p>
							</div>
							{#if wfProject.length === 0}
								<EmptyState
									title="No project workflows"
									description="Submit a workflow for this project or map an existing catalog entry to see rows here."
								/>
							{:else}
								<div class="overflow-hidden rounded-lg border border-[var(--border-primary)]">
									<table class="w-full text-sm">
										<thead class="bg-[var(--bg-tertiary)]">
											<tr>
												<th class="px-4 py-3 text-left font-medium text-[var(--text-secondary)]">Name</th>
												<th class="px-4 py-3 text-left font-medium text-[var(--text-secondary)]">Version</th>
												<th class="px-4 py-3 text-left font-medium text-[var(--text-secondary)]">Trust</th>
												<th class="px-4 py-3 text-right font-medium text-[var(--text-secondary)]">Actions</th>
											</tr>
										</thead>
										<tbody class="divide-y divide-[var(--border-secondary)]">
											{#each wfProject as w (w.id)}
												<tr class="bg-[var(--bg-secondary)]">
													<td class="px-4 py-3">
														<div class="font-medium text-[var(--text-primary)]">{w.name}</div>
														{#if w.description}
															<div class="mt-0.5 text-xs text-[var(--text-secondary)]">{w.description}</div>
														{/if}
														{#if w.deprecated}
															<Badge variant="warning" class="mt-1">Deprecated</Badge>
														{/if}
													</td>
													<td class="px-4 py-3 font-mono text-xs">{w.version}</td>
													<td class="px-4 py-3">
														<Badge variant="secondary">{w.trust_state}</Badge>
													</td>
													<td class="px-4 py-3 text-right">
														<Button variant="ghost" size="sm" href="/workflows/{w.id}">
															View
															<ExternalLink class="h-3.5 w-3.5 opacity-70" />
														</Button>
													</td>
												</tr>
											{/each}
										</tbody>
									</table>
								</div>
							{/if}
						</div>
					</Card>
				</div>
			{/if}
		{:else if activeTab === 'secrets'}
			<div class="flex flex-wrap items-center justify-between gap-3">
				<p class="text-sm text-[var(--text-secondary)]">
					Values are encrypted and never shown again after save. Use <strong>organization-wide</strong> secrets for
					values shared across projects (org admins only). Otherwise scope to this project or a single pipeline.
					Reference in pipeline YAML with{' '}
					<code class="rounded bg-[var(--bg-tertiary)] px-1 font-mono text-xs"
						>stored: &#123; name: MY_TOKEN &#125;</code
					>
					(use the same logical name you entered here).
				</p>
				<div class="flex gap-2">
					<Button variant="outline" size="sm" onclick={loadSecrets} loading={secretsLoading}>
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

			{#if secretsLoading && secrets.length === 0}
				<Card>
					<div class="space-y-3 p-4">
						{#each Array(4) as _, i (i)}
							<Skeleton class="h-10 w-full" />
						{/each}
					</div>
				</Card>
			{:else if secrets.length === 0}
				<Card>
					<EmptyState
						title="No stored secrets"
						description="Create a secret to inject into jobs via the pipeline secrets block."
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
							{#each secrets as s (s.id)}
								<tr class="bg-[var(--bg-secondary)]">
									<td class="px-4 py-3 font-mono text-sm">{s.path}</td>
									<td class="px-4 py-3">{s.kind}</td>
									<td class="px-4 py-3">
										{storedSecretScopeLabel(s)}
									</td>
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
		{:else if activeTab === 'runs'}
			<Card>
				<EmptyState
					title="No recent runs"
					description="Runs will appear here when you trigger a pipeline."
				/>
			</Card>
		{:else if activeTab === 'apps'}
			<div class="space-y-6">
				<Card>
					<div class="flex flex-wrap items-start justify-between gap-4">
						<div>
							<h3 class="text-lg font-medium text-[var(--text-primary)]">Meticulous Apps</h3>
							<p class="mt-1 text-sm text-[var(--text-secondary)]">
								Install machine identities on this project for integration APIs and read-only automation.
								Apps must be registered in your organization first (<a
									class="text-primary-600 underline"
									href="/admin/apps">Admin → Apps</a
								>).
							</p>
						</div>
						<Button variant="outline" size="sm" onclick={loadMeticulousAppsTab} loading={appsLoading}>
							<RefreshCw class="h-4 w-4" />
							Refresh
						</Button>
					</div>
					{#if appsError}
						<div class="mt-4">
							<Alert variant="error" title="Apps" dismissible ondismiss={() => (appsError = null)}>
								{appsError}
							</Alert>
						</div>
					{/if}
				</Card>

				<Card>
					<h4 class="text-sm font-medium text-[var(--text-primary)]">Installations</h4>
					{#if appsLoading && appInstallations.length === 0}
						<div class="mt-4"><Skeleton class="h-24 w-full" /></div>
					{:else if appInstallations.length === 0}
						<p class="mt-3 text-sm text-[var(--text-secondary)]">No apps installed on this project yet.</p>
					{:else}
						<div class="mt-4 overflow-x-auto">
							<table class="w-full text-left text-sm">
								<thead>
									<tr class="border-b border-[var(--border-primary)] text-[var(--text-secondary)]">
										<th class="py-2 pr-4 font-medium">Application</th>
										<th class="py-2 pr-4 font-medium">Installation</th>
										<th class="py-2 pr-4 font-medium">Permissions</th>
										<th class="py-2 font-medium">Status</th>
									</tr>
								</thead>
								<tbody>
									{#each appInstallations as row}
										<tr class="border-b border-[var(--border-secondary)]">
											<td class="py-2 pr-4">
												<div class="font-medium text-[var(--text-primary)]">{row.app_name}</div>
												<div class="font-mono text-xs text-[var(--text-secondary)]">
													{row.application_id}
												</div>
											</td>
											<td class="py-2 pr-4 font-mono text-xs">{row.installation_id}</td>
											<td class="py-2 pr-4 text-xs text-[var(--text-secondary)]">
												{row.permissions.join(', ') || '—'}
											</td>
											<td class="py-2">
												{#if row.revoked_at}
													<Badge variant="secondary">Revoked</Badge>
												{:else}
													<Badge variant="success">Active</Badge>
												{/if}
											</td>
										</tr>
									{/each}
								</tbody>
							</table>
						</div>
					{/if}
				</Card>

				<Card>
					<h4 class="text-sm font-medium text-[var(--text-primary)]">Install app</h4>
					<p class="mt-1 text-sm text-[var(--text-secondary)]">
						Requires project administrator access. Permissions are enforced on integration and read API routes.
					</p>
					{#if appCatalog.length === 0 && !appsLoading}
						<p class="mt-4 text-sm text-[var(--text-secondary)]">
							No enabled apps are available from your organization. Ask an org admin to register one, or create an
							app in Admin → Apps.
						</p>
					{:else}
						<div class="mt-4 grid max-w-xl gap-4">
							<div>
								<label class="mb-1 block text-sm font-medium text-[var(--text-primary)]" for="app-pick"
									>Application</label
								>
								<Select
									id="app-pick"
									options={appCatalog.map((a) => ({
										value: a.application_id,
										label: `${a.name} (${a.application_id})`
									}))}
									bind:value={installApplicationId}
								/>
							</div>
							<div class="space-y-2 rounded-lg border border-[var(--border-primary)] bg-[var(--bg-tertiary)] p-3">
								<p class="text-xs font-medium text-[var(--text-primary)]">Permissions</p>
								<label class="flex cursor-pointer items-start gap-2 text-sm">
									<input type="checkbox" class="mt-0.5" bind:checked={permRead} />
									<span>
										<code class="rounded bg-[var(--bg-secondary)] px-1 font-mono text-xs">read</code>
										— project read APIs (pipelines, runs, variables, secrets metadata, etc.)
									</span>
								</label>
								<label class="flex cursor-pointer items-start gap-2 text-sm">
									<input type="checkbox" class="mt-0.5" bind:checked={permJoinCreate} />
									<span>
										<code class="rounded bg-[var(--bg-secondary)] px-1 font-mono text-xs"
											>join_tokens:create</code
										>
										— integration API to mint join tokens
									</span>
								</label>
								<label class="flex cursor-pointer items-start gap-2 text-sm">
									<input type="checkbox" class="mt-0.5" bind:checked={permJoinRevoke} />
									<span>
										<code class="rounded bg-[var(--bg-secondary)] px-1 font-mono text-xs"
											>join_tokens:revoke</code
										>
									</span>
								</label>
								<label class="flex cursor-pointer items-start gap-2 text-sm">
									<input type="checkbox" class="mt-0.5" bind:checked={permAgentsDelete} />
									<span>
										<code class="rounded bg-[var(--bg-secondary)] px-1 font-mono text-xs"
											>agents:delete</code
										>
										— integration API for agent cleanup
									</span>
								</label>
							</div>
							<div>
								<Button
									variant="primary"
									onclick={installMeticulousAppOnProject}
									loading={installAppLoading}
									disabled={!installApplicationId.trim() || appCatalog.length === 0}
								>
									<Plus class="h-4 w-4" />
									Install
								</Button>
							</div>
						</div>
					{/if}
				</Card>
			</div>
		{:else if activeTab === 'settings'}
			<Card>
				<div class="space-y-6">
					<div>
						<h3 class="text-lg font-medium text-[var(--text-primary)]">Project Settings</h3>
						<p class="mt-1 text-sm text-[var(--text-secondary)]">
							Update the display name, URL slug, and description. The slug is used in URLs and API paths.
						</p>
					</div>

					{#if settingsError}
						<Alert variant="error" title="Settings" dismissible ondismiss={() => (settingsError = null)}>
							{settingsError}
						</Alert>
					{/if}

					<div class="grid max-w-xl gap-4">
						<div>
							<label class="mb-1 block text-sm font-medium text-[var(--text-primary)]" for="proj-name"
								>Name</label
							>
							<Input id="proj-name" bind:value={settingsName} placeholder="Project name" />
						</div>
						<div>
							<label class="mb-1 block text-sm font-medium text-[var(--text-primary)]" for="proj-slug"
								>Slug</label
							>
							<Input
								id="proj-slug"
								bind:value={settingsSlug}
								class="font-mono text-sm"
								placeholder="my-project"
							/>
						</div>
						<div>
							<label class="mb-1 block text-sm font-medium text-[var(--text-primary)]" for="proj-desc"
								>Description</label
							>
							<textarea
								id="proj-desc"
								bind:value={settingsDescription}
								rows="3"
								class="w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm text-[var(--text-primary)] focus:outline-none focus:ring-2 focus:ring-primary-500"
								placeholder="Optional"
							></textarea>
						</div>
						<div class="flex justify-end">
							<Button
								variant="primary"
								onclick={saveProjectSettings}
								loading={settingsSaving}
								disabled={settingsSaveDisabled}
							>
								Save changes
							</Button>
						</div>
					</div>

					<div class="border-t border-[var(--border-primary)] pt-6">
						<h4 class="text-sm font-medium text-[var(--text-primary)]">Danger Zone</h4>
						<div class="mt-4 rounded-lg border border-amber-200 p-4 dark:border-amber-900/60">
							<div class="flex items-center justify-between gap-4">
								<div>
									<p class="font-medium text-amber-900 dark:text-amber-200">Archive project</p>
									<p class="mt-1 text-sm text-[var(--text-secondary)]">
										Hides this project from the main project list, archives all pipelines in it, and disables
										normal use. Only an organization admin can unarchive or permanently delete it from
										<a href="/admin/archive" class="text-primary-600 hover:underline dark:text-primary-400"
											>Admin → Archive</a
										>.
									</p>
								</div>
								<Button
									variant="outline"
									size="sm"
									class="shrink-0 border-amber-300 text-amber-900 hover:bg-amber-50 dark:border-amber-700 dark:text-amber-100 dark:hover:bg-amber-950/40"
									onclick={() => {
										archiveProjectError = null;
										showArchiveProjectDialog = true;
									}}
								>
									<Archive class="h-4 w-4" />
									Archive
								</Button>
							</div>
						</div>
					</div>
				</div>
			</Card>
		{/if}
	{/if}
</div>

<Dialog bind:open={showArchiveProjectDialog} title="Archive this project?">
	<div class="space-y-4">
		<p class="text-sm text-[var(--text-secondary)]">
			Archive <span class="font-medium text-[var(--text-primary)]">{project?.name ?? 'this project'}</span>?
			All pipelines in this project will be archived with it. Permanent removal is only available to organization
			admins under Admin → Archive.
		</p>
		{#if archiveProjectError}
			<Alert variant="error">{archiveProjectError}</Alert>
		{/if}
		<div class="flex justify-end gap-2">
			<Button
				variant="outline"
				onclick={() => {
					showArchiveProjectDialog = false;
					archiveProjectError = null;
				}}
				disabled={archiveProjectLoading}
			>
				Cancel
			</Button>
			<Button variant="primary" onclick={confirmArchiveProject} loading={archiveProjectLoading}>
				Archive project
			</Button>
		</div>
	</div>
</Dialog>

<Dialog bind:open={showCreateSecret} title="Add stored secret">
	<div class="space-y-4">
		<div>
			<label class="mb-1 block text-sm font-medium text-[var(--text-primary)]" for="sec-path"
				>Logical name</label
			>
			<Input id="sec-path" bind:value={createPath} placeholder="e.g. MY_API_TOKEN" />
		</div>
		<div>
			<label class="mb-1 block text-sm font-medium text-[var(--text-primary)]" for="sec-kind">Kind</label>
			<Select id="sec-kind" options={kindOptions} bind:value={createKind} />
		</div>
		{#if createKind === 'github_app'}
			<div class="rounded-lg border border-[var(--border-primary)] bg-[var(--bg-tertiary)] p-3 space-y-3">
				<p class="text-xs text-[var(--text-secondary)]">
					Create a GitHub App, install it on your org or repo, then paste credentials here. Values are encrypted; the
					private key never leaves the control plane except to mint short-lived tokens for jobs.
				</p>
				<div class="grid gap-3 sm:grid-cols-2">
					<div>
						<label class="mb-1 block text-xs font-medium" for="gh-app-id">App ID</label>
						<Input id="gh-app-id" bind:value={ghAppId} placeholder="123456" />
					</div>
					<div>
						<label class="mb-1 block text-xs font-medium" for="gh-install">Installation ID</label>
						<Input id="gh-install" bind:value={ghInstallationId} placeholder="78901234" />
					</div>
				</div>
				<div>
					<label class="mb-1 block text-xs font-medium" for="gh-pem">Private key (PEM)</label>
					<textarea
						id="gh-pem"
						bind:value={ghPrivateKey}
						rows="6"
						class="w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 font-mono text-xs text-[var(--text-primary)] focus:outline-none focus:ring-2 focus:ring-primary-500"
						placeholder="-----BEGIN RSA PRIVATE KEY----- ..."
					></textarea>
				</div>
				<div>
					<label class="mb-1 block text-xs font-medium" for="gh-api-base">GitHub API base (optional)</label>
					<Input
						id="gh-api-base"
						bind:value={ghApiBase}
						placeholder="https://api.github.com (default)"
					/>
				</div>
				<div>
					<label class="mb-1 block text-xs font-medium" for="gh-extra">Additional fields (optional JSON object)</label>
					<textarea
						id="gh-extra"
						bind:value={ghExtraJson}
						rows="3"
						class="w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 font-mono text-xs text-[var(--text-primary)] focus:outline-none focus:ring-2 focus:ring-primary-500"
						placeholder={`{\n  "client_id": "...",\n  "webhook_secret": "..."\n}`}
					></textarea>
				</div>
			</div>
		{:else}
			<div>
				<label class="mb-1 block text-sm font-medium text-[var(--text-primary)]" for="sec-val"
					>Value (one-time)</label
				>
				<textarea
					id="sec-val"
					bind:value={createValue}
					rows="4"
					class="w-full rounded-lg border border-[var(--border-primary)] bg-[var(--bg-secondary)] px-3 py-2 text-sm text-[var(--text-primary)] focus:outline-none focus:ring-2 focus:ring-primary-500"
					placeholder="Secret value or PEM / JSON payload"
				></textarea>
			</div>
		{/if}
		<div>
			<label class="mb-1 block text-sm font-medium text-[var(--text-primary)]" for="sec-desc"
				>Description (optional)</label
			>
			<Input id="sec-desc" bind:value={createDescription} />
		</div>
		<div class="rounded-lg border border-[var(--border-primary)] bg-[var(--bg-tertiary)] p-3">
			<label class="flex cursor-pointer items-start gap-3">
				<input
					type="checkbox"
					class="mt-1 h-4 w-4 rounded border-[var(--border-primary)]"
					bind:checked={createOrgWideSecret}
				/>
				<span>
					<span class="text-sm font-medium text-[var(--text-primary)]">Organization-wide secret</span>
					<span class="mt-0.5 block text-xs text-[var(--text-secondary)]">
						Available to every project in the organization. Creating or rotating these requires an organization
						admin. Pipeline scope does not apply.
					</span>
				</span>
			</label>
			{#if createOrgWideSecret}
				<label class="mt-3 flex cursor-pointer items-start gap-3 border-t border-[var(--border-secondary)] pt-3">
					<input
						type="checkbox"
						class="mt-1 h-4 w-4 rounded border-[var(--border-primary)]"
						bind:checked={orgWidePropagateToProjects}
					/>
					<span>
						<span class="text-sm font-medium text-[var(--text-primary)]"
							>Expose to all projects and pipelines</span>
						<span class="mt-0.5 block text-xs text-[var(--text-secondary)]">
							When off, the secret stays organization-wide but is only used for platform features that opt in
							(such as importing the global workflow catalog from source code), not for pipeline <code
								class="font-mono">stored:</code>
							resolution or per-project secret lists.
						</span>
					</span>
				</label>
			{/if}
		</div>
		<div>
			<label class="mb-1 block text-sm font-medium text-[var(--text-primary)]" for="sec-scope">Scope</label>
			<Select
				id="sec-scope"
				options={pipelineScopeOptions}
				bind:value={createPipelineId}
				disabled={createOrgWideSecret}
			/>
			{#if createOrgWideSecret}
				<p class="mt-1 text-xs text-[var(--text-secondary)]">Organization secrets are not limited to one pipeline.</p>
			{/if}
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
			·
			{versionsContext.pipeline_id ? pipelineLabel(versionsContext.pipeline_id) : 'Project-wide'}
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

<Dialog bind:open={showCreateVariable} title="Add environment variable">
	<div class="space-y-4">
		<div>
			<label class="mb-1 block text-sm font-medium" for="v-name">Name</label>
			<Input id="v-name" bind:value={cvName} placeholder="e.g. NODE_VERSION" />
		</div>
		<div>
			<label class="mb-1 block text-sm font-medium" for="v-val">Value</label>
			<Input id="v-val" bind:value={cvValue} />
		</div>
		<label class="flex items-center gap-2 text-sm">
			<input type="checkbox" bind:checked={cvSensitive} class="rounded border-[var(--border-primary)]" />
			Mask value in API responses (sensitive)
		</label>
		<div>
			<label class="mb-1 block text-sm font-medium" for="v-scope">Scope</label>
			<Select id="v-scope" options={pipelineScopeOptions} bind:value={cvPipelineId} />
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
				<label class="mb-1 block text-sm font-medium" for="ev-name">Name</label>
				<Input id="ev-name" bind:value={evName} />
			</div>
			<div>
				<label class="mb-1 block text-sm font-medium" for="ev-val">New value</label>
				<Input
					id="ev-val"
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
			Delete <span class="font-mono">{deleteVariableTarget.name}</span>? Running pipelines keep already-loaded values
			until the next run.
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

<Dialog
	bind:open={showCreatePw}
	title="New project webhook"
	onclose={() => {
		pwCreatePipelineIds = [];
		pwDescription = '';
	}}
>
	<p class="text-sm text-[var(--text-secondary)]">
		Choose one or more pipelines to run for each matching request. The inbound URL and (when applicable) signing secret
		are shown after creation.
	</p>
	<div class="mt-4 space-y-3">
		<div>
			<label class="mb-1 block text-sm font-medium text-[var(--text-primary)]" for="pw-desc"
				>Description (optional)</label
			>
			<Input id="pw-desc" bind:value={pwDescription} placeholder="e.g. ACME deploy hook" />
		</div>
		<div>
			<label class="mb-1 block text-sm font-medium text-[var(--text-primary)]" for="pw-auth-mode"
				>Inbound authentication</label
			>
			<Select
				id="pw-auth-mode"
				options={[
					{ value: 'hmac', label: 'HMAC header (X-Hub-Signature-256)' },
					{ value: 'query', label: 'Query parameter (secret in URL)' },
					{ value: 'none', label: 'None (open — no verification)' }
				]}
				bind:value={pwAuthMode}
			/>
		</div>
		{#if pwAuthMode === 'query'}
			<div>
				<label class="mb-1 block text-sm font-medium text-[var(--text-primary)]" for="pw-qparam"
					>Query parameter name</label
				>
				<Input id="pw-qparam" bind:value={pwQueryParamName} placeholder="token" />
				<p class="mt-1 text-xs text-[var(--text-tertiary)]">
					Callers append <code class="font-mono">?{pwQueryParamName.trim() || 'token'}=&lt;secret&gt;</code> using the
					signing secret shown once after create.
				</p>
			</div>
		{/if}
		{#if pwAuthMode === 'none'}
			<p class="text-xs text-amber-700 dark:text-amber-400">
				Anyone who can reach this URL can trigger pipelines. Prefer HMAC or query secret when the caller supports it.
			</p>
		{/if}
	</div>
	<p class="mt-4 text-sm font-medium text-[var(--text-primary)]">Pipelines to run</p>
	<div class="mt-2 max-h-60 space-y-2 overflow-y-auto">
		{#each pipelines as p (p.id)}
			<label class="flex cursor-pointer items-center gap-2 text-sm text-[var(--text-primary)]">
				<input
					type="checkbox"
					class="rounded border-[var(--border-primary)]"
					checked={pwCreatePipelineIds.includes(p.id)}
					onchange={() => togglePwCreatePipeline(p.id)}
				/>
				<span>{p.name}</span>
			</label>
		{/each}
	</div>
	<div class="mt-6 flex justify-end gap-2">
		<Button variant="outline" onclick={() => (showCreatePw = false)}>Cancel</Button>
		<Button
			variant="primary"
			onclick={submitCreateProjectWebhook}
			loading={pwCreateLoading}
			disabled={pwCreatePipelineIds.length === 0}
		>
			Create
		</Button>
	</div>
</Dialog>

<Dialog
	bind:open={showEditPw}
	title="Edit generic webhook"
	description="Update description and inbound authentication. Enabling HMAC or query from an open URL generates a new signing secret (shown once below the list, same as create)."
	onclose={() => {
		editPwTarget = null;
	}}
>
	{#if editPwTarget}
		<div class="space-y-4">
			<div>
				<label class="mb-1 block text-sm font-medium text-[var(--text-primary)]" for="epw-desc"
					>Description</label
				>
				<Input id="epw-desc" bind:value={epwDescription} placeholder="Optional label" />
			</div>
			<div>
				<label class="mb-1 block text-sm font-medium text-[var(--text-primary)]" for="epw-auth-mode"
					>Inbound authentication</label
				>
				<Select
					id="epw-auth-mode"
					options={[
						{ value: 'hmac', label: 'HMAC header (X-Hub-Signature-256)' },
						{ value: 'query', label: 'Query parameter (secret in URL)' },
						{ value: 'none', label: 'None (open — no verification)' }
					]}
					bind:value={epwAuthMode}
				/>
			</div>
			{#if epwAuthMode === 'query'}
				<div>
					<label class="mb-1 block text-sm font-medium text-[var(--text-primary)]" for="epw-qparam"
						>Query parameter name</label
					>
					<Input id="epw-qparam" bind:value={epwQueryParam} placeholder="token" />
					<p class="mt-1 text-xs text-[var(--text-tertiary)]">
						Callers append
						<code class="font-mono">?{epwQueryParam.trim() || 'token'}=&lt;secret&gt;</code>.
					</p>
				</div>
			{/if}
			{#if epwAuthMode === 'none'}
				<p class="text-xs text-amber-700 dark:text-amber-400">
					Saving removes the stored signing secret. Anyone who can reach the URL can trigger pipelines.
				</p>
			{/if}
			<div class="flex justify-end gap-2 pt-2">
				<Button variant="outline" onclick={() => (showEditPw = false)}>Cancel</Button>
				<Button variant="primary" onclick={submitEditProjectWebhook} loading={pwEditLoading}>
					Save
				</Button>
			</div>
		</div>
	{/if}
</Dialog>

<Dialog
	bind:open={showClearPwInboundDialog}
	title="Use an open webhook URL?"
	onclose={() => {
		clearPwTarget = null;
	}}
>
	{#if clearPwTarget}
		<p class="text-sm text-[var(--text-secondary)]">
			This removes signing verification for this generic webhook. Any client that can POST to the URL can start runs
			in the configured pipelines. Prefer keeping HMAC or query auth when possible.
		</p>
		<div class="mt-6 flex justify-end gap-2">
			<Button variant="outline" onclick={() => (showClearPwInboundDialog = false)}>Cancel</Button>
			<Button
				variant="primary"
				class="bg-amber-600 hover:bg-amber-700"
				onclick={submitClearPwInbound}
				loading={clearPwLoading}
			>
				Remove verification
			</Button>
		</div>
	{/if}
</Dialog>
