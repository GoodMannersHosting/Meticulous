import type { Pipeline } from '$api/types';
import type { SortDirection } from '$components/data/DataTable.svelte';

function comparePipelines(a: Pipeline, b: Pipeline, key: string): number {
	switch (key) {
		case 'name': {
			const c = a.name.toLowerCase().localeCompare(b.name.toLowerCase());
			if (c !== 0) return c;
			return a.slug.toLowerCase().localeCompare(b.slug.toLowerCase());
		}
		case 'description':
			return (a.description ?? '').toLowerCase().localeCompare((b.description ?? '').toLowerCase());
		case 'enabled':
			return Number(a.enabled) - Number(b.enabled);
		case 'updated_at':
			return a.updated_at.localeCompare(b.updated_at);
		default:
			return 0;
	}
}

export function sortPipelineList(
	pipelines: Pipeline[],
	sortKey: string | null,
	sortDirection: SortDirection | undefined
): Pipeline[] {
	if (!sortKey || !sortDirection) return pipelines;
	const out = [...pipelines];
	out.sort((a, b) => {
		let c = comparePipelines(a, b, sortKey);
		if (c === 0) {
			c = a.name.toLowerCase().localeCompare(b.name.toLowerCase());
			if (c === 0) c = a.id.localeCompare(b.id);
		}
		return sortDirection === 'asc' ? c : -c;
	});
	return out;
}
