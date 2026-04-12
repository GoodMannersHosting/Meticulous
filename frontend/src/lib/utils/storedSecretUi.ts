/** Kinds that store a provider reference in `metadata.secret_ref` (not opaque ciphertext). */
export const REMOTE_STORED_SECRET_KINDS = new Set([
	'aws_sm',
	'vault',
	'gcp_sm',
	'azure_kv',
	'kubernetes'
]);

export function isRemoteRefSecretKind(kind: string): boolean {
	return REMOTE_STORED_SECRET_KINDS.has(kind);
}

export function storedSecretValueFieldLabel(kind: string): string {
	switch (kind) {
		case 'aws_sm':
			return 'Secret ARN or name';
		case 'vault':
			return 'Vault secret path';
		case 'gcp_sm':
			return 'Secret resource name';
		case 'azure_kv':
			return 'Key Vault secret name or URI';
		case 'kubernetes':
			return 'Kubernetes secret reference';
		default:
			return 'Value (one-time)';
	}
}

export function storedSecretValuePlaceholder(kind: string): string {
	switch (kind) {
		case 'aws_sm':
			return 'e.g. arn:aws:secretsmanager:us-east-1:123456789012:secret:myapp/api';
		case 'vault':
			return 'e.g. kv/data/production/database';
		case 'gcp_sm':
			return 'projects/PROJECT_ID/secrets/SECRET_ID/versions/latest';
		case 'azure_kv':
			return 'e.g. https://myvault.vault.azure.net/secrets/my-secret';
		case 'kubernetes':
			return 'e.g. my-namespace/my-secret';
		default:
			return 'Secret value or PEM / JSON payload';
	}
}

export function storedSecretValueHelpLine(kind: string): string {
	if (isRemoteRefSecretKind(kind)) {
		return 'Enter the provider reference (ARN, path, or URI). This is not the secret payload; the control plane resolves it at runtime.';
	}
	return 'Stored encrypted; never shown again after save.';
}

export function getSecretRefFromMetadata(metadata: Record<string, unknown> | undefined): string | null {
	if (!metadata) return null;
	const r = metadata.secret_ref;
	return typeof r === 'string' && r.length > 0 ? r : null;
}

/** Respects `GET /api/v1/stored-secret-policy` (missing key or null policy = allowed). */
export function kindAllowedByExternalPolicy(
	kind: string,
	policy: Record<string, boolean> | null | undefined
): boolean {
	if (!policy || !isRemoteRefSecretKind(kind)) return true;
	return policy[kind] !== false;
}
