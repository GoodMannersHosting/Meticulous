# ADR-011: Remote cache key derivation and tenant isolation

**Status:** Proposed
**Date:** 2026-03-31
**PRDs:** [050](../prd/050-agent-execution-logs-artifacts.md), [030](../prd/030-pipeline-authoring-dag-workflows.md)

## Context

PRD-050 FR-6 assigns remote cache *execution-plane behavior* to this ADR; PRD-030 owns the YAML surface (`cache:` block). ADR-001 notes remote caching as a future concern. Without explicit key derivation rules and tenant isolation, cache poisoning or cross-tenant key collisions are possible even with separate backing stores.

## Decision

### 1. Key structure

Cache keys are **opaque SHA-256 hex strings** derived from a structured input set. The canonical key derivation function is:

```
cache_key = SHA-256(
    "meticulous-cache-v1\n"          // domain-separation prefix
    + org_id + "\n"                  // tenant isolation (not slug — immutable UUID)
    + project_id + "\n"              // project scope
    + step_name + "\n"               // step identifier from pipeline YAML
    + platform_triple + "\n"         // e.g. "linux/amd64", "macos/arm64"
    + cache_version + "\n"           // operator-bumped version; resets cache on infra changes
    + user_key_expression            // pipeline author's `cache.key` expression (resolved)
)
```

The `user_key_expression` is the resolved string from the pipeline `cache:` block (e.g. `hashFiles('**/Cargo.lock')` result, branch name, etc.) and **must not contain secret material**. The pipeline linter (`met lint`) enforces this via the secret-hygiene rule category ([ADR-009](009-pipeline-linter-architecture.md)).

### 2. Tenant isolation

- `org_id` (UUID) is always the **first variable component** of the key — a collision in slugs or project names cannot bridge tenants.
- The cache storage backend uses **per-org prefixed paths** in object storage: `met-cache/{org_id}/{sha256_key}`. Even if a cache backend does not enforce access controls at the key level, path-prefix RBAC policies (S3 bucket policies) restrict cross-org reads.
- The controller validates `org_id` from the authenticated session before generating presigned PUT/GET URLs; agents never construct presigned URLs themselves.

### 3. Cache restore order and fallback

The pipeline YAML supports an ordered `restore-keys` list (same semantics as GitHub Actions cache). The agent tries keys in order, returning the first hit. Fallback keys **must** include `org_id` and `project_id` in their derivation — they cannot be global across tenants.

### 4. Secret-free keys

Cache key expressions in the YAML surface are evaluated in a **restricted environment** that has access to:
- File hash helpers: `hashFiles(glob)` → SHA-256 of matched file contents
- Branch/ref names from `github.ref` / `meticulous.ref`
- Explicit environment variables marked as non-secret (no `${{ secrets.* }}` interpolation)

If the linter detects a `secrets.*` context reference inside a `cache.key` expression, it fails the lint gate with error `SC-003: secret value in cache key expression`. This prevents secret material from appearing in cache keys, which are stored as object storage paths (not encrypted).

### 5. Cache invalidation

- **Explicit:** operator bumps `cache_version` in pool or project config; all existing keys become unreachable (keys are immutable; old objects expire via S3 lifecycle rules).
- **Automatic TTL:** cache entries expire after 7 days by default (configurable per project, max 90 days). Lifecycle rule applied to `met-cache/` prefix in the artifacts bucket (or a dedicated `met-cache` bucket if volume justifies separation).
- **Branch cleanup:** when a branch is deleted, the controller schedules a cache GC task that sets a 1-day expiry on all objects prefixed `met-cache/{org_id}/{project_id}/{branch_name_hash}/`. Full deletion is async; immediate invalidation is not guaranteed.

### 6. Storage backend

V1 uses S3-compatible object storage (same bucket group as artifacts, separate `met-cache/` prefix with its own lifecycle policy). A content-addressed key store (e.g., Redis or a dedicated cache service) is not required in v1 — object storage GET latency is acceptable for the restore-on-miss pattern. This decision is revisited if cache hit rate is low and restore overhead dominates step startup time.

### 7. Cache poisoning mitigations

- Agents have write access only to `met-cache/{org_id}/` via presigned PUT URLs with a 15-minute TTL and a maximum upload size of 5 GiB per entry (enforced by S3 `Content-Length` condition on the presigned URL).
- The controller records `{cache_key, uploader_agent_id, upload_time}` in Postgres for forensic tracing.
- Cache entries are **not** verified by the controller (no content hash check on restore) in v1. V2 may add a manifest hash stored in Postgres and verified by the agent before extraction.

## Consequences

- Pipeline engine must implement `hashFiles()` evaluation before dispatching a job (or delegate to agent on first use with a cache-miss path).
- `met lint` must add rule `SC-003` for secret references in cache key expressions.
- Object storage lifecycle policy must include a `met-cache/` rule.
- Cache GC task is a new background worker in the controller.

## Threat model

- **Assets:** Cached build artifacts that may contain pre-built binaries; cache keys as path metadata.
- **Adversaries:** Tenant A poisoning tenant B's cache; malicious cache entry substituting a trojan binary.
- **Mitigations:** Per-org prefix isolation; presigned PUT scope; audit log of cache writes. V2 manifest hash check addresses binary substitution.
- **Residual risk:** V1 lacks content verification on restore — a compromised agent within an org could poison that org's cache.

## References

- [PRD-050 FR-6](../prd/050-agent-execution-logs-artifacts.md)
- [PRD-030](../prd/030-pipeline-authoring-dag-workflows.md) (YAML `cache:` block)
- [ADR-009](009-pipeline-linter-architecture.md) (linter rule SC-003)
- [ADR-001](001-run-and-job-lifecycle.md) (remote cache future concern note)
