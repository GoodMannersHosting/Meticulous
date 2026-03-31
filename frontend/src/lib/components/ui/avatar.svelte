<script lang="ts" module>
	export type AvatarSize = 'xs' | 'sm' | 'md' | 'lg' | 'xl';

	export interface AvatarProps {
		src?: string | null;
		email?: string | null;
		alt?: string;
		name?: string;
		size?: AvatarSize;
		useGravatar?: boolean;
		gravatarDefault?: 'mp' | 'identicon' | 'monsterid' | 'wavatar' | 'retro' | 'robohash';
		class?: string;
	}
</script>

<script lang="ts">
	import { getGravatarUrl } from '$utils/gravatar';

	let {
		src,
		email,
		alt = '',
		name = '',
		size = 'md',
		useGravatar = true,
		gravatarDefault = 'identicon',
		class: className = ''
	}: AvatarProps = $props();

	const sizeClasses: Record<AvatarSize, string> = {
		xs: 'h-6 w-6 text-xs',
		sm: 'h-8 w-8 text-sm',
		md: 'h-10 w-10 text-base',
		lg: 'h-12 w-12 text-lg',
		xl: 'h-16 w-16 text-xl'
	};

	const sizePixels: Record<AvatarSize, number> = {
		xs: 24,
		sm: 32,
		md: 40,
		lg: 48,
		xl: 64
	};

	function getInitials(name: string): string {
		const parts = name.trim().split(/\s+/);
		if (parts.length >= 2) {
			return (parts[0][0] + parts[parts.length - 1][0]).toUpperCase();
		}
		return name.slice(0, 2).toUpperCase();
	}

	const initials = $derived(name ? getInitials(name) : '?');

	const gravatarUrl = $derived(
		email && useGravatar
			? getGravatarUrl(email, {
					size: sizePixels[size] * 2,
					default: gravatarDefault
				})
			: null
	);

	const imageSrc = $derived(src || gravatarUrl);
	const hasImage = $derived(!!imageSrc);

	let imageError = $state(false);

	function handleImageError() {
		imageError = true;
	}

	const showImage = $derived(hasImage && !imageError);
</script>

<div
	class="
		inline-flex items-center justify-center rounded-full overflow-hidden
		bg-primary-100 text-primary-700
		dark:bg-primary-900/30 dark:text-primary-400
		{sizeClasses[size]}
		{className}
	"
>
	{#if showImage}
		<img
			src={imageSrc}
			{alt}
			class="h-full w-full rounded-full object-cover"
			onerror={handleImageError}
		/>
	{:else}
		<span class="font-medium">{initials}</span>
	{/if}
</div>
