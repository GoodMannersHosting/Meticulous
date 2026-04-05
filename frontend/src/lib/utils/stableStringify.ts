/** Sort object keys recursively so JSON diffs are stable across runs. */
export function sortKeysDeep(value: unknown): unknown {
	if (value === null || typeof value !== 'object') {
		return value;
	}
	if (Array.isArray(value)) {
		return value.map(sortKeysDeep);
	}
	const obj = value as Record<string, unknown>;
	const out: Record<string, unknown> = {};
	for (const k of Object.keys(obj).sort()) {
		out[k] = sortKeysDeep(obj[k]);
	}
	return out;
}

export function stableStringify(value: unknown): string {
	return `${JSON.stringify(sortKeysDeep(value), null, 2)}\n`;
}
