import type { PageLoad } from './$types';

export const load: PageLoad = ({ params }) => {
	return {
		title: 'Pipeline',
		breadcrumbs: [
			{ label: 'Pipelines', href: '/pipelines' },
			{ label: 'Pipeline Details', href: `/pipelines/${params.id}` }
		]
	};
};
