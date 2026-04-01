# Phased build order

Summary from [.cursor/plans/master_architecture_4bf1d365.plan.md](../../../.cursor/plans/master_architecture_4bf1d365.plan.md). Phases are planning aids; implementation status varies by area.

| Phase | Focus | Key deliverables |
| --- | --- | --- |
| 0 | Foundation | Workspace, `met-core`, Postgres schema, protobuf, CI |
| 1 | Agent system | `met-agent`, `met-controller`, NATS, provisioning, join tokens |
| 2 | Pipeline engine | YAML parser, DAG, scheduling, containers, variable/secret injection |
| 3 | Security layer | Per-job PKI, OIDC/JWT, secrets integrations, auditing |
| 4 | API and CLI | REST CRUD, WebSocket logs, `met-cli`, debug mode |
| 5 | Frontend | Web UI: projects, runs, logs, DAG, users/groups |
| 6 | Observability | OTel, log pipeline, SBOM, blast radius, tool tracking |
| 7 | Release management | Release workflows, scheduling, rollback, notifications |
