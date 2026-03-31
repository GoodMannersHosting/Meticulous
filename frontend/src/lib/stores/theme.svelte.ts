import { browser } from '$app/environment';

export type Theme = 'light' | 'dark' | 'system';

function getSystemTheme(): 'light' | 'dark' {
	if (!browser) return 'light';
	return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
}

function getStoredTheme(): Theme {
	if (!browser) return 'system';
	return (localStorage.getItem('theme') as Theme) || 'system';
}

function getEffectiveTheme(theme: Theme): 'light' | 'dark' {
	if (theme === 'system') {
		return getSystemTheme();
	}
	return theme;
}

function applyTheme(theme: 'light' | 'dark'): void {
	if (!browser) return;

	const root = document.documentElement;
	if (theme === 'dark') {
		root.classList.add('dark');
	} else {
		root.classList.remove('dark');
	}

	// Also set cookie for SSR
	document.cookie = `theme=${theme};path=/;max-age=31536000;samesite=lax`;
}

class ThemeStore {
	#preference = $state<Theme>(getStoredTheme());
	#effective = $state<'light' | 'dark'>(getEffectiveTheme(this.#preference));

	constructor() {
		if (browser) {
			// Apply initial theme
			applyTheme(this.#effective);

			// Listen for system theme changes
			const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)');
			mediaQuery.addEventListener('change', () => {
				if (this.#preference === 'system') {
					this.#effective = getSystemTheme();
					applyTheme(this.#effective);
				}
			});
		}
	}

	get preference(): Theme {
		return this.#preference;
	}

	get effective(): 'light' | 'dark' {
		return this.#effective;
	}

	get isDark(): boolean {
		return this.#effective === 'dark';
	}

	set(theme: Theme): void {
		this.#preference = theme;
		this.#effective = getEffectiveTheme(theme);

		if (browser) {
			localStorage.setItem('theme', theme);
			applyTheme(this.#effective);
		}
	}

	toggle(): void {
		const newTheme = this.#effective === 'dark' ? 'light' : 'dark';
		this.set(newTheme);
	}
}

export const theme = new ThemeStore();
