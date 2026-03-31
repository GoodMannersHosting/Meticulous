import type { PageLoad } from './$types';

export const load: PageLoad = async ({ params }) => {
	return {
		title: 'User Profile',
		userId: params.id,
		breadcrumbs: [
			{ label: 'Admin', href: '/admin' },
			{ label: 'Users', href: '/admin/users' },
			{ label: 'Profile', href: `/admin/users/${params.id}` }
		]
	};
};
