// @ts-nocheck
import type { PageLoad } from './$types';

export const load = ({ params }: Parameters<PageLoad>[0]) => {
	return {
		title: 'Project',
		breadcrumbs: [
			{ label: 'Projects', href: '/projects' },
			{ label: 'Project Details', href: `/projects/${params.id}` }
		]
	};
};
