import type { PageLoad } from './$types';

export const load: PageLoad = ({ params }) => ({
	title: 'App',
	breadcrumbs: [
		{ label: 'Admin', href: '/admin' },
		{ label: 'Apps', href: '/admin/apps' },
		{ label: params.applicationId, href: `/admin/apps/${encodeURIComponent(params.applicationId)}` }
	]
});
