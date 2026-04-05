/**
 * Normalize CycloneDX / SPDX JSON SBOM documents for UI tables.
 * Treat document payloads as untrusted data: defensive checks only, no code execution.
 */

export type SbomDocumentKind = 'cyclonedx' | 'spdx' | 'json';

/** Row shape aligned with `SbomPackage` in the viewer. */
export interface SbomPackageRow {
	name: string;
	version: string;
	license?: string;
	ecosystem: string;
	direct: boolean;
}

export interface ParsedSbom {
	kind: SbomDocumentKind;
	/** Display label aligned with API (`cyclonedx`, `spdx`, `json`). */
	formatLabel: string;
	specVersion?: string;
	rootName?: string;
	rootVersion?: string;
	componentCount: number;
	packages: SbomPackageRow[];
}

function asRecord(v: unknown): Record<string, unknown> | null {
	if (v !== null && typeof v === 'object' && !Array.isArray(v)) {
		return v as Record<string, unknown>;
	}
	return null;
}

function asString(v: unknown): string | undefined {
	return typeof v === 'string' && v.length > 0 ? v : undefined;
}

function ecosystemFromPurl(purl?: string): string {
	if (!purl) return 'unknown';
	const m = /^pkg:([^/]+)\//i.exec(purl.trim());
	return m ? m[1].toLowerCase() : 'unknown';
}

function cycloneLicense(comp: Record<string, unknown>): string | undefined {
	const licenses = comp['licenses'];
	if (!Array.isArray(licenses) || licenses.length === 0) return undefined;
	const first = asRecord(licenses[0]);
	if (!first) return undefined;
	const lic = asRecord(first['license']);
	if (lic) {
		const id = asString(lic['id']);
		if (id) return id;
		const name = asString(lic['name']);
		if (name) return name;
	}
	return asString(first['expression']);
}

function spdxLicense(pkg: Record<string, unknown>): string | undefined {
	const concluded = asString(pkg['licenseConcluded']);
	if (concluded && concluded !== 'NOASSERTION' && concluded !== 'NONE') {
		return concluded;
	}
	const declared = asString(pkg['licenseDeclared']);
	if (declared && declared !== 'NOASSERTION' && declared !== 'NONE') {
		return declared;
	}
	return undefined;
}

function spdxPurl(pkg: Record<string, unknown>): string | undefined {
	const refs = pkg['externalRefs'];
	if (!Array.isArray(refs)) return undefined;
	for (const r of refs) {
		const ref = asRecord(r);
		if (!ref) continue;
		const t = asString(ref['referenceType'])?.toLowerCase();
		const loc = asString(ref['referenceLocator']);
		if (t === 'purl' && loc) return loc;
	}
	return undefined;
}

function parseCycloneDx(doc: Record<string, unknown>): ParsedSbom {
	const specVersion = asString(doc['specVersion']);
	const meta = asRecord(doc['metadata']);
	let rootName: string | undefined;
	let rootVersion: string | undefined;
	let rootRef: string | undefined;

	if (meta) {
		const root = asRecord(meta['component']);
		if (root) {
			rootName = asString(root['name']);
			rootVersion = asString(root['version']);
			rootRef = asString(root['bom-ref']);
		}
	}

	const rawComponents = doc['components'];
	const components: Record<string, unknown>[] = Array.isArray(rawComponents)
		? rawComponents.map((c) => asRecord(c)).filter((c): c is Record<string, unknown> => c !== null)
		: [];

	const directRefs = new Set<string>();
	const deps = doc['dependencies'];
	if (Array.isArray(deps) && rootRef) {
		for (const d of deps) {
			const dr = asRecord(d);
			if (!dr) continue;
			if (asString(dr['ref']) === rootRef) {
				const dependsOn = dr['dependsOn'];
				if (Array.isArray(dependsOn)) {
					for (const x of dependsOn) {
						if (typeof x === 'string') directRefs.add(x);
					}
				}
			}
		}
	}

	const packages: SbomPackageRow[] = [];
	for (const c of components) {
		const name = asString(c['name']) ?? '(unnamed)';
		const version = asString(c['version']) ?? '—';
		const purl = asString(c['purl']);
		const bomRef = asString(c['bom-ref']);
		const ecosystem = ecosystemFromPurl(purl);
		const license = cycloneLicense(c);
		let direct = false;
		if (directRefs.size > 0 && bomRef) {
			direct = directRefs.has(bomRef);
		} else if (directRefs.size === 0) {
			direct = true;
		}
		packages.push({ name, version, license, ecosystem, direct });
	}

	return {
		kind: 'cyclonedx',
		formatLabel: 'cyclonedx',
		specVersion,
		rootName,
		rootVersion,
		componentCount: packages.length,
		packages
	};
}

function parseSpdx(doc: Record<string, unknown>): ParsedSbom {
	const specVersion = asString(doc['spdxVersion']);
	const rootName = asString(doc['name']);
	const pkgsRaw = doc['packages'];
	const pkgList: Record<string, unknown>[] = Array.isArray(pkgsRaw)
		? pkgsRaw.map((p) => asRecord(p)).filter((p): p is Record<string, unknown> => p !== null)
		: [];

	const packages: SbomPackageRow[] = [];
	for (const p of pkgList) {
		const name = asString(p['name']) ?? '(unnamed package)';
		const version = asString(p['versionInfo']) ?? '—';
		const purl = spdxPurl(p);
		const ecosystem = ecosystemFromPurl(purl);
		packages.push({
			name,
			version,
			license: spdxLicense(p),
			ecosystem,
			direct: true
		});
	}

	return {
		kind: 'spdx',
		formatLabel: 'spdx',
		specVersion,
		rootName,
		rootVersion: undefined,
		componentCount: packages.length,
		packages
	};
}

/** Infer SBOM format from JSON shape (matches backend heuristics). */
export function detectSbomKind(doc: Record<string, unknown>): SbomDocumentKind {
	const bf = doc['bomFormat'];
	if (typeof bf === 'string' && bf.length > 0) return 'cyclonedx';
	const sv = doc['spdxVersion'];
	if (typeof sv === 'string' && sv.length > 0) return 'spdx';
	return 'json';
}

export function parseSbomDocument(doc: Record<string, unknown>): ParsedSbom {
	const kind = detectSbomKind(doc);
	if (kind === 'cyclonedx') return parseCycloneDx(doc);
	if (kind === 'spdx') return parseSpdx(doc);
	return {
		kind: 'json',
		formatLabel: 'json',
		specVersion: undefined,
		rootName: undefined,
		rootVersion: undefined,
		componentCount: 0,
		packages: []
	};
}

/** Default download filename for an SBOM blob. */
export function sbomExportFilename(kind: SbomDocumentKind, runId?: string): string {
	const prefix = runId?.trim() ? `sbom-${runId.trim()}` : 'sbom';
	if (kind === 'cyclonedx') return `${prefix}.cdx.json`;
	if (kind === 'spdx') return `${prefix}.spdx.json`;
	return `${prefix}.json`;
}
