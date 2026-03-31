import type { PageLoad } from './$types';

export const load: PageLoad = ({ params }) => {
	return {
		title: 'Run Details',
		breadcrumbs: [
			{ label: 'Runs', href: '/runs' },
			{ label: 'Run Details', href: `/runs/${params.id}` }
		]
	};
};
