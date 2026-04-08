<script lang="ts">
	import '../app.css';
	import { Shell } from '$components/layout';
	import { page } from '$app/stores';
	import { auth } from '$stores';
	import { goto } from '$app/navigation';
	import { browser } from '$app/environment';

	let { children } = $props();

	/** Routes served without main Shell (keep off `/auth/*` so gateways can send `/auth` only to met-api). */
	const isAuthRoute = $derived.by(() => {
		const p = $page.url.pathname;
		return (
			p === '/login' ||
			p.startsWith('/login/') ||
			p === '/oauth/callback' ||
			p.startsWith('/oauth/callback/') ||
			p === '/change-password' ||
			p.startsWith('/change-password/')
		);
	});

	/** Keep users with a pending forced password change on the change-password screen. */
	$effect(() => {
		if (!browser) return;
		if (auth.state !== 'authenticated' || !auth.user?.password_must_change) return;
		const path = $page.url.pathname;
		if (path === '/change-password' || path.startsWith('/change-password/')) return;
		goto('/change-password');
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
