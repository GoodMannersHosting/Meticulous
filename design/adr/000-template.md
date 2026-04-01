# ADR-NNN: Short title

**Status:** Proposed | Accepted | Deprecated  
**Date:** YYYY-MM-DD  
**PRDs:** Link to `design/prd/0xx-*.md` files.

## Context

What forces a decision? Link issues, PRDs, prior plans.

## Decision

State the choice in one paragraph, then bullets if needed.

## Consequences

Positive and negative. What breaks, what migrations run.

## Threat model (if security-sensitive)

- **Assets:** …
- **Adversaries:** …
- **Mitigations:** …
- **Residual risk:** …

**Certificates:** If this ADR references X.509 or TLS trust stores, operational verification should include `openssl x509 -text -noout -in <file>` for expiry, key strength, and signature algorithm before production use (workspace security rules).

## References

- Code paths, `proto/`, migrations, other ADRs.
