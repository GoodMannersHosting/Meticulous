/** Resolved reusable workflow ref string: `scope/name@version` when shape matches. */
export function catalogWorkflowRef(
	sw: Record<string, unknown> | undefined | null
): string | null {
	if (!sw || typeof sw !== 'object') return null;
	const scope = sw['scope'];
	const name = sw['name'];
	const version = sw['version'];
	if (typeof scope === 'string' && typeof name === 'string' && typeof version === 'string') {
		return `${scope}/${name}@${version}`;
	}
	return null;
}
