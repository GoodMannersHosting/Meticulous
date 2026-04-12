<script lang="ts">
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { Button, Card, Input, Select, Alert } from '$components/ui';
	import { apiMethods } from '$api/client';
	import type { Project, CreatePipelineInput, StoredSecret } from '$api/types';
	import { ArrowLeft, Save, Code, GitBranch } from 'lucide-svelte';
	import { parsePipelineDefinitionError } from '$lib/utils/apiErrorMessage';

	let projects = $state<Project[]>([]);
	let projectSecrets = $state<StoredSecret[]>([]);
	let loading = $state(true);
	let saving = $state(false);
	let error = $state<string | null>(null);

	let createMode = $state<'manual' | 'git'>('git');

	let form = $state<Partial<CreatePipelineInput>>({
		project_id: $page.url.searchParams.get('project') ?? '',
		name: '',
		slug: '',
		description: '',
		definition: {
			jobs: [
				{
					name: 'build',
					steps: [
						{ name: 'Checkout', uses: 'actions/checkout@v4' },
						{ name: 'Build', run: 'cargo build --release' }
					]
				}
			]
		}
	});

	let definitionText = $state(JSON.stringify(form.definition, null, 2));

	let gitForm = $state({
		repository: '',
		git_ref: 'main',
		scm_path: '.stable/pipeline.yaml',
		credentials_path: ''
	});

	$effect(() => {
		loadProjects();
	});

	$effect(() => {
		const pid = form.project_id;
		if (!pid || typeof pid !== 'string') {
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
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load projects';
		} finally {
			loading = false;
		}
	}

	async function loadSecretsForProject(projectId: string) {
		try {
			projectSecrets = await apiMethods.storedSecrets.list(projectId);
		} catch {
			projectSecrets = [];
		}
	}

	function generateSlug(name: string): string {
		return name
			.toLowerCase()
			.replace(/[^a-z0-9]+/g, '-')
			.replace(/^-|-$/g, '');
	}

	function handleNameChange(e: Event) {
		const name = (e.target as HTMLInputElement).value;
		form.name = name;
		if (!form.slug || form.slug === generateSlug(form.name || '')) {
			form.slug = generateSlug(name);
		}
	}

	function handleDefinitionChange(e: Event) {
		const text = (e.target as HTMLTextAreaElement).value;
		definitionText = text;
		try {
			form.definition = JSON.parse(text);
			error = null;
		} catch {
			error = 'Invalid JSON in definition';
		}
	}

	async function savePipeline() {
		if (!form.project_id || !form.name || !form.slug) return;

		if (createMode === 'git') {
			if (!gitForm.repository.trim() || !gitForm.credentials_path.trim()) {
				error = 'Repository and credentials are required for source code import';
				return;
			}
			saving = true;
			error = null;
			try {
				const pipeline = await apiMethods.pipelines.importGit(form.project_id as string, {
					name: form.name!,
					slug: form.slug!,
					description: form.description?.trim() || undefined,
					repository: gitForm.repository.trim(),
					git_ref: gitForm.git_ref.trim() || 'main',
					scm_path: gitForm.scm_path.trim(),
					credentials_path: gitForm.credentials_path.trim()
				});
				goto(`/pipelines/${pipeline.id}`);
			} catch (e) {
				error = e instanceof Error ? e.message : 'Failed to import pipeline';
			} finally {
				saving = false;
			}
			return;
		}

		try {
			form.definition = JSON.parse(definitionText);
		} catch {
			error = 'Invalid JSON in definition';
			return;
		}

		saving = true;
		error = null;
		try {
			const pipeline = await apiMethods.pipelines.create(form as CreatePipelineInput);
			goto(`/pipelines/${pipeline.id}`);
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to create pipeline';
		} finally {
			saving = false;
		}
	}

	const projectOptions = $derived(
		projects.map((p) => ({ value: p.id, label: p.name }))
	);

	const credentialOptions = $derived([
		{ value: '', label: 'Select GitHub App secret…' },
		...projectSecrets
			.filter((s) => s.kind === 'github_app')
			.map((s) => ({ value: s.path, label: `${s.path} (github_app)` }))
	]);

	const pipelineDefError = $derived(error ? parsePipelineDefinitionError(error) : null);
</script>

<svelte:head>
	<title>New Pipeline | Meticulous</title>
</svelte:head>

<div class="mx-auto max-w-3xl space-y-6">
	<div class="flex items-center gap-4">
		<Button variant="ghost" size="sm" href="/pipelines">
			<ArrowLeft class="h-4 w-4" />
		</Button>
		<div>
			<h1 class="text-2xl font-bold text-[var(--text-primary)]">Create Pipeline</h1>
			<p class="mt-1 text-[var(--text-secondary)]">
				Define a new CI/CD pipeline for your project.
			</p>
		</div>
	</div>

	{#if error}
		<Alert
			variant="error"
			title={pipelineDefError ? pipelineDefError.title : 'Something went wrong'}
			dismissible
			ondismiss={() => (error = null)}
		>
			{#if pipelineDefError}
				<div class="max-h-80 overflow-y-auto overscroll-y-contain pr-1">
					<ul class="list-disc space-y-2.5 pl-5">
						{#each pipelineDefError.bullets as line}
							<li class="leading-snug">{line}</li>
						{/each}
					</ul>
				</div>
			{:else}
				{error}
			{/if}
		</Alert>
	{/if}

	<Card>
		<form
			onsubmit={(e) => {
				e.preventDefault();
				savePipeline();
			}}
			class="space-y-6"
		>
			<div>
				<label for="project" class="block text-sm font-medium text-[var(--text-primary)]">
					Project
				</label>
				<Select
					options={projectOptions}
					bind:value={form.project_id}
					placeholder="Select a project..."
					class="mt-1"
				/>
			</div>

			<div class="flex gap-4 border-b border-[var(--border-primary)] pb-2">
				<button
					type="button"
					class="flex items-center gap-2 border-b-2 px-1 py-2 text-sm font-medium transition-colors"
					class:border-primary-500={createMode === 'manual'}
					class:text-[var(--text-primary)]={createMode === 'manual'}
					class:border-transparent={createMode !== 'manual'}
					class:text-[var(--text-secondary)]={createMode !== 'manual'}
					onclick={() => (createMode = 'manual')}
				>
					<Code class="h-4 w-4" />
					Manual (JSON)
				</button>
				<button
					type="button"
					class="flex items-center gap-2 border-b-2 px-1 py-2 text-sm font-medium transition-colors"
					class:border-primary-500={createMode === 'git'}
					class:text-[var(--text-primary)]={createMode === 'git'}
					class:border-transparent={createMode !== 'git'}
					class:text-[var(--text-secondary)]={createMode !== 'git'}
					onclick={() => (createMode = 'git')}
				>
					<GitBranch class="h-4 w-4" />
					Import from source code
				</button>
			</div>

			<div class="grid gap-4 sm:grid-cols-2">
				<div>
					<label for="name" class="block text-sm font-medium text-[var(--text-primary)]">
						Name
					</label>
					<Input
						id="name"
						placeholder="Build & Test"
						value={form.name}
						oninput={handleNameChange}
						class="mt-1"
						required
					/>
				</div>
				<div>
					<label for="slug" class="block text-sm font-medium text-[var(--text-primary)]">
						Slug
					</label>
					<Input id="slug" placeholder="build-test" bind:value={form.slug} class="mt-1" required />
					<p class="mt-1 text-xs text-[var(--text-tertiary)]">Used in URLs and CLI commands</p>
				</div>
			</div>

			<div>
				<label for="description" class="block text-sm font-medium text-[var(--text-primary)]">
					Description
				</label>
				<Input
					id="description"
					placeholder="Optional description..."
					bind:value={form.description}
					class="mt-1"
				/>
			</div>

			{#if createMode === 'git'}
				<div class="space-y-4 rounded-lg border border-[var(--border-primary)] bg-[var(--bg-tertiary)] p-4">
					<p class="text-sm text-[var(--text-secondary)]">
						Add a project GitHub App secret first (Project → Secrets), install the app on the target
						repo, then import the pipeline YAML path.
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
							<label class="block text-sm font-medium" for="scmpath">Path to YAML in repo</label>
							<Input id="scmpath" bind:value={gitForm.scm_path} class="mt-1" />
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
			{:else}
				<div>
					<div class="mb-2 flex items-center justify-between">
						<label for="definition" class="block text-sm font-medium text-[var(--text-primary)]">
							Pipeline Definition
						</label>
						<Code class="h-4 w-4 text-[var(--text-tertiary)]" />
					</div>
					<textarea
						id="definition"
						value={definitionText}
						oninput={handleDefinitionChange}
						class="
						h-80 w-full rounded-lg border border-[var(--border-primary)]
						bg-secondary-950 p-4 font-mono text-sm text-secondary-100
						focus:outline-none focus:ring-2 focus:ring-primary-500
					"
						placeholder="Enter pipeline definition as JSON..."
					></textarea>
					<p class="mt-1 text-xs text-[var(--text-tertiary)]">Define jobs and steps in JSON format</p>
				</div>
			{/if}

			<div class="flex justify-end gap-3 border-t border-[var(--border-primary)] pt-4">
				<Button variant="outline" href="/pipelines">Cancel</Button>
				<Button
					variant="primary"
					type="submit"
					loading={saving}
					disabled={!form.project_id || !form.name || !form.slug}
				>
					<Save class="h-4 w-4" />
					{createMode === 'git' ? 'Import Pipeline' : 'Create Pipeline'}
				</Button>
			</div>
		</form>
	</Card>
</div>
