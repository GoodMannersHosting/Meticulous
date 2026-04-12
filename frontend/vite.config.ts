import { sveltekit } from '@sveltejs/kit/vite';
import tailwindcss from '@tailwindcss/vite';
import { defineConfig, loadEnv } from 'vite';

export default defineConfig(({ mode }) => {
	const env = loadEnv(mode, process.cwd(), '');
	const apiTarget = (env.PUBLIC_API_URL || 'http://127.0.0.1:8080').replace(/\/$/, '');

	return {
		plugins: [tailwindcss(), sveltekit()],
		server: {
			port: 5173,
			strictPort: true,
			// Defense-in-depth against CVE-2026-39363 (fetchModule WebSocket fs bypass) and
			// CVE-2025-31486 (server.fs.deny bypass). Vite 6.4.2+ patches the root cause, but
			// these settings close the attack surface independently of the version guard:
			//   - fs.strict: reject any request outside the allowed roots at the fs layer
			//   - fs.allow: explicit allowlist (project root only)
			//   - cors: restrict the WebSocket upgrade to same-origin; blocks the no-Origin PoC
			fs: {
				strict: true,
				allow: ['.']
			},
			cors: {
				origin: false
			},
			// met-api serves `/.well-known/*` (OIDC discovery, JWKS). Without a proxy, the dev UI
			// origin (5173) would 404; rely on the same base as `PUBLIC_API_URL`.
			proxy: {
				'/.well-known': {
					target: apiTarget,
					changeOrigin: true
				}
			}
		}
	};
});
