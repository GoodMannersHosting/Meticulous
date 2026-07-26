# Meticulous vs GitHub Actions

GitHub Actions is an excellent product. Meticulous solves different problems — particularly for teams with **security, compliance, or infrastructure requirements** that GitHub-hosted runners can't satisfy.

---

## At a glance

| Dimension | GitHub Actions | Meticulous |
|-----------|---------------|------------|
| **Compute location** | GitHub-hosted runners (shared) or self-hosted runners | Self-hosted only; you own every agent host |
| **Network model** | Runners accept inbound connections (GitHub control) | Agents dial out only — no inbound attack surface |
| **Reusable units** | Actions referenced by Git repo + SHA/tag/branch | Versioned workflows in a platform catalog |
| **Supply chain** | Any public action from any GitHub repo | Admin-controlled workflow versions; no arbitrary external refs |
| **Secrets delivery** | Environment variables injected by runner | Per-job encrypted keypair; secrets decrypted in-memory, discarded after job |
| **Tenancy** | GitHub org/repo RBAC | Multi-org, multi-project RBAC with scoped tokens and audit log |
| **Triggers** | Push, PR, schedule, workflow_dispatch, etc. | Push, tag, PR, schedule, manual, webhook |
| **Artifact storage** | GitHub artifact store | Self-hosted S3-compatible; SBOM/provenance attestations |
| **On-prem / airgap** | Self-hosted runners (complex setup, inbound required) | Native; agents only need egress |

---

## Reusable units: actions vs workflows

### GitHub Actions approach

A GitHub Action is code in a Git repository, referenced by org/repo and a Git ref:

```yaml
steps:
  - uses: actions/checkout@v4           # SHA-pinned by Dependabot
  - uses: docker/build-push-action@v5   # tag — could change
  - uses: my-org/internal-action@main   # branch — always latest, risky
```

**Problems this creates:**

- Any action on `main` or a floating tag can change silently under your pipeline.
- SHA-pinning (via Dependabot) requires constant maintenance and doesn't prevent a compromised upstream from sneaking in a bad version before you pin.
- Actions can `require-secrets: true` and request arbitrary environment secrets — the runner injects them if the workflow grants them.
- Third-party actions run in your runner context with access to all secrets available to the job.

### Meticulous approach

Workflows are versioned in the platform catalog by an administrator:

```yaml
workflows:
  - workflow: global/docker-build
    version: v0.2        # pinned to a catalog version, not a Git ref
    inputs:
      image_tag: "${REGISTRY}/${IMAGE}"
```

- A developer cannot reference arbitrary external code — only catalog entries.
- Updating a workflow version is a platform-level change; a single diff in the catalog propagates to all pipelines using it.
- Supply chain auditing is centralized: one catalog, not hundreds of per-repo `uses:` references.

---

## Networking and runner security

### GitHub Actions

Self-hosted GitHub Actions runners must register with `github.com` and **accept webhook callbacks** from GitHub infrastructure. This requires an inbound connection (or a polling proxy) from GitHub's IP ranges, which must be allowed through your firewall.

Shared GitHub-hosted runners:

- Run in shared VMs; secrets are injected into environment variables visible to any step in the job.
- No guarantee of physical isolation between runs from different organizations.

### Meticulous

Agents establish an **outbound gRPC connection** to your controller. There is no inbound surface:

```
Agent (your network) ──outbound TCP──▶ met-controller (your cluster)
```

Agents can run in:

- Air-gapped environments with only egress allowed.
- Strict DMZs where inbound connections are prohibited.
- Kubernetes pods with a NetworkPolicy that only permits egress to the controller.

---

## Secrets model

GitHub Actions injects secrets as environment variables at job start. Any step — including any `uses:` action — can read all secrets available to the job via `process.env` or equivalent.

Meticulous delivers secrets using **per-job asymmetric encryption**:

1. Controller generates a one-time keypair per job.
2. API encrypts each secret value to the job public key.
3. Agent decrypts in memory; key material discarded after job completion.
4. No step or action reads the raw secret from an environment variable shared with the process tree.

External secret stores (AWS Secrets Manager, Vault, Kubernetes Secrets) are supported as first-class sources — in that case the secret value never enters the Meticulous control plane at all.
