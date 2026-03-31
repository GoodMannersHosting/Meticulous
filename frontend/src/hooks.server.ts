import type { Handle } from '@sveltejs/kit';
import { PUBLIC_API_URL } from '$env/static/public';

interface MeResponse {
	id: string;
	email: string;
	name: string;
	org_id: string;
	role: string;
	created_at: string;
}

async function validateToken(token: string, fetch: typeof globalThis.fetch): Promise<MeResponse | null> {
	const apiUrl = PUBLIC_API_URL || 'http://localhost:8080';
	
	try {
		const response = await fetch(`${apiUrl}/auth/me`, {
			headers: {
				'Authorization': `Bearer ${token}`,
				'Content-Type': 'application/json'
			}
		});

		if (!response.ok) {
			return null;
		}

		return await response.json();
	} catch {
		return null;
	}
}

export const handle: Handle = async ({ event, resolve }) => {
	const token = event.cookies.get('auth_token');

	if (token) {
		try {
			const userData = await validateToken(token, event.fetch);
			
			if (userData) {
				event.locals.user = {
					id: userData.id,
					name: userData.name,
					email: userData.email,
					avatar: undefined
				};
			} else {
				event.cookies.delete('auth_token', { path: '/' });
				event.locals.user = undefined;
			}
		} catch {
			event.cookies.delete('auth_token', { path: '/' });
			event.locals.user = undefined;
		}
	}

	const response = await resolve(event, {
		transformPageChunk: ({ html }) => {
			const theme = event.cookies.get('theme') || 'light';
			const wantDark = theme === 'dark';
			return html.replace(/(<html[^>]*\bclass=")([^"]*)(")/, (_, prefix, classes, suffix) => {
				const set = new Set(classes.split(/\s+/).filter(Boolean));
				if (wantDark) {
					set.add('dark');
				} else {
					set.delete('dark');
				}
				return `${prefix}${[...set].join(' ')}${suffix}`;
			});
		}
	});

	return response;
};
