// @ts-nocheck
import type { LayoutLoad } from './$types';

export const load = async ({ url }: Parameters<LayoutLoad>[0]) => {
	return {
		pathname: url.pathname
	};
};

export const prerender = false;
export const ssr = true;
