import type { PageLoad } from './$types';

export const load: PageLoad = ({ params }) => {
	return {
		title: 'Project',
		breadcrumbs: [
			{ label: 'Projects', href: '/projects' },
			{ label: 'Project Details', href: `/projects/${params.id}` }
		]
	};
};
