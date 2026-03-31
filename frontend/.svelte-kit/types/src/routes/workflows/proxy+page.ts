// @ts-nocheck
import { redirect } from '@sveltejs/kit';
import type { PageLoad } from './$types';

export const load = () => {
	redirect(301, '/pipelines');
};
;null as any as PageLoad;