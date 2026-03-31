<script lang="ts" module>
	export type ProgressVariant = 'primary' | 'success' | 'warning' | 'error';
	export type ProgressSize = 'sm' | 'md' | 'lg';

	export interface ProgressProps {
		value: number;
		max?: number;
		variant?: ProgressVariant;
		size?: ProgressSize;
		showLabel?: boolean;
		class?: string;
	}
</script>

<script lang="ts">
	let {
		value,
		max = 100,
		variant = 'primary',
		size = 'md',
		showLabel = false,
		class: className = ''
	}: ProgressProps = $props();

	const percentage = $derived(Math.min(100, Math.max(0, (value / max) * 100)));

	const sizeClasses: Record<ProgressSize, string> = {
		sm: 'h-1',
		md: 'h-2',
		lg: 'h-3'
	};

	const variantClasses: Record<ProgressVariant, string> = {
		primary: 'bg-primary-600',
		success: 'bg-success-600',
		warning: 'bg-warning-600',
		error: 'bg-error-600'
	};
</script>

<div class="w-full {className}">
	{#if showLabel}
		<div class="mb-1 flex justify-between text-sm">
			<span class="text-[var(--text-secondary)]">Progress</span>
			<span class="text-[var(--text-primary)]">{Math.round(percentage)}%</span>
		</div>
	{/if}

	<div
		class="w-full overflow-hidden rounded-full bg-[var(--bg-tertiary)] {sizeClasses[size]}"
		role="progressbar"
		aria-valuenow={value}
		aria-valuemin={0}
		aria-valuemax={max}
	>
		<div
			class="h-full rounded-full transition-all duration-300 ease-out {variantClasses[variant]}"
			style="width: {percentage}%"
		></div>
	</div>
</div>
