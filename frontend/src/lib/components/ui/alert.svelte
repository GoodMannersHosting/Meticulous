<script lang="ts" module>
	export type AlertVariant = 'info' | 'success' | 'warning' | 'error';

	export interface AlertProps {
		variant?: AlertVariant;
		title?: string;
		dismissible?: boolean;
		class?: string;
		ondismiss?: () => void;
	}
</script>

<script lang="ts">
	import { Info, CheckCircle2, AlertTriangle, XCircle, X } from 'lucide-svelte';

	let {
		variant = 'info',
		title,
		dismissible = false,
		class: className = '',
		ondismiss,
		children
	}: AlertProps & { children?: import('svelte').Snippet } = $props();

	const variantStyles: Record<AlertVariant, { bg: string; border: string; icon: typeof Info }> = {
		info: {
			bg: 'bg-primary-50 dark:bg-primary-900/20',
			border: 'border-primary-200 dark:border-primary-800',
			icon: Info
		},
		success: {
			bg: 'bg-success-50 dark:bg-success-900/20',
			border: 'border-success-200 dark:border-success-800',
			icon: CheckCircle2
		},
		warning: {
			bg: 'bg-warning-50 dark:bg-warning-900/20',
			border: 'border-warning-200 dark:border-warning-800',
			icon: AlertTriangle
		},
		error: {
			bg: 'bg-error-50 dark:bg-error-900/20',
			border: 'border-error-200 dark:border-error-800',
			icon: XCircle
		}
	};

	const iconColors: Record<AlertVariant, string> = {
		info: 'text-primary-600 dark:text-primary-400',
		success: 'text-success-600 dark:text-success-400',
		warning: 'text-warning-600 dark:text-warning-400',
		error: 'text-error-600 dark:text-error-400'
	};

	const style = $derived(variantStyles[variant]);
	const Icon = $derived(style.icon);
</script>

<div
	class="
		flex gap-3 rounded-lg border p-4
		{style.bg} {style.border}
		{className}
	"
	role="alert"
>
	<Icon class="h-5 w-5 flex-shrink-0 {iconColors[variant]}" />

	<div class="flex-1">
		{#if title}
			<p class="font-medium text-[var(--text-primary)]">{title}</p>
		{/if}
		<div class="text-sm text-[var(--text-secondary)] {title ? 'mt-1' : ''}">
			{@render children?.()}
		</div>
	</div>

	{#if dismissible}
		<button
			type="button"
			class="
				flex-shrink-0 rounded p-1
				text-[var(--text-tertiary)] transition-colors
				hover:bg-[var(--bg-hover)] hover:text-[var(--text-secondary)]
			"
			onclick={ondismiss}
			aria-label="Dismiss"
		>
			<X class="h-4 w-4" />
		</button>
	{/if}
</div>
