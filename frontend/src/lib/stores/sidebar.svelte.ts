import { browser } from '$app/environment';

const STORAGE_KEY = 'sidebar_collapsed';
const MOBILE_BREAKPOINT = 1024;

function getStoredState(): boolean {
	if (!browser) return false;
	return localStorage.getItem(STORAGE_KEY) === 'true';
}

function isMobile(): boolean {
	if (!browser) return false;
	return window.innerWidth < MOBILE_BREAKPOINT;
}

class SidebarStore {
	#collapsed = $state<boolean>(getStoredState());
	#mobileOpen = $state<boolean>(false);
	#isMobile = $state<boolean>(isMobile());

	constructor() {
		if (browser) {
			// Listen for resize events
			window.addEventListener('resize', () => {
				this.#isMobile = isMobile();
				// Auto-close mobile sidebar on resize to desktop
				if (!this.#isMobile) {
					this.#mobileOpen = false;
				}
			});
		}
	}

	get collapsed(): boolean {
		return this.#collapsed;
	}

	get mobileOpen(): boolean {
		return this.#mobileOpen;
	}

	get isMobile(): boolean {
		return this.#isMobile;
	}

	get isVisible(): boolean {
		if (this.#isMobile) {
			return this.#mobileOpen;
		}
		return !this.#collapsed;
	}

	toggle(): void {
		if (this.#isMobile) {
			this.#mobileOpen = !this.#mobileOpen;
		} else {
			this.#collapsed = !this.#collapsed;
			if (browser) {
				localStorage.setItem(STORAGE_KEY, String(this.#collapsed));
			}
		}
	}

	collapse(): void {
		this.#collapsed = true;
		if (browser) {
			localStorage.setItem(STORAGE_KEY, 'true');
		}
	}

	expand(): void {
		this.#collapsed = false;
		if (browser) {
			localStorage.setItem(STORAGE_KEY, 'false');
		}
	}

	openMobile(): void {
		this.#mobileOpen = true;
	}

	closeMobile(): void {
		this.#mobileOpen = false;
	}
}

export const sidebar = new SidebarStore();
