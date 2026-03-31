import type { Handle } from '@sveltejs/kit';

export const handle: Handle = async ({ event, resolve }) => {
	const token = event.cookies.get('auth_token');

	if (token) {
		// TODO: Validate token with backend and extract user info
		// For now, we just check if a token exists
		// In production, verify JWT signature and expiration
		try {
			// Placeholder: decode token and set user
			// const user = await validateToken(token);
			// event.locals.user = user;
			event.locals.user = undefined;
		} catch {
			// Invalid token - clear it
			event.cookies.delete('auth_token', { path: '/' });
		}
	}

	const response = await resolve(event, {
		transformPageChunk: ({ html }) => {
			// Inject theme class based on cookie or default
			const theme = event.cookies.get('theme') || 'light';
			return html.replace('<html lang="en"', `<html lang="en" class="${theme === 'dark' ? 'dark' : ''}"`);
		}
	});

	return response;
};
