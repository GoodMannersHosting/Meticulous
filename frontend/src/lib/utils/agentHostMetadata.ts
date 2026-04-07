/** UI labels for host metadata keys (API/JSON keys unchanged). Matches controller `security_bundle_to_json`. */
export const HOST_METADATA_KEY_LABELS: Record<string, string> = {
	hostname: 'Hostname',
	os: 'Operating system',
	arch: 'Architecture',
	kernel_version: 'Kernel version',
	public_ips: 'Public IP addresses',
	private_ips: 'Private IP addresses',
	ntp_synchronized: 'NTP synchronized',
	container_runtime: 'Container runtime',
	container_runtime_version: 'Container runtime version',
	environment_type: 'Environment',
	agent_x509_public_key_hex: 'Agent public key (hex)',
	machine_id: 'Machine Identifier',
	logical_cpus: 'CPU Cores',
	memory_total_bytes: 'Memory (GB)',
	egress_public_ip: 'Egress public IP',
	kubernetes_pod_uid: 'Kubernetes pod UID',
	kubernetes_namespace: 'Kubernetes namespace',
	kubernetes_node_name: 'Kubernetes node'
};

export const ENVIRONMENT_TYPE_LABELS: Record<string, string> = {
	ENVIRONMENT_TYPE_UNSPECIFIED: 'Unspecified',
	ENVIRONMENT_TYPE_PHYSICAL: 'Physical',
	ENVIRONMENT_TYPE_VIRTUAL: 'Virtual',
	ENVIRONMENT_TYPE_CONTAINER: 'Container'
};

function humanizeMetadataKey(key: string): string {
	return key
		.split('_')
		.filter(Boolean)
		.map((w) => w.charAt(0).toUpperCase() + w.slice(1).toLowerCase())
		.join(' ');
}

export function hostMetadataRowLabel(key: string): string {
	return HOST_METADATA_KEY_LABELS[key] ?? humanizeMetadataKey(key);
}

export function hostMetadataValueCellClass(key: string): string {
	return key === 'agent_x509_public_key_hex'
		? 'whitespace-pre-wrap break-all px-3 py-2 text-[var(--text-primary)] align-top font-mono text-xs sm:text-sm'
		: 'whitespace-pre-wrap break-all px-3 py-2 text-[var(--text-primary)] align-top text-sm';
}

export function hostMetadataDisplayValue(key: string, val: unknown): string {
	if (key === 'memory_total_bytes') {
		const n = typeof val === 'number' ? val : Number(val);
		if (Number.isFinite(n)) {
			return (n / 1024 ** 3).toFixed(2);
		}
	}
	if (key === 'ntp_synchronized') {
		if (val === true || val === 'true') return 'Yes';
		if (val === false || val === 'false') return 'No';
	}
	if (key === 'environment_type') {
		if (typeof val === 'number' && Number.isInteger(val) && val >= 0 && val <= 3) {
			return ['Unspecified', 'Physical', 'Virtual', 'Container'][val] ?? String(val);
		}
		const s = String(val);
		if (ENVIRONMENT_TYPE_LABELS[s]) return ENVIRONMENT_TYPE_LABELS[s];
		if (s.startsWith('ENVIRONMENT_TYPE_')) {
			return s
				.replace(/^ENVIRONMENT_TYPE_/, '')
				.split('_')
				.filter(Boolean)
				.map((w) => w.charAt(0).toUpperCase() + w.slice(1).toLowerCase())
				.join(' ');
		}
	}
	return Array.isArray(val)
		? val.join(', ')
		: val != null && typeof val === 'object'
			? JSON.stringify(val)
			: String(val);
}

export function filterHostMetadataEntries(
	bundle: Record<string, unknown> | null | undefined,
	filter: string
): [string, unknown][] {
	if (!bundle || typeof bundle !== 'object') return [];
	const q = filter.trim().toLowerCase();
	return Object.entries(bundle).filter(([key, val]) => {
		if (!q) return true;
		const label = hostMetadataRowLabel(key).toLowerCase();
		const str = hostMetadataDisplayValue(key, val).toLowerCase();
		return key.toLowerCase().includes(q) || label.includes(q) || str.includes(q);
	});
}
