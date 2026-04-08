import { writable } from 'svelte/store';

export type BreadcrumbItem = { label: string; href?: string };

/** When set (non-null), the top bar prefers this trail over static `+page.ts` crumbs. */
export const breadcrumbTrail = writable<BreadcrumbItem[] | null>(null);
