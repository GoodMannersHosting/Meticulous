<script lang="ts">
	import '../app.css';
	import { Shell } from '$components/layout';
	import { page } from '$app/stores';

	let { children } = $props();

	const isAuthRoute = $derived($page.url.pathname.startsWith('/auth'));
</script>

<svelte:head>
	{#if $page.data.title}
		<title>{$page.data.title} | Meticulous</title>
	{:else}
		<title>Meticulous</title>
	{/if}
</svelte:head>

{#if isAuthRoute}
	{@render children()}
{:else}
	<Shell>
		{@render children()}
	</Shell>
{/if}
