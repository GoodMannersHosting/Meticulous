<script lang="ts">
	import { Button, Card, Input, Badge, Dialog } from '$components/ui';
	import { DataTable, EmptyState, Skeleton } from '$components/data';
	import { apiMethods } from '$api/client';
	import type { Project, CreateProjectInput } from '$api/types';
	import { formatRelativeTime } from '$utils/format';
	import { Plus, Search, FolderKanban, GitBranch, ExternalLink } from 'lucide-svelte';
	import type { Column } from '$components/data/DataTable.svelte';
	import { goto } from '$app/navigation';

	let projects = $state<Project[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let searchQuery = $state('');
	let showCreateDialog = $state(false);

	let newProject = $state<Partial<CreateProjectInput>>({
		name: '',
		slug: '',
		description: '',
		owner_type: 'user',
		owner_id: ''
	});
	let creating = $state(false);

	$effect(() => {
		loadProjects();
	});

	async function loadProjects() {
		loading = true;
		error = null;
		try {
			const response = await apiMethods.projects.list({ search: searchQuery || undefined });
			projects = response.items;
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load projects';
			projects = [];
		} finally {
			loading = false;
		}
	}

	async function createProject() {
		if (!newProject.name || !newProject.slug) return;

		creating = true;
		try {
			const created = await apiMethods.projects.create(newProject as CreateProjectInput);
			projects = [created, ...projects];
			showCreateDialog = false;
			newProject = { name: '', slug: '', description: '', owner_type: 'user', owner_id: '' };
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to create project';
		} finally {
			creating = false;
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
		newProject.name = name;
		if (!newProject.slug || newProject.slug === generateSlug(newProject.name || '')) {
			newProject.slug = generateSlug(name);
		}
	}

	const columns: Column<Project>[] = [
		{
			key: 'name',
			label: 'Name',
			sortable: true,
			render: (_, row) => `
				<div class="flex items-center gap-3">
					<div class="flex h-10 w-10 items-center justify-center rounded-lg bg-primary-100 dark:bg-primary-900/30">
						<svg class="h-5 w-5 text-primary-600 dark:text-primary-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
							<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z"/>
						</svg>
					</div>
					<div>
						<div class="font-medium text-[var(--text-primary)]">${row.name}</div>
						<div class="text-sm text-[var(--text-secondary)]">${row.slug}</div>
					</div>
				</div>
			`
		},
		{
			key: 'description',
			label: 'Description',
			render: (value) => (value as string) || '<span class="text-[var(--text-tertiary)]">No description</span>'
		},
		{
			key: 'updated_at',
			label: 'Last Updated',
			sortable: true,
			render: (value) => formatRelativeTime(value as string)
		}
	];

	function handleRowClick(project: Project) {
		goto(`/projects/${project.id}`);
	}
</script>

<svelte:head>
	<title>Projects | Meticulous</title>
</svelte:head>

<div class="space-y-6">
	<div class="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
		<div>
			<h1 class="text-2xl font-bold text-[var(--text-primary)]">Projects</h1>
			<p class="mt-1 text-[var(--text-secondary)]">
				Organize your pipelines and configurations.
			</p>
		</div>

		<Button variant="primary" onclick={() => (showCreateDialog = true)}>
			<Plus class="h-4 w-4" />
			New Project
		</Button>
	</div>

	<div class="flex gap-4">
		<div class="relative flex-1 max-w-md">
			<Search class="absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-[var(--text-tertiary)]" />
			<Input
				type="search"
				placeholder="Search projects..."
				class="pl-10"
				bind:value={searchQuery}
				onchange={() => loadProjects()}
			/>
		</div>
	</div>

	{#if error}
		<div class="rounded-lg border border-error-200 bg-error-50 p-4 text-sm text-error-700 dark:border-error-800 dark:bg-error-900/20 dark:text-error-400">
			{error}
		</div>
	{/if}

	{#if loading}
		<Card>
			<div class="space-y-4">
				{#each Array(5) as _, i (i)}
					<div class="flex items-center gap-4">
						<Skeleton class="h-10 w-10 rounded-lg" />
						<div class="flex-1 space-y-2">
							<Skeleton class="h-4 w-48" />
							<Skeleton class="h-3 w-32" />
						</div>
						<Skeleton class="h-4 w-24" />
					</div>
				{/each}
			</div>
		</Card>
	{:else if projects.length === 0}
		<Card>
			<EmptyState
				title="No projects yet"
				description="Create your first project to organize your CI/CD pipelines."
			>
				<Button variant="primary" onclick={() => (showCreateDialog = true)}>
					<Plus class="h-4 w-4" />
					Create Project
				</Button>
			</EmptyState>
		</Card>
	{:else}
		<DataTable
			{columns}
			data={projects}
			rowKey="id"
			onRowClick={handleRowClick}
		/>
	{/if}
</div>

<Dialog bind:open={showCreateDialog} title="Create Project">
	<form onsubmit={(e) => { e.preventDefault(); createProject(); }} class="space-y-4">
		<div>
			<label for="project-name" class="block text-sm font-medium text-[var(--text-primary)]">
				Name
			</label>
			<Input
				id="project-name"
				placeholder="My Project"
				value={newProject.name}
				oninput={handleNameChange}
				class="mt-1"
				required
			/>
		</div>

		<div>
			<label for="project-slug" class="block text-sm font-medium text-[var(--text-primary)]">
				Slug
			</label>
			<Input
				id="project-slug"
				placeholder="my-project"
				bind:value={newProject.slug}
				class="mt-1"
				required
			/>
			<p class="mt-1 text-xs text-[var(--text-tertiary)]">
				Used in URLs and API references
			</p>
		</div>

		<div>
			<label for="project-description" class="block text-sm font-medium text-[var(--text-primary)]">
				Description
			</label>
			<Input
				id="project-description"
				placeholder="Optional description..."
				bind:value={newProject.description}
				class="mt-1"
			/>
		</div>

		<div class="flex justify-end gap-3 pt-4">
			<Button variant="outline" onclick={() => (showCreateDialog = false)}>
				Cancel
			</Button>
			<Button variant="primary" type="submit" loading={creating}>
				Create Project
			</Button>
		</div>
	</form>
</Dialog>
