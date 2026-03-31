<script lang="ts" module>
	export type ButtonVariant = 'primary' | 'secondary' | 'outline' | 'ghost' | 'destructive';
	export type ButtonSize = 'sm' | 'md' | 'lg';

	export interface ButtonProps {
		variant?: ButtonVariant;
		size?: ButtonSize;
		disabled?: boolean;
		loading?: boolean;
		type?: 'button' | 'submit' | 'reset';
		href?: string;
		class?: string;
		title?: string;
		onclick?: (event: MouseEvent) => void;
	}
</script>

<script lang="ts">
	import { Loader2 } from 'lucide-svelte';

	let {
		variant = 'primary',
		size = 'md',
		disabled = false,
		loading = false,
		type = 'button',
		href,
		class: className = '',
		title,
		onclick,
		children
	}: ButtonProps & { children?: import('svelte').Snippet } = $props();

	const baseClasses = `
		inline-flex items-center justify-center gap-2
		font-medium rounded-lg
		transition-colors duration-150
		focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-offset-2
		disabled:pointer-events-none disabled:opacity-50
	`;

	const variantClasses: Record<ButtonVariant, string> = {
		primary: `
			bg-primary-600 text-white
			hover:bg-primary-700
			focus-visible:ring-primary-500
		`,
		secondary: `
			bg-secondary-100 text-secondary-900
			hover:bg-secondary-200
			focus-visible:ring-secondary-500
			dark:bg-secondary-800 dark:text-secondary-100 dark:hover:bg-secondary-700
		`,
		outline: `
			border border-secondary-300 bg-transparent text-secondary-700
			hover:bg-secondary-100
			focus-visible:ring-secondary-500
			dark:border-secondary-600 dark:text-secondary-300 dark:hover:bg-secondary-800
		`,
		ghost: `
			bg-transparent text-secondary-700
			hover:bg-secondary-100
			focus-visible:ring-secondary-500
			dark:text-secondary-300 dark:hover:bg-secondary-800
		`,
		destructive: `
			bg-error-600 text-white
			hover:bg-error-700
			focus-visible:ring-error-500
		`
	};

	const sizeClasses: Record<ButtonSize, string> = {
		sm: 'h-8 px-3 text-sm',
		md: 'h-10 px-4 text-sm',
		lg: 'h-12 px-6 text-base'
	};

	const classes = $derived(
		[baseClasses, variantClasses[variant], sizeClasses[size], className]
			.join(' ')
			.replace(/\s+/g, ' ')
			.trim()
	);

	const isDisabled = $derived(disabled || loading);
</script>

{#if href && !isDisabled}
	<a {href} class={classes} aria-disabled={isDisabled} {title}>
		{#if loading}
			<Loader2 class="h-4 w-4 animate-spin" aria-hidden="true" />
		{/if}
		{@render children?.()}
	</a>
{:else}
	<button
		{type}
		class={classes}
		disabled={isDisabled}
		aria-disabled={isDisabled}
		aria-busy={loading}
		{title}
		{onclick}
	>
		{#if loading}
			<Loader2 class="h-4 w-4 animate-spin" aria-hidden="true" />
		{/if}
		{@render children?.()}
	</button>
{/if}
