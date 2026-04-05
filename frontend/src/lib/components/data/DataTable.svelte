<script lang="ts" module>
	export type SortDirection = 'asc' | 'desc' | null;

	export interface Column<T> {
		key: keyof T | string;
		label: string;
		sortable?: boolean;
		width?: string;
		align?: 'left' | 'center' | 'right';
		render?: (value: unknown, row: T) => string;
	}

	export interface DataTableProps<T> {
		columns: Column<T>[];
		data: T[];
		loading?: boolean;
		selectable?: boolean;
		selectedRows?: T[];
		sortKey?: string | null;
		sortDirection?: SortDirection;
		rowKey?: keyof T;
		class?: string;
		onSort?: (key: string, direction: SortDirection) => void;
		onSelect?: (rows: T[]) => void;
		onRowClick?: (row: T) => void;
	}
</script>

<script lang="ts" generics="T extends object">
	import { ArrowUp, ArrowDown, ArrowUpDown, Check } from 'lucide-svelte';
	import Skeleton from './Skeleton.svelte';
	import EmptyState from './EmptyState.svelte';

	let {
		columns,
		data,
		loading = false,
		selectable = false,
		selectedRows = $bindable([]),
		sortKey = null,
		sortDirection = null,
		rowKey = 'id' as keyof T,
		class: className = '',
		onSort,
		onSelect,
		onRowClick
	}: DataTableProps<T> = $props();

	const allSelected = $derived(
		data.length > 0 && selectedRows.length === data.length
	);

	const someSelected = $derived(
		selectedRows.length > 0 && selectedRows.length < data.length
	);

	function getNestedValue(obj: T, path: string): unknown {
		return path.split('.').reduce((acc: unknown, part) => {
			if (acc && typeof acc === 'object') {
				return (acc as Record<string, unknown>)[part];
			}
			return undefined;
		}, obj);
	}

	function handleSort(key: string, e: MouseEvent) {
		e.preventDefault();
		e.stopPropagation();
		let newDirection: SortDirection;
		const sk = sortKey == null ? null : String(sortKey);
		if (sk !== key) {
			newDirection = 'asc';
		} else if (sortDirection === 'asc') {
			newDirection = 'desc';
		} else {
			newDirection = null;
		}
		onSort?.(key, newDirection);
	}

	function isActiveSortColumn(columnKey: keyof T | string): boolean {
		return sortKey != null && String(sortKey) === String(columnKey);
	}

	function handleSelectAll() {
		if (allSelected) {
			selectedRows = [];
		} else {
			selectedRows = [...data];
		}
		onSelect?.(selectedRows);
	}

	function handleSelectRow(row: T) {
		const rowId = row[rowKey];
		const isSelected = selectedRows.some((r) => r[rowKey] === rowId);

		if (isSelected) {
			selectedRows = selectedRows.filter((r) => r[rowKey] !== rowId);
		} else {
			selectedRows = [...selectedRows, row];
		}
		onSelect?.(selectedRows);
	}

	function isRowSelected(row: T): boolean {
		return selectedRows.some((r) => r[rowKey] === row[rowKey]);
	}

	const alignClasses = {
		left: 'text-left',
		center: 'text-center',
		right: 'text-right'
	};
</script>

<div class="overflow-hidden rounded-lg border border-[var(--border-primary)] {className}">
	<div class="overflow-x-auto">
		<table class="w-full border-collapse text-sm">
			<thead class="bg-[var(--bg-tertiary)]">
				<tr>
					{#if selectable}
						<th class="w-12 px-4 py-3">
							<button
								type="button"
								class="
									flex h-5 w-5 items-center justify-center rounded border
									transition-colors
									{allSelected || someSelected
										? 'border-primary-600 bg-primary-600 text-white'
										: 'border-secondary-300 bg-white dark:border-secondary-600 dark:bg-secondary-800'}
								"
								onclick={handleSelectAll}
								aria-label="Select all rows"
							>
								{#if allSelected}
									<Check class="h-3.5 w-3.5" />
								{:else if someSelected}
									<div class="h-0.5 w-2.5 bg-current"></div>
								{/if}
							</button>
						</th>
					{/if}

					{#each columns as column (column.key)}
						<th
							class="
								px-4 py-3 font-medium text-[var(--text-secondary)]
								{alignClasses[column.align ?? 'left']}
							"
							style={column.width ? `width: ${column.width}` : undefined}
						>
							{#if column.sortable}
								<button
									type="button"
									class="
										inline-flex items-center gap-1
										transition-colors hover:text-[var(--text-primary)]
									"
									onclick={(e) => handleSort(String(column.key), e)}
								>
									{column.label}
									{#if isActiveSortColumn(column.key)}
										{#if sortDirection === 'asc'}
											<ArrowUp class="h-4 w-4" />
										{:else if sortDirection === 'desc'}
											<ArrowDown class="h-4 w-4" />
										{/if}
									{:else}
										<ArrowUpDown class="h-4 w-4 opacity-50" />
									{/if}
								</button>
							{:else}
								{column.label}
							{/if}
						</th>
					{/each}
				</tr>
			</thead>

			<tbody class="divide-y divide-[var(--border-secondary)]">
				{#if loading}
					{#each Array(5) as _, i (i)}
						<tr>
							{#if selectable}
								<td class="px-4 py-3">
									<Skeleton class="h-5 w-5 rounded" />
								</td>
							{/if}
							{#each columns as column (column.key)}
								<td class="px-4 py-3">
									<Skeleton class="h-5 w-24" />
								</td>
							{/each}
						</tr>
					{/each}
				{:else if data.length === 0}
					<tr>
						<td colspan={columns.length + (selectable ? 1 : 0)} class="px-4 py-12">
							<EmptyState title="No data" description="No records found." />
						</td>
					</tr>
				{:else}
					{#each data as row, index (row[rowKey] ?? index)}
						{@const selected = isRowSelected(row)}
						<tr
							class="
								bg-[var(--bg-secondary)] transition-colors
								{selected ? 'bg-primary-50 dark:bg-primary-900/10' : 'hover:bg-[var(--bg-hover)]'}
								{onRowClick ? 'cursor-pointer' : ''}
							"
							onclick={() => onRowClick?.(row)}
						>
							{#if selectable}
								<td class="px-4 py-3">
									<button
										type="button"
										class="
											flex h-5 w-5 items-center justify-center rounded border
											transition-colors
											{selected
												? 'border-primary-600 bg-primary-600 text-white'
												: 'border-secondary-300 bg-white dark:border-secondary-600 dark:bg-secondary-800'}
										"
										onclick={(e) => {
											e.stopPropagation();
											handleSelectRow(row);
										}}
										aria-label={selected ? 'Deselect row' : 'Select row'}
									>
										{#if selected}
											<Check class="h-3.5 w-3.5" />
										{/if}
									</button>
								</td>
							{/if}

							{#each columns as column (column.key)}
								{@const value = getNestedValue(row, String(column.key))}
								<td
									class="px-4 py-3 text-[var(--text-primary)] {alignClasses[column.align ?? 'left']}"
								>
									{#if column.render}
										{@html column.render(value, row)}
									{:else}
										{value ?? '—'}
									{/if}
								</td>
							{/each}
						</tr>
					{/each}
				{/if}
			</tbody>
		</table>
	</div>
</div>
