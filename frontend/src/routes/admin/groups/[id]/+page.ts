import type { PageLoad } from './$types';

export const load: PageLoad = async ({ params }) => {
	return {
		title: 'Group',
		groupId: params.id,
		breadcrumbs: [
			{ label: 'Admin', href: '/admin' },
			{ label: 'Groups', href: '/admin/groups' },
			{ label: 'Group', href: `/admin/groups/${params.id}` }
		]
	};
};
