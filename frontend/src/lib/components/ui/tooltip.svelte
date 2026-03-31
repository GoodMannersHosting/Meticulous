<script lang="ts" module>
	export type TooltipSide = 'top' | 'right' | 'bottom' | 'left';

	export interface TooltipProps {
		content: string;
		side?: TooltipSide;
		delay?: number;
		class?: string;
	}
</script>

<script lang="ts">
	import { Tooltip } from 'bits-ui';

	let {
		content,
		side = 'top',
		delay = 200,
		class: className = '',
		children
	}: TooltipProps & { children?: import('svelte').Snippet } = $props();
</script>

<Tooltip.Root delayDuration={delay}>
	<Tooltip.Trigger>
		{@render children?.()}
	</Tooltip.Trigger>

	<Tooltip.Portal>
		<Tooltip.Content
			{side}
			sideOffset={4}
			class="
				z-50 overflow-hidden rounded-md bg-secondary-900 px-3 py-1.5 
				text-sm text-secondary-50 shadow-md
				dark:bg-secondary-100 dark:text-secondary-900
				{className}
			"
		>
			{content}
			<Tooltip.Arrow class="fill-secondary-900 dark:fill-secondary-100" />
		</Tooltip.Content>
	</Tooltip.Portal>
</Tooltip.Root>
