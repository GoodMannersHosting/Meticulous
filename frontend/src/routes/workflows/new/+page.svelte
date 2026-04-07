<script lang="ts">
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { Button, Card, Input, Select, Alert } from '$components/ui';
	import { apiMethods } from '$api/client';
	import type { Project, StoredSecret, WorkspaceStoredSecretListItem } from '$api/types';
	import { ArrowLeft, Save, GitBranch } from 'lucide-svelte';

	let projects = $state<Project[]>([]);
	let loading = $state(true);
	let orgSecretsLoading = $state(false);
	let saving = $state(false);
	let error = $state<string | null>(null);

	const initialProjectParam = $page.url.searchParams.get('project') ?? '';

	/** Org import resolves only organization-scoped stored secrets (incl. platform-only catalog credentials). */
	let importScope = $state<'organization' | 'project'>(initialProjectParam ? 'project' : 'organization');
	let projectId = $state(initialProjectParam);

	let credentialSecrets = $state<(StoredSecret | WorkspaceStoredSecretListItem)[]>([]);

	let gitForm = $state({
		repository: '',
		git_ref: 'main',
		workflow_path: '.github/workflows/reusable.yaml',
		credentials_path: ''
	});

	const scopeOptions = [
		{ value: 'organization', label: 'Organization (global catalog)' },
		{ value: 'project', label: 'Project' }
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
				secret for global imports, or a project’s secrets when importing from a project context.
			</p>
		</div>
	</div>

	{#if error}
		<Alert variant="error" dismissible ondismiss={() => (error = null)}>
			{error}
		</Alert>
	{/if}

	<Card>
		<form
			onsubmit={(e) => {
				e.preventDefault();
				submit();
			}}
			class="space-y-6"
		>
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
						(including “platform only” secrets used to import the workflow catalog from source code).
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
					{#if importScope === 'organization'}
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
					<div>
						<label class="block text-sm font-medium" for="wfpath">Path to workflow YAML</label>
						<Input id="wfpath" bind:value={gitForm.workflow_path} class="mt-1" />
					</div>
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
							(organization scope) or the project’s Secrets tab.
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
					<Save class="h-4 w-4" />
					Import workflow
				</Button>
			</div>
		</form>
	</Card>
</div>
