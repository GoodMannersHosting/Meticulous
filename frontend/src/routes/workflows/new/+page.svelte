<script lang="ts">
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { Button, Card, Input, Select, Alert } from '$components/ui';
	import { apiMethods } from '$api/client';
	import type { Project, StoredSecret } from '$api/types';
	import { ArrowLeft, Save, GitBranch } from 'lucide-svelte';

	let projects = $state<Project[]>([]);
	let projectSecrets = $state<StoredSecret[]>([]);
	let loading = $state(true);
	let saving = $state(false);
	let error = $state<string | null>(null);

	let projectId = $state($page.url.searchParams.get('project') ?? '');

	let gitForm = $state({
		repository: '',
		git_ref: 'main',
		workflow_path: '.github/workflows/reusable.yaml',
		credentials_path: ''
	});

	$effect(() => {
		void loadProjects();
	});

	$effect(() => {
		const pid = projectId;
		if (!pid) {
			projectSecrets = [];
			return;
		}
		void loadSecretsForProject(pid);
	});

	async function loadProjects() {
		loading = true;
		try {
			const response = await apiMethods.projects.list();
			projects = response.data;
			if (!projectId && projects.length > 0) {
				projectId = projects[0].id;
			}
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load projects';
		} finally {
			loading = false;
		}
	}

	async function loadSecretsForProject(id: string) {
		try {
			projectSecrets = await apiMethods.storedSecrets.list(id);
		} catch {
			projectSecrets = [];
		}
	}

	async function submit() {
		if (!projectId.trim()) {
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
			const wf = await apiMethods.wfCatalog.importGit(projectId, {
				repository: gitForm.repository.trim(),
				git_ref: gitForm.git_ref.trim() || 'main',
				workflow_path: gitForm.workflow_path.trim(),
				credentials_path: gitForm.credentials_path.trim()
			});
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
		...projectSecrets
			.filter((s) => s.kind === 'github_app')
			.map((s) => ({ value: s.path, label: `${s.path} (github_app)` }))
	]);
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
				Publish a reusable workflow YAML into the org catalog (GitHub via project secret).
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

			<div
				class="space-y-4 rounded-lg border border-[var(--border-primary)] bg-[var(--bg-tertiary)] p-4"
			>
				<p class="flex items-center gap-2 text-sm text-[var(--text-secondary)]">
					<GitBranch class="h-4 w-4 shrink-0" />
					Add a project GitHub App secret first, install the app on the target repo, then import the
					workflow file path.
				</p>
				<div>
					<label class="block text-sm font-medium" for="repo">Repository</label>
					<Input
						id="repo"
						bind:value={gitForm.repository}
						placeholder="org/repo or https://github.com/org/repo"
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
						class="mt-1"
					/>
				</div>
			</div>

			<div class="flex justify-end gap-3 border-t border-[var(--border-primary)] pt-4">
				<Button variant="outline" href="/workflows">Cancel</Button>
				<Button
					variant="primary"
					type="submit"
					loading={saving}
					disabled={!projectId || loading}
				>
					<Save class="h-4 w-4" />
					Import workflow
				</Button>
			</div>
		</form>
	</Card>
</div>
