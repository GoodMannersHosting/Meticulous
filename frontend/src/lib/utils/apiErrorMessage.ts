/** Prefix returned by the API when YAML/JSON pipeline parsing fails. */
const INVALID_PIPELINE_PREFIX = 'invalid pipeline definition:';

/**
 * Turn a long semicolon-joined validation message into a title and bullet list.
 * Returns null if the message should be shown as plain text.
 */
export function parsePipelineDefinitionError(raw: string): {
	title: string;
	bullets: string[];
} | null {
	const t = raw.trim();
	if (!t.toLowerCase().startsWith(INVALID_PIPELINE_PREFIX)) return null;
	const body = t.slice(INVALID_PIPELINE_PREFIX.length).trim();
	const bullets = body
		.split('; ')
		.map((p) => p.trim())
		.filter(Boolean);
	if (bullets.length === 0) {
		return { title: 'Invalid pipeline definition', bullets: [t] };
	}
	return { title: 'Invalid pipeline definition', bullets };
}
