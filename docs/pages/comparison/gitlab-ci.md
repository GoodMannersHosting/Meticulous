# Meticulous vs GitLab CI

GitLab CI is mature, deeply integrated with GitLab SCM, and includes its own runner fleet. Meticulous is SCM-agnostic and built around a stricter security and networking model.

---

## At a glance

| Dimension | GitLab CI | Meticulous |
|-----------|-----------|------------|
| **SCM coupling** | Tightly integrated with GitLab (also works with external SCMs via mirrors) | SCM-agnostic; integrates with GitHub, GitLab, Bitbucket, or any webhook source |
| **Runner model** | GitLab Runner (shell, Docker, Kubernetes executor) | met-agent (Linux-native, container support) |
| **Runner networking** | Runners poll GitLab API (outbound) or use GitLab.com shared runners | Agents dial out to met-controller only — no GitLab dependency |
| **Pipeline definition** | `.gitlab-ci.yml` in repo root | `.stable/pipeline-*.yaml` files; can be project-external |
| **Reusable components** | CI components (catalog), `include:` from remote URLs | Versioned workflow catalog (global or project) |
| **Secrets** | CI/CD variables (env vars) | Per-job encrypted keypair; external store preferred |
| **Multi-tenancy** | GitLab groups/subgroups | Native multi-org, multi-project |
| **Self-hosted** | GitLab CE/EE + Runner | met-api + met-controller + agents |

---

## Reusable components

### GitLab CI

GitLab CI supports reusability via `include:` and the **CI/CD Component Catalog**. `include:` can pull templates from remote URLs or other repositories:

```yaml
include:
  - project: 'my-group/my-templates'
    ref: main          # floating ref — can change
    file: '/docker-build.yml'
  - remote: 'https://raw.githubusercontent.com/.../template.yml'  # arbitrary URL
```

Remote `include:` from external URLs or floating Git refs carries the same supply-chain concerns as GitHub Actions: templates can change under you.

GitLab's **CI/CD Component Catalog** addresses some of this by requiring semantic versions, but adoption is per-team.

### Meticulous

All reusable workflows are versioned in the platform catalog. There is no mechanism to pull in arbitrary external YAML or scripts — only catalog entries registered by a platform admin.

---

## Runner vs agent networking

GitLab Runner uses **polling** against the GitLab API to pick up jobs. This is outbound-only, which is a strength GitLab and Meticulous share. The difference: GitLab Runner is polling against `gitlab.com` or your self-hosted GitLab instance; Meticulous agents poll a NATS subject via the met-controller, which is entirely under your control and requires no dependency on an external SaaS.

For teams running fully self-hosted: GitLab CE + GitLab Runner is a comparable footprint to Meticulous (control plane + agents), but GitLab's control plane is substantially larger in resource footprint and operational complexity.

---

## Secrets

GitLab CI variables are injected as environment variables. Variables marked "masked" are redacted from job logs but are still environment variables in the process.

GitLab CI also supports **external secrets** via HashiCorp Vault (EE), but setup requires the Vault integration plugin and EE license for the most robust features.

Meticulous delivers secrets via per-job asymmetric encryption and supports AWS Secrets Manager, Vault/OpenBao, and Kubernetes Secrets as first-class external sources on all license tiers.

---

## SCM agnosticism

GitLab CI is designed for GitLab-hosted repositories. Using it with GitHub or Bitbucket requires mirroring or external trigger workarounds.

Meticulous supports webhook triggers from GitHub, GitLab, and Bitbucket natively, and can trigger pipelines from any SCM via generic webhooks — with no hard dependency on any SCM platform.
