<script lang="ts" module>
	export interface CardProps {
		class?: string;
		padding?: 'none' | 'sm' | 'md' | 'lg';
		hover?: boolean;
	}
</script>

<script lang="ts">
	let {
		class: className = '',
		padding = 'md',
		hover = false,
		children
	}: CardProps & { children?: import('svelte').Snippet } = $props();

	const paddingClasses = {
		none: '',
		sm: 'p-3',
		md: 'p-5',
		lg: 'p-6'
	};

	const classes = $derived(
		[
			'rounded-xl border border-[var(--border-primary)] bg-[var(--bg-secondary)]',
			paddingClasses[padding],
			hover && 'transition-shadow hover:shadow-md',
			className
		]
			.filter(Boolean)
			.join(' ')
	);
</script>

<div class={classes}>
	{@render children?.()}
</div>
