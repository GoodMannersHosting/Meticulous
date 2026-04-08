<script lang="ts">
	import { goto } from '$app/navigation';
	import { onMount } from 'svelte';
	import { auth } from '$stores';
	import { Loader2 } from 'lucide-svelte';

	onMount(() => {
		if (auth.state === 'authenticated') {
			goto('/dashboard', { replaceState: true });
		} else if (auth.state === 'unauthenticated') {
			goto('/login', { replaceState: true });
		}
	});

	$effect(() => {
		if (auth.state === 'authenticated') {
			goto('/dashboard', { replaceState: true });
		} else if (auth.state === 'unauthenticated') {
			goto('/login', { replaceState: true });
		}
	});
</script>

<div class="flex min-h-screen items-center justify-center">
	<div class="flex flex-col items-center gap-4">
		<Loader2 class="h-8 w-8 animate-spin text-primary-600" />
		<p class="text-[var(--text-secondary)]">Loading...</p>
	</div>
</div>
