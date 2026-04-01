Canonical agent-security content: [`skills/meticulous-agent-security-invariants/SKILL.md`](../skills/meticulous-agent-security-invariants/SKILL.md) (and `references/` there).

- Agents should NEVER have access to secrets outside of their specific pipeline scope
- Agents must be revokable/killable server-side
- Dev Testing and Iteration must be less painful than other solutions like GitHub Actions; push users to build out powershell/bash/python and re-usable formats for testing
- Support "test" or "dry" mode
- OIDC claims and Sub for JWT Auth to secrets providers, similar to how GitHub Actions does it (https://docs.github.com/en/actions/concepts/security/openid-connect)
- Live log-streaming to the control plane/agent controller (final location tbd) for easy remote debugging/viewing
- One-Time auth tokens to pull secrets
- Agent join tokens should be scoped (i.e "This pipeline", "this project", or "Any workflow/pipeline/group/etc")
  Agent JWT Token Expirations and approval workflow for long lived agents (i.e Windows/MacOS runners more difficult to make ephemeral)

- Operating Environment Validation
  - IPs (public/private), hostname, OS, Physical/Virtual/Container, Route to known IP, os patch level, etc (need to flush this out more)

- Kubernetes Operator for the agent controller (Similar to GitHub Action Runner Controller)

Remote Agents would have one-time and per-job PKI to encrypt/decrypt secrets

Support Agents on:

- Linux AMD64/ARM64
- MacOS Darwin ARM64
- Windows AMD64
- DOWN THE LINE - BSD AMD/ARM64 (would not have container support)

POSSIBLE PKI PROCESS

1. Server: New Job Published to Pub/sub
2. Agent: Picks Up Job
3. Agent: Create one-time X25519 keypair (raw 32-byte Curve25519 point; see ADR-004)
4. Server: Validate receipt of the Pubkey
5. Server: Determine which secrets to pull; pull them
6. Server: Encrypt each key=value pair of secrets with pubkey and get sha256sum for each secret
7. Server: Respond to Agent with key=(encrypted values) and sha256sum for each
8. Agent: Receive encrypted value; decrypt and validate the sha256sum for each

- Administrators/Security should be able to have configurable "join" security checks
- NTP **MUST** be enabled

---

## Provisioning Agent Pipeline

1. Controller: Create Join Token
2. Agent: Start w/ Join token
3. Agent: Generate security bundle & x509 pubkey; send to Controller
4. Controller: Validate security bundle & store x509 pubkey (in agent metadata db table)
5. Controller: If security bundle validated, create JWT (including renewable or not)
   6a. Controller: Create Job queue if necessary
   6b. Agent: Receive 200/OK and JWT Token; validate, join job queue
