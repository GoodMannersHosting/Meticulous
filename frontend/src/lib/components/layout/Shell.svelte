<script lang="ts">
	import { sidebar } from '$stores';
	import Sidebar from './Sidebar.svelte';
	import TopBar from './TopBar.svelte';

	let { children }: { children?: import('svelte').Snippet } = $props();

	const mainPaddingLeft = $derived(
		sidebar.isMobile
			? '0'
			: sidebar.collapsed
				? 'var(--sidebar-collapsed-width)'
				: 'var(--sidebar-width)'
	);
</script>

<div class="flex h-dvh max-h-dvh flex-col overflow-hidden bg-[var(--bg-primary)]">
	<Sidebar />

	<div
		class="flex min-h-0 flex-1 flex-col overflow-hidden transition-[padding] duration-200 ease-out"
		style="padding-left: {mainPaddingLeft};"
	>
		<TopBar />

		<main
			class="flex min-h-0 flex-1 flex-col overflow-y-auto overscroll-y-contain px-4 py-6 sm:px-6 lg:px-8"
		>
			{@render children?.()}
		</main>
	</div>

	{#if sidebar.isMobile && sidebar.mobileOpen}
		<button
			type="button"
			class="fixed inset-0 z-30 bg-black/50 backdrop-blur-sm"
			aria-label="Close sidebar"
			onclick={() => sidebar.closeMobile()}
		></button>
	{/if}
</div>
