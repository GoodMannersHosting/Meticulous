<script lang="ts">
	import { page } from '$app/stores';
	import { Button } from '$components/ui';
	import { AlertTriangle, Home, RotateCcw } from 'lucide-svelte';

	const statusCode = $derived($page.status);
	const errorMessage = $derived($page.error?.message || 'Something went wrong');

	const title = $derived.by(() => {
		switch (statusCode) {
			case 404:
				return 'Page not found';
			case 403:
				return 'Access denied';
			case 500:
				return 'Server error';
			default:
				return 'An error occurred';
		}
	});

	const description = $derived.by(() => {
		switch (statusCode) {
			case 404:
				return "The page you're looking for doesn't exist or has been moved.";
			case 403:
				return "You don't have permission to access this resource.";
			case 500:
				return 'Something went wrong on our end. Please try again later.';
			default:
				return errorMessage;
		}
	});
</script>

<div class="flex min-h-[60vh] flex-col items-center justify-center px-4 text-center">
	<div class="mb-6 rounded-full bg-error-100 p-4 dark:bg-error-900/30">
		<AlertTriangle class="h-10 w-10 text-error-600 dark:text-error-500" />
	</div>

	<h1 class="text-4xl font-bold text-[var(--text-primary)]">
		{statusCode}
	</h1>

	<h2 class="mt-2 text-xl font-semibold text-[var(--text-primary)]">
		{title}
	</h2>

	<p class="mt-2 max-w-md text-[var(--text-secondary)]">
		{description}
	</p>

	<div class="mt-8 flex gap-4">
		<Button variant="outline" onclick={() => history.back()}>
			<RotateCcw class="h-4 w-4" />
			Go back
		</Button>

		<Button variant="primary" href="/dashboard">
			<Home class="h-4 w-4" />
			Dashboard
		</Button>
	</div>
</div>
