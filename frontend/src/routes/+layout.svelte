<script lang="ts">
	import '../app.css';
	import { Shell } from '$components/layout';
	import { page } from '$app/stores';
	import { auth } from '$stores';
	import { goto } from '$app/navigation';
	import { browser } from '$app/environment';

	let { children } = $props();

	const isAuthRoute = $derived($page.url.pathname.startsWith('/auth'));

	/** Keep users with a pending forced password change on the change-password screen. */
	$effect(() => {
		if (!browser) return;
		if (auth.state !== 'authenticated' || !auth.user?.password_must_change) return;
		const path = $page.url.pathname;
		if (path === '/auth/change-password' || path.startsWith('/auth/change-password/')) return;
		goto('/auth/change-password');
	});
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
