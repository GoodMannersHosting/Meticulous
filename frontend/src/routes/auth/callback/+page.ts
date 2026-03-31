import { browser } from '$app/environment';
import { error } from '@sveltejs/kit';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ url }) => {
	if (!browser) {
		return {};
	}

	const errorParam = url.searchParams.get('error');
	const errorDescription = url.searchParams.get('error_description');

	if (errorParam) {
		throw error(400, {
			message: errorDescription || errorParam
		});
	}

	// The backend sends back a JWT token directly (it already exchanged the OAuth code)
	const token = url.searchParams.get('token');
	const tokenType = url.searchParams.get('token_type');

	if (!token) {
		throw error(400, {
			message: 'Missing authentication token'
		});
	}

	return {
		token,
		tokenType: tokenType || 'Bearer'
	};
};
