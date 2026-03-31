import type { LayoutLoad } from './$types';

export const load: LayoutLoad = async () => {
	return {
		title: 'Admin',
		isAdminSection: true,
		breadcrumbs: [{ label: 'Admin', href: '/admin' }]
	};
};
