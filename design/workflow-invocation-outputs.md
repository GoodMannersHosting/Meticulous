# Workflow invocation outputs (`met-output` + IPC)

This document is the **normative platform spec** for passing structured outputs from reusable workflow steps to the controller and downstream pipeline jobs. It complements [`pipelines.md`](pipelines.md) (affinity, composition) and the crypto workspace rules under [`.cursor/rules/codeguard-1-crypto-algorithms.mdc`](../.cursor/rules/codeguard-1-crypto-algorithms.mdc).

## Transport: anonymous pipe vs socket

**Linux / macOS (native agent):** A unidirectional **anonymous pipe** (`pipe2` + `O_CLOEXEC`). After `fork`, the child runs `dup2` in `pre_exec` so the write end is **FD 3**; the environment variable **`METICULOUS_OUTPUT_FD=3`** is set and **`METICULOUS_OUTPUT_PATH` is unset**. The parent closes its duplicate of the write end after spawn, holds the read end, and drains frames until the step exits (all write ends closed). **`met-output`** uses `METICULOUS_OUTPUT_FD` first (via `write(2)` without closing the FD), then falls back to **`METICULOUS_OUTPUT_PATH`** for containers. **Rationale:** No filesystem artifacts; EOF works once the child exits; no `SO_PEERCRED` needed because only the spawned step tree receives the write FD.

**Linux container steps (`docker run` / `podman run`):** An optional **named FIFO** under the workspace (e.g. `$METICULOUS_WORKSPACE/.meticulous/output-ipc`) passed as **`METICULOUS_OUTPUT_PATH`** (absolute path inside the container). The agent opens the FIFO for **read** in parallel with container start so `met-output` can open for **write** without deadlock.

**Windows (native):** **Named pipe** with a DACL granting **same-user + WRITE_DAC/owner** only; agent validates client identity (equivalent intent to `SO_PEERCRED`). If not implemented in a build, `met-output` may return a clear “unsupported platform” error.

**Unix-domain socket** is **optional**; pipes are the default to minimize attack surface and host configuration.

## Frame format (wire)

All multi-byte integers are **big-endian**.

| Offset | Size | Field |
|--------|------|--------|
| 0 | 4 | Magic **ASCII `"MOUT"`** (`0x4D 0x4F 0x55 0x54`) |
| 4 | 1 | Protocol version (`1`) |
| 5 | 1 | Message type (`1` = VAR, `2` = SECRET) |
| 6 | 2 | Reserved (`0`) |
| 8 | 4 | `key_len` (UTF-8 bytes, ≥ 1) |
| 12 | 4 | `value_len` (payload bytes; see message types) |
| 16 | `key_len` | Key (UTF-8) |
| 16+`key_len` | `value_len` | Payload |

**Maximum frame size:** `key_len + value_len + 16` MUST NOT exceed **16 MiB + 4 KiB** (exactly: `16 * 1024 * 1024 + 4096` bytes for the sum of `key_len`, `value_len`, and the 16-byte header). Implementations SHOULD use the shared constants in `met_core::output_ipc`.

**Graceful close:** End of stream is **EOF** on the pipe/FIFO after the step exits. No “END” frame is required. Partial frames after EOF are **malformed**.

## Message types

### VAR (`1`)

- **Payload:** UTF-8 **value** (may be empty unless key rules forbid—values can be empty).
- Semantics: **non-secret** invocation output. Safe to show in UI (subject to product redaction rules).

### SECRET (`2`)

- **Payload:** UTF-8 **plaintext secret value** (only on the IPC leg inside the agent boundary).
- The agent MUST **re-wrap** into a **controller-facing envelope** before persistence or NATS (see **Crypto**). The agent MUST NOT log plaintext secrets.

## Duplicate keys

**Policy:** **Last-wins** per job for both VAR and SECRET: a later frame with the same key replaces the earlier value in the merged map for that job. Implementations MUST document this in diagnostics when debugging duplicate emissions.

## Aggregate size budget (per job / invocation)

**Per-value cap:** UTF-8 **value** length (VAR) or UTF-8 plaintext length (SECRET on IPC) ≤ **16 MiB**.

**Per-job aggregate cap:** After framing and crypto re-wrap:

- For each **public** output stored on the controller: add **UTF-8 byte length** of the value string.
- For each **secret** output stored: add **`ciphertext` envelope length on the wire to the controller** (see below: `ephemeral_x25519_public` + `nonce` + `ciphertext` inclusive).

Total MUST NOT exceed **256 MiB** per **workflow job run** (single `job_run_id`). The agent enforces this while draining IPC; the controller/engine re-validates on ingest (**defense in depth**).

## Peer identity (`SO_PEERCRED` / Windows ACL)

- **Pipe (Unix):** Only the child process tree that received the write end can write; the agent does not expose the read end to the step.
- **FIFO:** Any process in the container that can write the mounted path can emit frames; rely on container boundary and **caps**/**policy** (documented limitation).
- **Windows named pipe:** Restrict writes via pipe **DACL** to the same security principal as the agent job runner; document failures when impersonation is used.

## Key grammar

- Pattern: **`^[A-Za-z_][A-Za-z0-9_]{0,127}$`** (1–128 chars).
- **Reserved:** Keys starting with **`MET_OUTPUT_RESERVED_`**, and the literal names **`PATH`**, **`PWD`**, **`HOME`**, **`USER`** (case-sensitive) are rejected by `met-output` to avoid confusion with ambient environment.
- Values are **untrusted** strings; recipients MUST NOT interpolate into shell without escaping.

## Declared outputs (YAML)

Reusable workflows may declare **`outputs:`** at **workflow** level in `RawWorkflowDef`. Optional **`outputs:`** on **`RawStep`** declare step-local names for documentation/diagnostics.

**Strictness:**

- **Optional (default):** Undeclared keys emitted by `met-output` are accepted but may trigger **diagnostics** in workflow diagnostics / linter tier.
- **Strict mode (future):** Pipelines or workflow metadata may require declaration before emission; not all runners enable strict mode in v1.

References from a pipeline use:

```text
${{ workflows.<invocation_id>.outputs.<name> }}
```

See parser / `VariableContext` validation: `<invocation_id>` MUST exist on `pipeline.workflows[].id`. Unknown declared names MAY be warned when the callee workflow definition is available.

## Failure modes / exit codes (`met-output`)

| Condition | Exit code | Stderr (no secrets) |
|-----------|-----------|---------------------|
| Bad usage / missing `KEY=value` | 2 | Usage |
| Key fails grammar / reserved | 3 | Key rejected |
| Value > 16 MiB UTF-8 | 4 | Oversize value |
| Aggregate would exceed 256 MiB | 5 | Oversize aggregate |
| I/O error writing IPC | 6 | I/O error (redacted) |
| Malformed frame on reader (agent) | n/a | Step fails; agent maps to step failure |

## Crypto (secret outputs on the wire to the controller)

- **No hardcoded keys** (see codeguard). Per **`job_run`**, the engine generates an **X25519** key pair. The **public** key is sent to the agent in **`JobDispatch`**. The **private** key is stored with the `job_run` row (protected by database / KMS posture—**not** in application logs).
- **Envelope (v1):** `ephemeral_x25519_public` (32) + `nonce` (12) + `ciphertext` (**AES-256-GCM** with 128-bit tag, ciphertext includes tag) where the AEAD key is derived from **HKDF-SHA256** over the X25519 shared secret with domain separation string **`meticulous.met-output.v1`**.
- UI MUST redact secret outputs; secret values MUST NOT appear in logs.

## Related

- Expression grammar updates: `met-parser` (`workflows.<id>.outputs.<name>`).
- Persistence: `job_runs.outputs` JSON merges + optional dedicated columns for large blobs in future revisions.
