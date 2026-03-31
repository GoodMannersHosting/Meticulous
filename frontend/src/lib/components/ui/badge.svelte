<script lang="ts" module>
	export type BadgeVariant = 'default' | 'primary' | 'secondary' | 'success' | 'warning' | 'error' | 'outline';
	export type BadgeSize = 'sm' | 'md';

	export interface BadgeProps {
		variant?: BadgeVariant;
		size?: BadgeSize;
		class?: string;
	}
</script>

<script lang="ts">
	let {
		variant = 'default',
		size = 'md',
		class: className = '',
		children
	}: BadgeProps & { children?: import('svelte').Snippet } = $props();

	const baseClasses = `
		inline-flex items-center font-medium rounded-full
		transition-colors duration-150
	`;

	const variantClasses: Record<BadgeVariant, string> = {
		default: `
			bg-secondary-100 text-secondary-800
			dark:bg-secondary-800 dark:text-secondary-200
		`,
		primary: `
			bg-primary-100 text-primary-800
			dark:bg-primary-900/30 dark:text-primary-300
		`,
		secondary: `
			bg-secondary-100 text-secondary-600
			dark:bg-secondary-800 dark:text-secondary-400
		`,
		success: `
			bg-success-100 text-success-700
			dark:bg-success-900/30 dark:text-success-500
		`,
		warning: `
			bg-warning-100 text-warning-700
			dark:bg-warning-900/30 dark:text-warning-500
		`,
		error: `
			bg-error-100 text-error-700
			dark:bg-error-900/30 dark:text-error-500
		`,
		outline: `
			border border-secondary-300 text-secondary-700
			dark:border-secondary-600 dark:text-secondary-300
		`
	};

	const sizeClasses: Record<BadgeSize, string> = {
		sm: 'px-2 py-0.5 text-xs',
		md: 'px-2.5 py-1 text-xs'
	};

	const classes = $derived(
		[baseClasses, variantClasses[variant], sizeClasses[size], className]
			.join(' ')
			.replace(/\s+/g, ' ')
			.trim()
	);
</script>

<span class={classes}>
	{@render children?.()}
</span>
