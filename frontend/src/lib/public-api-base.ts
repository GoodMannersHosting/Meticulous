import { browser } from '$app/environment';
import { PUBLIC_API_URL } from '$env/static/public';

function hostnameIsLoopback(hostname: string): boolean {
	return hostname === 'localhost' || hostname === '127.0.0.1' || hostname === '[::1]';
}

/**
 * If the image was built with `PUBLIC_API_URL` pointing at loopback (Dockerfile / CI default)
 * but the UI is served from a real hostname, use the page origin so path-based gateways work.
 */
export function resolveConfiguredApiBaseAgainstPageOrigin(
	configuredRaw: string,
	pageOrigin: string,
): string {
	const configured = configuredRaw.trim().replace(/\/$/, '');
	const page = pageOrigin.trim().replace(/\/$/, '');
	if (!configured) {
		return page;
	}
	try {
		const api = new URL(configured);
		const pg = new URL(page);
		const apiLoop = hostnameIsLoopback(api.hostname);
		const pageLoop = hostnameIsLoopback(pg.hostname);
		if (apiLoop && !pageLoop) {
			return pg.origin;
		}
	} catch {
		/* invalid URL: fall through */
	}
	return configured;
}

/**
 * Public HTTP base for the Meticulous API (no trailing slash).
 *
 * Empty `PUBLIC_API_URL` → browser uses `window.location.origin` (same host as `/auth`, `/api`, …).
 * Loopback `PUBLIC_API_URL` + public page → use page origin (fixes images built without correct `--build-arg`).
 */
export function getPublicApiBase(): string {
	const raw = (PUBLIC_API_URL ?? '').trim();
	if (!raw) {
		if (browser && typeof window !== 'undefined') {
			return window.location.origin;
		}
		return '';
	}
	if (browser && typeof window !== 'undefined') {
		return resolveConfiguredApiBaseAgainstPageOrigin(raw, window.location.origin);
	}
	return raw.replace(/\/$/, '');
}

/** SSR / hooks: use configured API URL, or the incoming request origin for same-host gateways. */
export function getPublicApiBaseFromRequestOrigin(requestOrigin: string): string {
	const raw = (PUBLIC_API_URL ?? '').trim();
	if (!raw) {
		return requestOrigin.replace(/\/$/, '');
	}
	return resolveConfiguredApiBaseAgainstPageOrigin(raw, requestOrigin);
}
