<script lang="ts" module>
	export interface PaginationProps {
		page: number;
		perPage: number;
		total: number;
		class?: string;
		onPageChange?: (page: number) => void;
		onPerPageChange?: (perPage: number) => void;
	}
</script>

<script lang="ts">
	import { ChevronLeft, ChevronRight, ChevronsLeft, ChevronsRight } from 'lucide-svelte';
	import Button from '../ui/button.svelte';
	import Select from '../ui/select.svelte';

	let {
		page = $bindable(1),
		perPage = $bindable(10),
		total,
		class: className = '',
		onPageChange,
		onPerPageChange
	}: PaginationProps = $props();

	const totalPages = $derived(Math.ceil(total / perPage));
	const start = $derived((page - 1) * perPage + 1);
	const end = $derived(Math.min(page * perPage, total));

	const canGoPrev = $derived(page > 1);
	const canGoNext = $derived(page < totalPages);

	const perPageOptions = [
		{ value: '10', label: '10 per page' },
		{ value: '20', label: '20 per page' },
		{ value: '25', label: '25 per page' },
		{ value: '50', label: '50 per page' },
		{ value: '100', label: '100 per page' }
	];

	function goToPage(newPage: number) {
		if (newPage >= 1 && newPage <= totalPages) {
			page = newPage;
			onPageChange?.(newPage);
		}
	}

	function handlePerPageChange(value: string) {
		const newPerPage = parseInt(value, 10);
		perPage = newPerPage;
		page = 1;
		onPerPageChange?.(newPerPage);
		onPageChange?.(1);
	}

	const visiblePages = $derived.by(() => {
		const pages: (number | 'ellipsis')[] = [];
		const delta = 1;

		for (let i = 1; i <= totalPages; i++) {
			if (
				i === 1 ||
				i === totalPages ||
				(i >= page - delta && i <= page + delta)
			) {
				pages.push(i);
			} else if (pages[pages.length - 1] !== 'ellipsis') {
				pages.push('ellipsis');
			}
		}

		return pages;
	});
</script>

<div class="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between {className}">
	<div class="flex items-center gap-4">
		<p class="text-sm text-[var(--text-secondary)]">
			Showing <span class="font-medium text-[var(--text-primary)]">{start}</span>
			to <span class="font-medium text-[var(--text-primary)]">{end}</span>
			of <span class="font-medium text-[var(--text-primary)]">{total}</span> results
		</p>

		<Select
			options={perPageOptions}
			value={String(perPage)}
			size="sm"
			onchange={handlePerPageChange}
		/>
	</div>

	<nav class="flex items-center gap-1" aria-label="Pagination">
		<Button
			variant="ghost"
			size="sm"
			disabled={!canGoPrev}
			onclick={() => goToPage(1)}
			class="hidden sm:flex"
		>
			<ChevronsLeft class="h-4 w-4" />
		</Button>

		<Button
			variant="ghost"
			size="sm"
			disabled={!canGoPrev}
			onclick={() => goToPage(page - 1)}
		>
			<ChevronLeft class="h-4 w-4" />
		</Button>

		<div class="flex items-center gap-1">
			{#each visiblePages as pageNum, index (index)}
				{#if pageNum === 'ellipsis'}
					<span class="px-2 text-[var(--text-tertiary)]">...</span>
				{:else}
					<Button
						variant={pageNum === page ? 'primary' : 'ghost'}
						size="sm"
						onclick={() => goToPage(pageNum)}
						class="min-w-[2rem]"
					>
						{pageNum}
					</Button>
				{/if}
			{/each}
		</div>

		<Button
			variant="ghost"
			size="sm"
			disabled={!canGoNext}
			onclick={() => goToPage(page + 1)}
		>
			<ChevronRight class="h-4 w-4" />
		</Button>

		<Button
			variant="ghost"
			size="sm"
			disabled={!canGoNext}
			onclick={() => goToPage(totalPages)}
			class="hidden sm:flex"
		>
			<ChevronsRight class="h-4 w-4" />
		</Button>
	</nav>
</div>
