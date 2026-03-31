// @ts-nocheck
import { browser } from '$app/environment';
import { error, redirect } from '@sveltejs/kit';
import type { PageLoad } from './$types';

export const load = async ({ url }: Parameters<PageLoad>[0]) => {
	if (!browser) {
		return {};
	}

	const code = url.searchParams.get('code');
	const state = url.searchParams.get('state');
	const errorParam = url.searchParams.get('error');
	const errorDescription = url.searchParams.get('error_description');

	if (errorParam) {
		throw error(400, {
			message: errorDescription || errorParam
		});
	}

	if (!code || !state) {
		throw error(400, {
			message: 'Missing authorization code or state parameter'
		});
	}

	return {
		code,
		state,
		provider: 'github'
	};
};
