// @ts-nocheck
import type { PageLoad } from './$types';

export const load = async () => {
	return {
		title: 'Dashboard',
		breadcrumbs: [{ label: 'Dashboard' }]
	};
};
;null as any as PageLoad;