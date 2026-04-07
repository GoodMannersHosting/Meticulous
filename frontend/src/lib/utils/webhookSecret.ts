/**
 * 32-byte CSPRNG secret as lowercase hex (64 chars). Suitable for GitHub-style webhook HMAC.
 */
export function generateRandomWebhookSecret(): string {
	const c = globalThis.crypto;
	if (!c?.getRandomValues) {
		throw new Error('Secure random number generation is not available in this environment');
	}
	const bytes = new Uint8Array(32);
	c.getRandomValues(bytes);
	return Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join('');
}
