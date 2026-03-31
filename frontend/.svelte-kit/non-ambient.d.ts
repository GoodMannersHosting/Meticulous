
// this file is generated — do not edit it


declare module "svelte/elements" {
	export interface HTMLAttributes<T> {
		'data-sveltekit-keepfocus'?: true | '' | 'off' | undefined | null;
		'data-sveltekit-noscroll'?: true | '' | 'off' | undefined | null;
		'data-sveltekit-preload-code'?:
			| true
			| ''
			| 'eager'
			| 'viewport'
			| 'hover'
			| 'tap'
			| 'off'
			| undefined
			| null;
		'data-sveltekit-preload-data'?: true | '' | 'hover' | 'tap' | 'off' | undefined | null;
		'data-sveltekit-reload'?: true | '' | 'off' | undefined | null;
		'data-sveltekit-replacestate'?: true | '' | 'off' | undefined | null;
	}
}

export {};


declare module "$app/types" {
	type MatcherParam<M> = M extends (param : string) => param is (infer U extends string) ? U : string;

	export interface AppTypes {
		RouteId(): "/" | "/agents" | "/auth" | "/auth/callback" | "/auth/login" | "/dashboard" | "/jobs" | "/pipelines" | "/pipelines/new" | "/pipelines/[id]" | "/projects" | "/projects/[id]" | "/runs" | "/runs/[id]" | "/settings" | "/settings/security" | "/workflows";
		RouteParams(): {
			"/pipelines/[id]": { id: string };
			"/projects/[id]": { id: string };
			"/runs/[id]": { id: string }
		};
		LayoutParams(): {
			"/": { id?: string };
			"/agents": Record<string, never>;
			"/auth": Record<string, never>;
			"/auth/callback": Record<string, never>;
			"/auth/login": Record<string, never>;
			"/dashboard": Record<string, never>;
			"/jobs": Record<string, never>;
			"/pipelines": { id?: string };
			"/pipelines/new": Record<string, never>;
			"/pipelines/[id]": { id: string };
			"/projects": { id?: string };
			"/projects/[id]": { id: string };
			"/runs": { id?: string };
			"/runs/[id]": { id: string };
			"/settings": Record<string, never>;
			"/settings/security": Record<string, never>;
			"/workflows": Record<string, never>
		};
		Pathname(): "/" | "/agents" | "/auth/callback" | "/auth/login" | "/dashboard" | "/jobs" | "/pipelines" | "/pipelines/new" | `/pipelines/${string}` & {} | "/projects" | `/projects/${string}` & {} | "/runs" | `/runs/${string}` & {} | "/settings" | "/settings/security" | "/workflows";
		ResolvedPathname(): `${"" | `/${string}`}${ReturnType<AppTypes['Pathname']>}`;
		Asset(): "/favicon.svg" | string & {};
	}
}