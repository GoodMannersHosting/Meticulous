<script lang="ts">
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { Button, Card, Input, Select, Alert } from '$components/ui';
	import { apiMethods } from '$api/client';
	import type { BulkImportError, CatalogWorkflow, Project, StoredSecret, WorkspaceStoredSecretListItem } from '$api/types';
	import { ArrowLeft, Save, GitBranch, Layers, CheckSquare, Square } from 'lucide-svelte';

	let projects = $state<Project[]>([]);
	let loading = $state(true);
	let orgSecretsLoading = $state(false);
	let saving = $state(false);
	let error = $state<string | null>(null);

	const initialProjectParam = $page.url.searchParams.get('project') ?? '';

	/** Org import resolves only organization-scoped stored secrets (incl. platform-only catalog credentials). */
	let importScope = $state<'organization' | 'project'>(initialProjectParam ? 'project' : 'organization');
	let projectId = $state(initialProjectParam);

	/** Single vs bulk import mode. */
	const initialModeParam = $page.url.searchParams.get('mode') === 'bulk' ? 'bulk' : 'single';
	let importMode = $state<'single' | 'bulk'>(initialModeParam);

	let credentialSecrets = $state<(StoredSecret | WorkspaceStoredSecretListItem)[]>([]);

	let gitForm = $state({
		repository: '',
		git_ref: 'main',
		workflow_path: '.github/workflows/reusable.yaml',
		credentials_path: ''
	});

	/** Bulk import state */
	let bulkDirectoryPath = $state('.github/workflows');
	let bulkResult = $state<{ imported: CatalogWorkflow[]; errors: BulkImportError[] } | null>(null);

	const scopeOptions = [
		{ value: 'organization', label: 'Organization (global catalog)' },
		{ value: 'project', label: 'Project' }
	];

	const modeOptions = [
		{ value: 'single', label: 'Single workflow file' },
		{ value: 'bulk', label: 'Bulk import directory' }
	];

	$effect(() => {
		void loadProjects();
	});

	$effect(() => {
		if (importScope === 'organization') {
			void loadOrgSecrets();
			return;
		}
		const pid = projectId;
		if (!pid) {
			credentialSecrets = [];
			return;
		}
		void loadSecretsForProject(pid);
	});

	async function loadProjects() {
		loading = true;
		try {
			const response = await apiMethods.projects.list();
			projects = response.data;
			if (importScope === 'project' && !projectId && projects.length > 0) {
				projectId = projects[0].id;
			}
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load projects';
		} finally {
			loading = false;
		}
	}

	async function loadOrgSecrets() {
		orgSecretsLoading = true;
		error = null;
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
			credentialSecrets = acc;
		} catch (e) {
			credentialSecrets = [];
			error = e instanceof Error ? e.message : 'Failed to load organization secrets';
		} finally {
			orgSecretsLoading = false;
		}
	}

	async function loadSecretsForProject(id: string) {
		try {
			credentialSecrets = await apiMethods.storedSecrets.list(id);
		} catch {
			credentialSecrets = [];
		}
	}

	async function submit() {
		if (importScope === 'project' && !projectId.trim()) {
			error = 'Select a project';
			return;
		}
		if (!gitForm.repository.trim() || !gitForm.credentials_path.trim()) {
			error = 'Repository and GitHub App credential are required';
			return;
		}
		saving = true;
		error = null;
		bulkResult = null;

		if (importMode === 'single') {
			try {
				const body = {
					repository: gitForm.repository.trim(),
					git_ref: gitForm.git_ref.trim() || 'main',
					workflow_path: gitForm.workflow_path.trim(),
					credentials_path: gitForm.credentials_path.trim()
				};
				const wf =
					importScope === 'organization'
						? await apiMethods.wfCatalog.importGitOrganization(body)
						: await apiMethods.wfCatalog.importGit(projectId, body);
				goto(`/workflows/${wf.id}`);
			} catch (e) {
				error = e instanceof Error ? e.message : 'Failed to import workflow';
			} finally {
				saving = false;
			}
		} else {
			await submitBulk();
		}
	}

	async function submitBulk() {
		const explicitPaths = bulkDirectoryPath
			.split(',')
			.map((p) => p.trim())
			.filter(Boolean);
		const body = {
			repository: gitForm.repository.trim(),
			git_ref: gitForm.git_ref.trim() || 'main',
			workflow_paths: explicitPaths,
			credentials_path: gitForm.credentials_path.trim()
		};
		try {
			const result =
				importScope === 'organization'
					? await apiMethods.wfCatalog.bulkImportGitOrganization(body)
					: await apiMethods.wfCatalog.bulkImportGitProject(projectId, body);
			bulkResult = result;
		} catch (e) {
			error = e instanceof Error ? e.message : 'Bulk import failed';
		} finally {
			saving = false;
		}
	}

	const projectOptions = $derived(projects.map((p) => ({ value: p.id, label: p.name })));

	const credentialOptions = $derived([
		{ value: '', label: 'Select GitHub App secret…' },
		...credentialSecrets
			.filter((s) => s.kind === 'github_app')
			.map((s) => ({ value: s.path, label: `${s.path} (github_app)` }))
	]);

	const credentialsLoading = $derived(importScope === 'organization' ? orgSecretsLoading : loading);
</script>

<svelte:head>
	<title>Import workflow | Meticulous</title>
</svelte:head>

<div class="mx-auto max-w-3xl space-y-6">
	<div class="flex items-center gap-4">
		<Button variant="ghost" size="sm" href="/workflows">
			<ArrowLeft class="h-4 w-4" />
		</Button>
		<div>
			<h1 class="text-2xl font-bold text-[var(--text-primary)]">Import catalog workflow</h1>
			<p class="mt-1 text-[var(--text-secondary)]">
				Publish a reusable workflow YAML into the organization catalog. Use an organization-scoped GitHub App
				secret for global imports, or a project's secrets when importing from a project context.
			</p>
		</div>
	</div>

	{#if error}
		<Alert variant="error" dismissible ondismiss={() => (error = null)}>
			{error}
		</Alert>
	{/if}

	{#if bulkResult}
		<div class="space-y-4">
			<div class="rounded-lg border border-success-200 bg-success-50 p-4 dark:border-success-800 dark:bg-success-900/20">
				<div class="flex items-center gap-2 text-sm font-semibold text-success-800 dark:text-success-300">
					<Layers class="h-4 w-4" />
					Bulk import complete — {bulkResult.imported.length} workflow{bulkResult.imported.length !== 1 ? 's' : ''} imported
				</div>
				{#if bulkResult.imported.length > 0}
					<ul class="mt-3 space-y-1">
						{#each bulkResult.imported as wf (wf.id)}
							<li class="flex items-center gap-2 text-sm">
								<CheckSquare class="h-4 w-4 shrink-0 text-success-600 dark:text-success-400" />
								<a href="/workflows/{wf.id}" class="font-mono text-primary-600 hover:underline dark:text-primary-400">
									{wf.name} @ v{wf.version}
								</a>
							</li>
						{/each}
					</ul>
				{/if}
			</div>

			{#if bulkResult.errors.length > 0}
				<div class="rounded-lg border border-warning-200 bg-warning-50 p-4 dark:border-warning-800 dark:bg-warning-900/20">
					<p class="text-sm font-semibold text-warning-800 dark:text-warning-300">
						{bulkResult.errors.length} path{bulkResult.errors.length !== 1 ? 's' : ''} failed to import:
					</p>
					<ul class="mt-2 space-y-1.5">
						{#each bulkResult.errors as err (err.path)}
							<li class="text-sm">
								<span class="font-mono text-[var(--text-primary)]">{err.path}</span>
								<span class="ml-2 text-warning-700 dark:text-warning-400">— {err.message}</span>
							</li>
						{/each}
					</ul>
				</div>
			{/if}

			<div class="flex gap-3">
				<Button variant="outline" href="/workflows">Back to catalog</Button>
				<Button variant="outline" onclick={() => { bulkResult = null; }}>Import more</Button>
			</div>
		</div>
	{:else}
		<Card>
			<form
				onsubmit={(e) => {
					e.preventDefault();
					submit();
				}}
				class="space-y-6"
			>
				<!-- Mode toggle -->
				<div class="grid gap-3 sm:grid-cols-2">
					{#each modeOptions as opt}
						<button
							type="button"
							class="flex items-start gap-3 rounded-lg border p-3 text-left transition-colors {importMode === opt.value
								? 'border-primary-500 bg-primary-50 dark:bg-primary-900/20'
								: 'border-[var(--border-primary)] hover:bg-[var(--bg-tertiary)]'}"
							onclick={() => (importMode = opt.value as 'single' | 'bulk')}
						>
							<div class="mt-0.5 shrink-0">
								{#if importMode === opt.value}
									<CheckSquare class="h-4 w-4 text-primary-600 dark:text-primary-400" />
								{:else}
									<Square class="h-4 w-4 text-[var(--text-tertiary)]" />
								{/if}
							</div>
							<div>
								<p class="text-sm font-medium text-[var(--text-primary)]">
									{opt.value === 'single' ? 'Single file' : 'Bulk import'}
								</p>
								<p class="mt-0.5 text-xs text-[var(--text-secondary)]">
									{opt.value === 'single'
										? 'Import a specific workflow YAML file from any path'
										: 'Discover and import all workflow YAMLs from a directory'}
								</p>
							</div>
						</button>
					{/each}
				</div>

				<div>
					<label for="import-scope" class="block text-sm font-medium text-[var(--text-primary)]"
						>Import context</label
					>
					<Select
						id="import-scope"
						options={scopeOptions}
						bind:value={importScope}
						onchange={() => {
							gitForm.credentials_path = '';
						}}
						class="mt-1"
					/>
					{#if importScope === 'organization'}
						<p class="mt-2 text-xs text-[var(--text-secondary)]">
							Requires <span class="font-medium text-[var(--text-primary)]">org:admin</span> (or
							<span class="font-mono">*</span>). Credentials must be
							<span class="font-medium text-[var(--text-primary)]">organization-wide</span> stored secrets
							(including "platform only" secrets used to import the workflow catalog from source code).
						</p>
					{:else}
						<p class="mt-2 text-xs text-[var(--text-secondary)]">
							Uses GitHub App secrets visible to this project (org-wide secrets that propagate to projects are
							included).
						</p>
					{/if}
				</div>

				{#if importScope === 'project'}
					<div>
						<label for="project" class="block text-sm font-medium text-[var(--text-primary)]">Project</label>
						<Select
							id="project"
							options={projectOptions}
							bind:value={projectId}
							placeholder="Select a project…"
							disabled={loading}
							class="mt-1"
						/>
					</div>
				{/if}

				<div
					class="space-y-4 rounded-lg border border-[var(--border-primary)] bg-[var(--bg-tertiary)] p-4"
				>
					<p class="flex items-center gap-2 text-sm text-[var(--text-secondary)]">
						<GitBranch class="h-4 w-4 shrink-0" />
						{#if importMode === 'bulk'}
							All YAML workflow files in <span class="font-mono">.stable/workflows/</span> (or the explicit paths you specify) will be imported and submitted for review.
						{:else if importScope === 'organization'}
							Add an organization GitHub App secret, install the app on the target repo, then import the workflow
							file path.
						{:else}
							Add a project GitHub App secret first, install the app on the target repo, then import the
							workflow file path.
						{/if}
					</p>
					<div>
						<label class="block text-sm font-medium" for="repo">Repository</label>
						<Input
							id="repo"
							bind:value={gitForm.repository}
							placeholder="owner/repo or full repository URL"
							class="mt-1"
						/>
					</div>
					<div class="grid gap-4 sm:grid-cols-2">
						<div>
							<label class="block text-sm font-medium" for="ref">Git ref</label>
							<Input id="ref" bind:value={gitForm.git_ref} class="mt-1" />
						</div>
						{#if importMode === 'single'}
							<div>
								<label class="block text-sm font-medium" for="wfpath">Path to workflow YAML</label>
								<Input id="wfpath" bind:value={gitForm.workflow_path} class="mt-1" />
							</div>
						{:else}
							<div>
								<label class="block text-sm font-medium" for="dirpath">Specific paths (optional)</label>
								<Input
									id="dirpath"
									bind:value={bulkDirectoryPath}
									placeholder=".stable/workflows/deploy.yaml, .stable/workflows/test.yaml"
									class="mt-1"
								/>
								<p class="mt-1 text-xs text-[var(--text-tertiary)]">
									Leave blank to auto-discover all YAMLs under <span class="font-mono">.stable/workflows/</span> in the repo. Separate multiple paths with commas.
								</p>
							</div>
						{/if}
					</div>
					<div>
						<label class="block text-sm font-medium" for="cred">GitHub App credential</label>
						<Select
							id="cred"
							options={credentialOptions}
							bind:value={gitForm.credentials_path}
							disabled={credentialsLoading}
							class="mt-1"
						/>
						{#if !credentialsLoading && credentialOptions.length <= 1}
							<p class="mt-1 text-xs text-amber-700 dark:text-amber-400">
								No GitHub App secrets found for this context. Create one under Secrets &amp; Variables
								(organization scope) or the project's Secrets tab.
							</p>
						{/if}
					</div>
				</div>

				<div class="flex justify-end gap-3 border-t border-[var(--border-primary)] pt-4">
					<Button variant="outline" href="/workflows">Cancel</Button>
					<Button
						variant="primary"
						type="submit"
						loading={saving}
						disabled={(importScope === 'project' && !projectId) || credentialsLoading}
					>
						{#if importMode === 'bulk'}
							<Layers class="h-4 w-4" />
							Bulk import
						{:else}
							<Save class="h-4 w-4" />
							Import workflow
						{/if}
					</Button>
				</div>
			</form>
		</Card>
	{/if}
</div>
