<script lang="ts">
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { Button, Card, Input, Select, Alert } from '$components/ui';
	import { apiMethods } from '$api/client';
	import type { Project, CreatePipelineInput } from '$api/types';
	import { ArrowLeft, Save, Code } from 'lucide-svelte';

	let projects = $state<Project[]>([]);
	let loading = $state(true);
	let saving = $state(false);
	let error = $state<string | null>(null);

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

	$effect(() => {
		loadProjects();
	});

	async function loadProjects() {
		loading = true;
		try {
			const response = await apiMethods.projects.list();
			projects = response.items;
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load projects';
		} finally {
			loading = false;
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
		<Alert variant="error" dismissible ondismiss={() => (error = null)}>
			{error}
		</Alert>
	{/if}

	<Card>
		<form onsubmit={(e) => { e.preventDefault(); savePipeline(); }} class="space-y-6">
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
					<Input
						id="slug"
						placeholder="build-test"
						bind:value={form.slug}
						class="mt-1"
						required
					/>
					<p class="mt-1 text-xs text-[var(--text-tertiary)]">
						Used in URLs and CLI commands
					</p>
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
				<p class="mt-1 text-xs text-[var(--text-tertiary)]">
					Define jobs and steps in JSON format
				</p>
			</div>

			<div class="flex justify-end gap-3 pt-4 border-t border-[var(--border-primary)]">
				<Button variant="outline" href="/pipelines">
					Cancel
				</Button>
				<Button
					variant="primary"
					type="submit"
					loading={saving}
					disabled={!form.project_id || !form.name || !form.slug}
				>
					<Save class="h-4 w-4" />
					Create Pipeline
				</Button>
			</div>
		</form>
	</Card>
</div>
