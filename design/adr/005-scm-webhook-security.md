# ADR-005: SCM inbound webhook security

**Status:** Proposed  
**Date:** 2026-03-31  
**PRDs:** [020](../prd/020-scm-webhooks-triggers.md)

## Context

Inbound webhooks are implemented in [crates/met-api/src/routes/webhooks.rs](../../crates/met-api/src/routes/webhooks.rs). Secrets are loaded per `trigger_id` from `webhook_registrations.secret_hash` (column name is legacy; value is used as the **shared verifier** for HMAC or token equality, not as a one-way password hash today—see Consequences).

## Decision

1. **GitHub** — Require `X-Hub-Signature-256` when a verifier exists for the trigger. Verify with HMAC-SHA256 over the raw body; prefix `sha256=`; use **constant-time** comparison ([`verify_github_signature`](../../crates/met-api/src/routes/webhooks.rs)). Use `X-GitHub-Delivery` as the **dedupe key** stored in `webhook_deliveries(provider, delivery_id, received_at)`. GitHub webhooks do not embed a timestamp in the signature, so replay protection relies entirely on deduplication — the `webhook_deliveries` table must be implemented before production use. **Fork/PR trust classification:** the `pull_request` payload field `pull_request.head.repo.fork` (bool) and `pull_request.head.repo.full_name != pull_request.base.repo.full_name` must be parsed and stored as a `fork_pr` flag on the trigger event; see fork policy below.

2. **GitLab** — When a verifier exists, require `X-Gitlab-Token` and compare with **constant-time** equality to the stored verifier. GitLab has no delivery ID header; use `HMAC(verifier, raw_body)` as a synthetic dedup key combined with a **5-minute received-at window**: reject events where `received_at < now() - 300s` or `received_at > now() + 60s` (clock skew). Store in `webhook_deliveries`.

3. **Bitbucket** — **BLOCKED for production use** until HMAC-SHA256 verification is implemented (handler currently only loads the secret without verifying). Implementation must follow Atlassian's HMAC shared-secret spec. Apply the same 5-minute received-at window as GitLab. Treat as development-only until the implementation is merged and tested.

4. **Replay window** — **5 minutes** is the standard replay tolerance (aligned with Stripe, Svix, and major webhook platforms). 15 minutes is unnecessarily permissive. Where no timestamp is available in the provider signature (GitHub, GitLab), the `received_at` window applies to the event metadata field `pushedAt` / `created_at` from the payload body, not the HTTP request time, to avoid clock drift issues.

5. **Fork/PR policy** — Classify every `pull_request` webhook event into one of three trust tiers before dispatching:
   - `TRUSTED`: PR from same repo (non-fork) and author has write access.
   - `COLLABORATOR`: Fork PR by a user who is a verified collaborator/org member in the base org.
   - `EXTERNAL`: Fork PR from an unknown contributor.
   Default behavior: `TRUSTED` → full secrets; `COLLABORATOR` → project-configurable (require approval or restricted scope); `EXTERNAL` → no secrets, restricted environment, operator approval gate required. Pipeline definitions sourced from the fork branch must be validated against a `fork_pipeline_allowlist` that prohibits credential-bearing network egress. The current `GitHubPullRequest` parser must be extended with `fork: bool` and `author_association: String` fields.

6. **Generic** `/webhooks/{org}/{trigger}` — Treat as **development-only** unless augmented with a shared-secret header; do not expose on the public internet without auth.

7. **Production policy** — Every active production trigger for GitHub/GitLab MUST have a verifier configured. Deployment config MUST add `METICULOUS_WEBHOOK_REQUIRE_SECRET=true` enforcement; handlers without a configured verifier must return `403` in production mode, not silently accept.

8. **Mapping to runs** — After verification, normalize `(event_type, branch, commit_sha, fork_pr)` and resolve to pipeline triggers (engine). Idempotency for `run` creation uses `(provider, delivery_id)` per `webhook_deliveries` ([ADR-001](001-run-and-job-lifecycle.md)).

## webhook_deliveries table (required before production)

```sql
CREATE TABLE webhook_deliveries (
    id           uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    provider     text NOT NULL CHECK (provider IN ('github','gitlab','bitbucket','generic')),
    delivery_id  text NOT NULL,   -- X-GitHub-Delivery UUID, or HMAC(secret,body) for GitLab
    trigger_id   uuid REFERENCES webhook_registrations(id),
    received_at  timestamptz NOT NULL DEFAULT now(),
    run_id       uuid,            -- populated after successful run creation
    UNIQUE (provider, delivery_id)
);
CREATE INDEX ON webhook_deliveries (received_at);  -- for TTL purge job
```

Dedupe check: `INSERT ... ON CONFLICT (provider, delivery_id) DO NOTHING` returning the inserted row count; if 0, the delivery is a duplicate — return `200 OK` with no run created (GitHub retries expect 2xx). Purge rows older than 7 days (replay window is 5 minutes; 7-day retention is for audit purposes).

## SCM provider IP allowlisting (optional hardening)

GitHub, GitLab.com, and Atlassian publish their egress IP ranges (GitHub: `api.github.com/meta` → `hooks`; GitLab.com: their published ranges). For production deployments where webhook ingress is exposed to the internet, configure the reverse proxy to allowlist only SCM provider IP ranges for webhook routes. This is **optional** — it breaks self-hosted SCM instances with dynamic IPs — but strongly recommended for SaaS deployments targeting GitHub.com and GitLab.com only.

## Consequences

- Rename or retype `secret_hash` in a future migration to `webhook_secret` or encrypt at rest via column-level encryption; until then, treat row as sensitive.
- Rate limiting: 300 requests/minute per source IP at the reverse proxy; 60 requests/minute per `trigger_id` at Axum middleware. Body size cap: 25 MB (matches GitHub's maximum webhook payload).
- `METICULOUS_WEBHOOK_REQUIRE_SECRET=true` must be the default in production Helm values.

## Threat model

- **Assets:** Ability to enqueue runs, leak repo metadata from payloads.
- **Adversaries:** Anyone who can POST to the webhook URL; replay attacker with old signed payload.
- **Mitigations:** HMAC or token verification, TLS-only URLs, dedupe, short replay windows, fork/PR policy (PRD 020).
- **Residual risk:** Missing Bitbucket signature check until implemented; GitHub without configured secret accepts traffic.

## References

- GitHub: [Webhooks – validating payloads](https://docs.github.com/en/webhooks/using-webhooks/validating-webhook-deliverings)
- GitLab: [Webhooks – secret token](https://docs.gitlab.com/ee/user/project/integrations/webhooks.html#secret-token)
