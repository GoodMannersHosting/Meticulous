# Agent guidance

Architecture, workspace layout, and agent security rules for Meticulous live in **repository skills** under [`skills/`](skills/).

| Topic                                                  | Skill path                                                                                                     |
| ------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------- |
| Product scope, stability, where to look next           | [`skills/meticulous-repo-context/SKILL.md`](skills/meticulous-repo-context/SKILL.md)                           |
| Crates, migrations, `proto/`, build commands           | [`skills/meticulous-rust-workspace/SKILL.md`](skills/meticulous-rust-workspace/SKILL.md)                       |
| Control plane, agents, NATS, domain model, plan index  | [`skills/meticulous-system-architecture/SKILL.md`](skills/meticulous-system-architecture/SKILL.md)             |
| Join tokens, revocation, per-job secrets, provisioning | [`skills/meticulous-agent-security-invariants/SKILL.md`](skills/meticulous-agent-security-invariants/SKILL.md) |

Load the narrowest skill that matches the task. Additional enforcement and crypto rules remain in [`.cursor/rules/`](.cursor/rules/).
