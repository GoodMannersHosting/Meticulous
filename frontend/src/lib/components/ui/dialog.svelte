<script lang="ts" module>
	export interface DialogProps {
		open?: boolean;
		title?: string;
		description?: string;
		/** Tailwind max-width on the panel (default `max-w-lg`). */
		maxWidthClass?: string;
		class?: string;
		onclose?: () => void;
	}
</script>

<script lang="ts">
	import { Dialog } from 'bits-ui';
	import { X } from 'lucide-svelte';

	let {
		open = $bindable(false),
		title,
		description,
		maxWidthClass = 'max-w-lg',
		class: className = '',
		onclose,
		children
	}: DialogProps & { children?: import('svelte').Snippet } = $props();

	function handleOpenChange(isOpen: boolean) {
		open = isOpen;
		if (!isOpen) {
			onclose?.();
		}
	}
</script>

<Dialog.Root {open} onOpenChange={handleOpenChange}>
	<Dialog.Portal>
		<Dialog.Overlay
			class="fixed inset-0 z-50 bg-black/50 backdrop-blur-sm data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0"
		/>
		<Dialog.Content
			class="fixed left-1/2 top-1/2 z-50 mx-4 flex w-[calc(100%-2rem)] -translate-x-1/2 -translate-y-1/2 flex-col rounded-xl bg-white p-6 shadow-xl data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 data-[state=closed]:zoom-out-95 data-[state=open]:zoom-in-95 dark:bg-secondary-900 {maxWidthClass} {className}"
		>
			{#if title}
				<Dialog.Title class="shrink-0 text-lg font-semibold text-secondary-900 dark:text-secondary-100">
					{title}
				</Dialog.Title>
			{/if}

			{#if description}
				<Dialog.Description class="mt-2 shrink-0 text-sm text-secondary-600 dark:text-secondary-400">
					{description}
				</Dialog.Description>
			{/if}

			<div class="mt-4 min-h-0 flex-1 overflow-hidden">
				{@render children?.()}
			</div>

			<Dialog.Close
				class="absolute right-4 top-4 rounded-md p-1 text-secondary-400 transition-colors hover:bg-secondary-100 hover:text-secondary-600 focus:outline-none focus:ring-2 focus:ring-primary-500 dark:hover:bg-secondary-800 dark:hover:text-secondary-300"
				aria-label="Close dialog"
			>
				<X class="h-5 w-5" />
			</Dialog.Close>
		</Dialog.Content>
	</Dialog.Portal>
</Dialog.Root>
