# Index of `.cursor/plans`

One-line guide to detailed design plans under [.cursor/plans/](../../../.cursor/plans/). Open the file when implementing or reviewing that subsystem.

| Plan file | Topic |
| --- | --- |
| `master_architecture_4bf1d365.plan.md` | Master architecture, stack, hierarchy, phased order |
| `foundation_scaffolding.plan.md` | Foundation: workspace, core, schema, protobuf, CI |
| `agent_system.plan.md` | Agent binary, controller, NATS, provisioning, operator, gRPC contracts |
| `pipeline_engine_78625c45.plan.md` | Engine: parsers, DAG, workflows, caching, execution |
| `security_and_secrets_bcd5e7fa.plan.md` | PKI, OIDC, secrets broker, integrations, auditing |
| `platform_security_and_secrets_admin.plan.md` | Platform security and admin-oriented secrets work |
| `api_and_cli_aa29f1b3.plan.md` | REST, WebSocket, auth, CLI, debug |
| `frontend_and_ui_8feb09fc.plan.md` | Web UI structure, pages, viewers (check repo for SvelteKit reality) |
| `observability_and_storage_7006b334.plan.md` | Metrics, logs, object storage, SBOM, tooling |
| `codebase_completion_roadmap_a65c1dae.plan.md` | Completion roadmap across subsystems |
| `self-hosting_milestone_4f8d8095.plan.md` | Self-hosting milestone |
| `admin_portal_&_agent_deployment_71b​a9de0.plan.md` | Admin portal and agent deployment |
| `agent_config_file_loading_d100dad5.plan.md` | Agent configuration file loading |
| `vertical_slice_manual_run.plan.md` | E2E slice: manual run, NATS dispatch, agent logs/status (ADR-001–004) |

*(The admin portal row uses a zero-width space inside the plan id so spellcheck passes; on disk the name is normal ASCII—see [`.cursor/plans/`](../../../.cursor/plans/).)*
