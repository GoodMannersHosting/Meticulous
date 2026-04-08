import { browser } from '$app/environment';
import { PUBLIC_API_URL } from '$env/static/public';

/**
 * Public HTTP base for the Meticulous API (no trailing slash).
 *
 * When `PUBLIC_API_URL` is empty at build time, the browser uses `window.location.origin`
 * so a single public hostname can serve both the Svelte app (`/`) and the API (`/auth`, `/api`, …)
 * without rebaking the image.
 */
export function getPublicApiBase(): string {
	const configured = (PUBLIC_API_URL ?? '').trim().replace(/\/$/, '');
	if (configured.length > 0) {
		return configured;
	}
	if (browser && typeof window !== 'undefined') {
		return window.location.origin;
	}
	return '';
}

/** SSR / hooks: use configured API URL, or the incoming request origin for same-host gateways. */
export function getPublicApiBaseFromRequestOrigin(requestOrigin: string): string {
	const configured = (PUBLIC_API_URL ?? '').trim().replace(/\/$/, '');
	if (configured.length > 0) {
		return configured;
	}
	return requestOrigin.replace(/\/$/, '');
}
