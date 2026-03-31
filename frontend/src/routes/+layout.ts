import type { LayoutLoad } from './$types';

export const load: LayoutLoad = async ({ url }) => {
	return {
		pathname: url.pathname
	};
};

export const prerender = false;
export const ssr = true;
