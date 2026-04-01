# Open questions

Decisions still pending or needing a written ADR. Resolved direction from the master architecture plan is noted inline so this file stays actionable.

**Partial specs (see ADRs):** webhook verification and provider matrix — [adr/005-scm-webhook-security.md](adr/005-scm-webhook-security.md); executed binaries vs tool index — [adr/006-execution-telemetry-schema.md](adr/006-execution-telemetry-schema.md); OTel names and histogram buckets — [adr/007-observability-opentelemetry.md](adr/007-observability-opentelemetry.md); RBAC/token model and audit schema — [adr/008-tenancy-rbac-api-tokens.md](adr/008-tenancy-rbac-api-tokens.md); ops runbooks — [operations-and-reliability.md](operations-and-reliability.md).

**Resolved decisions** (ADRs/PRDs updated; no longer open):

- **mTLS for agent control plane** — Required in production; TLS-only only in dev. See [adr/003](adr/003-grpc-agent-control-plane.md).
- **Per-job crypto algorithms** — X25519 + AES-256-GCM (ChaCha20-Poly1305 fallback for non-AES-NI). See [adr/004](adr/004-secrets-and-per-job-pki.md).
- **Webhook replay window** — 5 minutes; Bitbucket blocked for production until HMAC implementation is complete. Fork/PR trust tier model (TRUSTED/COLLABORATOR/EXTERNAL) defined. See [adr/005](adr/005-scm-webhook-security.md).
- **eBPF / seccomp collection approach** — `SECCOMP_RET_USER_NOTIF` primary (Linux 5.0+, no elevated capabilities); eBPF tracepoint optional; polling fallback. See [adr/006](adr/006-execution-telemetry-schema.md).
- **NATS JetStream subject grammar** — Two-segment `meticulous.jobs.<org_slug>.<pool_id>`; no dot-encoded tags. DLQ pattern defined. See [adr/002](adr/002-nats-subjects-and-envelopes.md).
- **Token expiry defaults** — API tokens 90d; join tokens 7d default / 30d max; agent JWTs 24h/7d. Audit log schema OCSF-aligned JSON. See [adr/008](adr/008-tenancy-rbac-api-tokens.md).
- **Project ownership** — `project_members` join table, no `owner_user_id`, effective role = max(org default, explicit). See [adr/008](adr/008-tenancy-rbac-api-tokens.md) and Data model section below.
- **Secret scope** — Two-tier (org + project) in v1; `environment_id` reserved for v2. No global tier. See Data model section below.
- **SCM attachment** — `project_repos` join table, `fork_policy` enum, 1:1 in v1. See Data model section below.
- **Built-in secrets UX** — Shadow migration, 64-secret cap, UI warning, syntactic YAML separation. See Data model section below.
- **Executed binary path PII** — `PathRedactor` with `__user__` placeholder, applied agent-side before gRPC. See [adr/006](adr/006-execution-telemetry-schema.md).
- **Network metadata retention** — 30d hot / 90d pseudonymized warm / no cold by default. See Runtime section below.
- **ptrace policy** — containerd RuntimeDefault + drop CAP_SYS_PTRACE; opt-in debug steps. See Runtime section below.
- **Build log WORM** — Not required by SOC 2/ISO 27001; HMAC chain sufficient. Object Lock Governance on `met-audit` recommended. See [../operations-and-reliability.md](operations-and-reliability.md).
- **Log buffer sizing** — 512-line channel, 50–100 line batches / 100ms, drop-oldest, 50 MB WAL cap. See [../operations-and-reliability.md](operations-and-reliability.md).
- **SBOM lifecycle** — Separate `met-sboms` bucket, 3-year minimum, never shares artifact lifecycle. See [../operations-and-reliability.md](operations-and-reliability.md).
- **Debug CLI threat model** — Allowlist defined: logs, step status, env-names-only, local repro. No shell. `--no-secrets` is hardcoded behavior, not a flag. See [prd/130](prd/130-developer-debug-cli.md).
- **Pipeline linter architecture** — `met lint` (deterministic, blocking) separate from `met suggest` (AI, non-blocking). Minimum rule categories defined. See Pipeline quality section below.

## Execution backend

- **Tekton as execution backend?** — **Current direction: no** for the core engine; custom DAG/engine in-repo. Revisit only if a concrete gap (e.g. K8s-native step isolation) cannot be met otherwise.

## Data model and UX

- **Project owner** — **Resolved:** Do not put `owner_user_id` on `projects`. Use a `project_members(project_id, user_id, role enum(viewer|developer|maintainer|admin))` join table; the creator is seeded as `admin`. Effective role = `max(org_members.default_project_role, project_members.role)`. Add `org_members.default_project_role` (default `none`). Drop any existing `owner_user_id` column in the same migration that creates `project_members` — before any public API surface is released. This avoids a REST breaking change later and supports multi-owner with zero migration.

- **Project fields** — **Resolved:** `slug` (URL-safe, `[a-z0-9-]+`, unique per org, immutable after creation), `display_name` (non-unique, mutable), `description` (nullable), `visibility` (enum: private|internal, no public tier in v1), `archived` (bool, default false), `created_at`, `updated_at`. Uniqueness: `(org_id, slug)` unique constraint. Display-name collisions within an org are allowed but discouraged via UI warning.

- **Secret scope** — **Resolved (v1):** Two tiers: org-scoped and project-scoped. Project-scoped overrides org-scoped for the same name. Schema: `secrets(id, scope_type enum(org|project), scope_id uuid, name, provider, ...)`. Reserve `environment_id nullable uuid` column for v2 environment tier (staging/prod gate) — leave null in v1. No "global" admin tier; platform-wide shared secrets are modeled as org-scoped secrets in a designated platform org with restricted write access. Secret resolution: project first, then org fallback.

- **Built-in secrets** — **Resolved:** Support built-in storage but actively discourage production use. UX controls: (1) warning banner on creation ("Built-in secrets are suitable for development. Connect an external provider for production."); (2) built-in secrets visually badged in the UI; (3) hard cap of 64 built-in secrets per project to make built-in feel like a starter tier. **Migration path:** shadow migration — user creates a provider mapping (`secret name → Vault path`), control plane resolves from provider if mapping exists, falls back to built-in. Pipeline YAML is unchanged across the migration. Provider mapping replaces built-in row without re-keying any pipeline.

- **SCM attachment** — **Resolved:** Use a `project_repos` join table (not a column on `projects`). Schema: `project_repos(id, project_id, scm_provider enum(github|gitlab|bitbucket|gitea|plain_git), clone_url, ssh_url nullable, default_branch, clone_depth nullable int, fork_policy enum(block|no_secrets|allow_secrets), webhook_id nullable, created_at)`. In v1: unique constraint on `project_id` (one repo per project). In v2: drop constraint to support monorepos. `fork_policy` default: `no_secrets`. Store no credentials in this table — SCM provider auth lives in the secrets system. Path filtering for monorepos belongs in the pipeline trigger YAML (`paths:` field), not in the attachment row. `webhook_id` is the SCM-side webhook registration ID, used for de-registration on detach.

## Developer experience vs exfiltration

- **Debug CLI** — Can we offer a strong local/remote debug experience **without** making secret exfiltration easy? Needs threat model and capability allowlists. Working constraint: read-only log replay (no live secret injection), local repro using sanitized variable snapshots, and an explicit `--no-secrets` mode should define the allowlist floor.

## Runtime telemetry and hardening

- **Executed binary path PII** — **Resolved:** Apply canonical placeholder replacement in a `PathRedactor` struct on the agent side, before gRPC send (so redacted data never transits the wire). Rules: `/home/<username>/` → `/home/__user__/`, `/Users/<username>/` → `/Users/__user__/`, `/run/user/<uid>/` → `/run/user/__uid__/`, `/tmp/<username>-*` → `/tmp/__user__-<suffix>`. The SHA-256 of the binary content is the stable identity key for blast-radius queries; path is display metadata only. Opt-out via `telemetry.path_redaction: false` for single-tenant deployments where users have consented.

- **Network metadata retention** — **Resolved:** Tiered policy. Hot tier (Postgres, full src/dst IP): **30 days** default, operator-configurable per org. Warm tier (S3, pseudonymized — IPs replaced with `HMAC(org-secret, ip)`): **90 days**. No cold tier with raw IPs by default (GDPR Article 5(1)(e) storage limitation; 90 days is within accepted legitimate interest window). Pseudonymization preserves cardinality for anomaly queries without retaining identifying IPs. Purge job enforces per-org `retention_days` from `run_network_connections` table.

- **ptrace policy for containers** — **Resolved:** Use the containerd `RuntimeDefault` seccomp profile plus explicitly drop `CAP_SYS_PTRACE` from the container capability bounding set (this is already Docker's default, so "do nothing special" is correct). PID namespace isolation already blocks PTRACE_ATTACH across container boundaries at the kernel level — no extra seccomp rule needed for cross-namespace ptrace. Intra-container ptrace is blocked by capability drop. Impact on CI tools: none for standard build/test/coverage workflows; only `gdb`/`strace` attach-to-PID patterns are blocked. Opt-in for debug steps: add `CAP_SYS_PTRACE` only on steps with `debug: true`, logged as an audit event.

- **Tamper-evident logs** — **Resolved:** SOC 2 Type II and ISO 27001 A.8.15 do **not** require WORM for build logs — they require tamper-evidence. HMAC-SHA256 per `LogChunk` + S3 archival (without Object Lock) is sufficient for build logs. For `audit_log` (the compliance-critical table): Postgres append-only trigger is correct for primary storage; add **S3 Object Lock Governance mode** on the archived copy in production. Do not use Compliance mode by default — it prevents deletion of accidentally uploaded secrets for the full retention period. Build logs: Object Lock optional, Governance mode if used.

- **Log buffer sizing** — **Resolved:** Tokio bounded channel capacity **512 log lines** per step streaming task (~2–5s burst absorber at 200 lines/sec; ~32 KB memory). Batch 50–100 lines or flush every **100ms** (whichever first) before sending over gRPC. Overflow policy: **drop-oldest** with a `[N lines dropped due to backpressure]` sentinel (preserves recent context, most useful for debugging). Local WAL cap: **50 MB per job** on agent disk for NATS-disconnection recovery. Gzip log chunks at rest in the WAL.

- **SBOM vs artifact storage lifecycle** — **Resolved:** Separate buckets and separate lifecycle policies. SBOMs (50 KB–5 MB each) must outlive their corresponding build artifacts; they answer CVE blast-radius queries years after the artifact is gone. Artifact bucket (`met-artifacts`): 90-day default retention, Intelligent-Tiering after 30 days. SBOM bucket (`met-sboms`): **3-year minimum retention**, Glacier/IA after 180 days, soft-delete only (tombstone + operator confirmation). `sbom_reports` Postgres rows retained permanently (low cost); `sbom_components` rows (high cardinality) partitioned by `created_at` month and offloaded to a search index (e.g., OpenSearch) after 90 days for blast-radius queries. The `sbom_reports.s3_key` remains queryable even after the corresponding artifact is purged.

## Pipeline quality

- **Recommendations engine** — **Resolved:** Two separate tools, not one. `met lint`: deterministic rule engine, runs offline, no network calls, produces structured JSON output with rule ID / location / severity / remediation link. Blocking gate (can fail a pipeline). `met suggest`: AI-assisted layer that takes lint output + pipeline YAML as context and returns natural-language suggestions. Never auto-applies. Never blocks a pipeline. Clearly labeled as AI-generated. Rule linter rules are auditable by code review; AI suggestions are not — mixing trust levels in one binary is architecturally wrong.

  **Minimum lint rule categories:**
  - _Secret hygiene:_ regex for plaintext credentials in YAML; secrets passed as CLI args (ps-visible); base64-encoded values in `vars:` blocks; values matching known secret prefixes (`ghp_`, `AKIA`, `ya29.`) in non-secret fields.
  - _Fork/PR trust:_ warn if a step injects secrets in response to an `EXTERNAL`-tier trigger without an approval gate.
  - _Supply chain:_ workflow references pinned to branch names instead of commit SHAs; `curl | sh` patterns in run steps; container images without digest pins.
  - _Structural:_ `depends-on` cycle detection; unreachable jobs; missing required secret provider configuration.
  - _Security posture:_ `clone_depth: null` (full clone) when history is not needed; `fork_policy: allow_secrets` requires an explicit risk acknowledgment comment.

  AI hints are highest-value for supply chain best practices and posture improvements where the correct fix is context-dependent. Never use AI for secret hygiene or fork-trust rules where a wrong suggestion could introduce a real vulnerability.
