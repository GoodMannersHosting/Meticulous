<script lang="ts" module>
	export type StatusType =
		| 'pending'
		| 'queued'
		| 'running'
		| 'succeeded'
		| 'failed'
		| 'cancelled'
		| 'timed_out'
		| 'skipped'
		| 'online'
		| 'offline'
		| 'busy'
		| 'draining'
		| 'paused';

	export interface StatusBadgeProps {
		status: StatusType | string;
		size?: 'sm' | 'md';
		showIcon?: boolean;
		class?: string;
	}
</script>

<script lang="ts">
	import {
		Clock,
		Loader2,
		Play,
		CheckCircle2,
		XCircle,
		Ban,
		Timer,
		SkipForward,
		Wifi,
		WifiOff,
		Cpu,
		Power
	} from 'lucide-svelte';

	let {
		status,
		size = 'md',
		showIcon = true,
		class: className = ''
	}: StatusBadgeProps = $props();

	const normalizedStatus = $derived(status.toLowerCase().replace(/[-\s]/g, '_'));

	const statusConfig: Record<
		string,
		{ bg: string; text: string; icon: typeof Clock; animate?: boolean }
	> = {
		pending: {
			bg: 'bg-secondary-100 dark:bg-secondary-800',
			text: 'text-secondary-700 dark:text-secondary-300',
			icon: Clock
		},
		queued: {
			bg: 'bg-secondary-100 dark:bg-secondary-800',
			text: 'text-secondary-700 dark:text-secondary-300',
			icon: Clock
		},
		running: {
			bg: 'bg-primary-100 dark:bg-primary-900/30',
			text: 'text-primary-700 dark:text-primary-400',
			icon: Loader2,
			animate: true
		},
		succeeded: {
			bg: 'bg-success-100 dark:bg-success-900/30',
			text: 'text-success-700 dark:text-success-400',
			icon: CheckCircle2
		},
		failed: {
			bg: 'bg-error-100 dark:bg-error-900/30',
			text: 'text-error-700 dark:text-error-400',
			icon: XCircle
		},
		cancelled: {
			bg: 'bg-secondary-100 dark:bg-secondary-800',
			text: 'text-secondary-600 dark:text-secondary-400',
			icon: Ban
		},
		timed_out: {
			bg: 'bg-warning-100 dark:bg-warning-900/30',
			text: 'text-warning-700 dark:text-warning-400',
			icon: Timer
		},
		skipped: {
			bg: 'bg-secondary-100 dark:bg-secondary-800',
			text: 'text-secondary-500 dark:text-secondary-400',
			icon: SkipForward
		},
		online: {
			bg: 'bg-success-100 dark:bg-success-900/30',
			text: 'text-success-700 dark:text-success-400',
			icon: Wifi
		},
		offline: {
			bg: 'bg-secondary-100 dark:bg-secondary-800',
			text: 'text-secondary-500 dark:text-secondary-400',
			icon: WifiOff
		},
		busy: {
			bg: 'bg-warning-100 dark:bg-warning-900/30',
			text: 'text-warning-700 dark:text-warning-400',
			icon: Cpu
		},
		draining: {
			bg: 'bg-warning-100 dark:bg-warning-900/30',
			text: 'text-warning-700 dark:text-warning-400',
			icon: Power
		},
		paused: {
			bg: 'bg-secondary-100 dark:bg-secondary-800',
			text: 'text-secondary-700 dark:text-secondary-300',
			icon: Power
		}
	};

	const config = $derived(
		statusConfig[normalizedStatus] ?? {
			bg: 'bg-secondary-100 dark:bg-secondary-800',
			text: 'text-secondary-600 dark:text-secondary-400',
			icon: Clock
		}
	);

	const Icon = $derived(config.icon);

	const sizeClasses = {
		sm: 'px-2 py-0.5 text-xs gap-1',
		md: 'px-2.5 py-1 text-sm gap-1.5'
	};

	const iconSizes = {
		sm: 'h-3 w-3',
		md: 'h-3.5 w-3.5'
	};

	function formatStatus(s: string): string {
		return s
			.replace(/_/g, ' ')
			.replace(/\b\w/g, (c) => c.toUpperCase());
	}
</script>

<span
	class="
		inline-flex items-center rounded-full font-medium
		{config.bg} {config.text}
		{sizeClasses[size]}
		{className}
	"
>
	{#if showIcon}
		<Icon class="{iconSizes[size]} {config.animate ? 'animate-spin' : ''}" />
	{/if}
	{formatStatus(normalizedStatus)}
</span>
