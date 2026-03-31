<script lang="ts" module>
	export interface SkeletonProps {
		class?: string;
	}
</script>

<script lang="ts">
	let { class: className = '' }: SkeletonProps = $props();
</script>

<div
	class="
		animate-pulse rounded bg-secondary-200
		dark:bg-secondary-700
		{className}
	"
	aria-hidden="true"
></div>
