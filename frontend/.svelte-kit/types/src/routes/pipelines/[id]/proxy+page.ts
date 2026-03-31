// @ts-nocheck
import type { PageLoad } from './$types';

export const load = ({ params }: Parameters<PageLoad>[0]) => {
	return {
		title: 'Pipeline',
		breadcrumbs: [
			{ label: 'Pipelines', href: '/pipelines' },
			{ label: 'Pipeline Details', href: `/pipelines/${params.id}` }
		]
	};
};
