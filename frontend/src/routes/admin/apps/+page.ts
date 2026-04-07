import type { PageLoad } from './$types';

export const load: PageLoad = () => ({
	title: 'Meticulous Apps',
	breadcrumbs: [{ label: 'Admin', href: '/admin' }, { label: 'Apps', href: '/admin/apps' }]
});
