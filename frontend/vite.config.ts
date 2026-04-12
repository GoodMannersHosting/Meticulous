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
