<script lang="ts" module>
	export interface CopyButtonProps {
		text: string;
		size?: 'sm' | 'md';
		class?: string;
	}
</script>

<script lang="ts">
	import { Copy, Check } from 'lucide-svelte';

	let { text, size = 'md', class: className = '' }: CopyButtonProps = $props();

	let copied = $state(false);

	async function copyToClipboard() {
		try {
			await navigator.clipboard.writeText(text);
			copied = true;
			setTimeout(() => {
				copied = false;
			}, 2000);
		} catch {
			console.error('Failed to copy to clipboard');
		}
	}

	const sizeClasses = {
		sm: 'h-6 w-6 p-1',
		md: 'h-8 w-8 p-1.5'
	};

	const iconSizes = {
		sm: 'h-3.5 w-3.5',
		md: 'h-4 w-4'
	};
</script>

<button
	type="button"
	class="
		rounded-md transition-colors
		text-[var(--text-tertiary)] hover:text-[var(--text-primary)]
		hover:bg-[var(--bg-hover)]
		{sizeClasses[size]}
		{className}
	"
	onclick={copyToClipboard}
	aria-label={copied ? 'Copied' : 'Copy to clipboard'}
>
	{#if copied}
		<Check class="{iconSizes[size]} text-success-600" />
	{:else}
		<Copy class={iconSizes[size]} />
	{/if}
</button>
