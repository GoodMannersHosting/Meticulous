- Prom/Otel metrics and log shipping → **Resolved in [design/adr/007](adr/007-observability-opentelemetry.md):** OTel SDK with OTLP export; Prometheus-compatible scrape endpoint. OpenTelemetry Collector optional in reference deployment.

- S3 compatible storage requirement? → **Resolved in [design/constraints.md](constraints.md) and [operations-and-reliability.md](operations-and-reliability.md):** Yes — S3-compatible object storage required (SeaweedFS in dev, AWS S3 / GCS / R2 in production). Separate buckets per concern with independent lifecycle policies. See operations doc for bucket layout.

- Pre-populate build tool volumes, mount as read-only for agents (OCI Archive?) to improve build tool requirements and minimize workflow runtime?
  - symlink latest tools to `/usr/local/bin/`
  - `/buildtools/${binary}/${version}/${arch}/${binary}`
    → **Captured as future feature in [design/features.md](features.md)** (Build tool caching section). Requires ADR before implementation; not in v1 scope.
