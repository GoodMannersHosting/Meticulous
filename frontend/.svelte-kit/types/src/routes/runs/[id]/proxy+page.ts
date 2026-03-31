// @ts-nocheck
import type { PageLoad } from './$types';

export const load = ({ params }: Parameters<PageLoad>[0]) => {
	return {
		title: 'Run Details',
		breadcrumbs: [
			{ label: 'Runs', href: '/runs' },
			{ label: 'Run Details', href: `/runs/${params.id}` }
		]
	};
};
