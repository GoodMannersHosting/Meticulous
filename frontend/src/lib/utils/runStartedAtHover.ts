import type { Action } from 'svelte/action';

import { formatDateTime } from './format';

const SELECTOR = '.met-run-started-at';

/** After ~1s hover, sets `title` to the absolute start time parsed from `data-started-at`. */
export const runStartedAtHover: Action<HTMLElement> = (container) => {
	const cleanups = new Map<HTMLElement, () => void>();

	function bind(el: HTMLElement) {
		if (cleanups.has(el)) return;
		const iso = el.dataset.startedAt;
		if (!iso) return;

		let timer: ReturnType<typeof setTimeout> | undefined;

		const onEnter = () => {
			clearTimeout(timer);
			timer = setTimeout(() => {
				el.setAttribute('title', formatDateTime(iso));
			}, 1000);
		};

		const onLeave = () => {
			clearTimeout(timer);
			el.removeAttribute('title');
		};

		el.addEventListener('pointerenter', onEnter);
		el.addEventListener('pointerleave', onLeave);

		cleanups.set(el, () => {
			clearTimeout(timer);
			el.removeEventListener('pointerenter', onEnter);
			el.removeEventListener('pointerleave', onLeave);
			el.removeAttribute('title');
		});
	}

	function scan() {
		const current = new Set(
			Array.from(container.querySelectorAll<HTMLElement>(SELECTOR))
		);
		for (const el of current) bind(el);
		for (const [el, cleanup] of [...cleanups]) {
			if (!current.has(el)) {
				cleanup();
				cleanups.delete(el);
			}
		}
	}

	scan();
	const mo = new MutationObserver(scan);
	mo.observe(container, { childList: true, subtree: true });

	return {
		destroy() {
			mo.disconnect();
			for (const cleanup of cleanups.values()) cleanup();
			cleanups.clear();
		}
	};
};
