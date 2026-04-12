export function load() {
	return {
		title: 'Create API token',
		breadcrumbs: [
			{ label: 'Settings', href: '/settings' },
			{ label: 'Security', href: '/settings?tab=security' },
			{ label: 'New API token', href: '/settings/api-token/new' }
		]
	};
}
