# Runtime configuration

**Audience:** integrator, operator

Runtime settings control inference profiles, stage routing, TUI defaults, and verification policy.

---

## Execution lifetime and recovery

Interactive turns and delegated work have no implicit total execution deadline.
A build or other productive operation can continue for as long as it needs.
Explicit execution deadlines, permission expiry, permission revocation, and user
cancellation still apply. Tool rounds are unlimited by default; the optional
compatibility round limit is a separate operator setting.

Connection timeouts, stream idle detection, and tool observation windows bound
how long a caller waits for a response. They do not establish the lifetime of
accepted background work. A pending response is not a failure or permission to
repeat a mutation: retain its work handle and inspect the existing operation.
Reconnecting the app resumes observation of that operation.

Resource and authority checks remain in force, including filesystem and network
permissions, output and image memory budgets, workflow source and step limits,
and work-environment ownership. Repeated identical failed tool calls can produce
a recoverable failure checkpoint; successful or pending work does not trigger
that guard. See [agent tools](agent-tools.md).

Grapheme workflows and native shell commands have no implicit total execution
deadline. On the full daemon, an operator may set
`MEDOUSA_GRAPHEME_EXECUTION_TIMEOUT_MS`; the
legacy `STASIS_GRAPHEME_EXECUTION_TIMEOUT_MS` and
`GRAPHEME_EXECUTION_TIMEOUT_MS` variables remain accepted as explicit workflow
deadlines. These request cancellation at the deadline and wait for the
interpreter to exit before reporting failure. A caller's observation window
does not request cancellation.

---

## HTTP routes

| Method | Path | Purpose |
|--------|------|---------|
| GET | `/v1/runtime/defaults` | Engine default config snapshot |
| GET/PUT | `/v1/runtime/tui-defaults` | Full `tui_defaults.json` blob |
| GET/PUT | `/v1/runtime/workers` | Worker capacity and preferred lane shares; restart required after PUT |
| PUT | `/v1/runtime/inference-profiles` | Inference profile matrix |
| POST | `/v1/runtime/config/command` | `RuntimeConfigCommandSpec` mutations |
| POST | `/v1/runtime/stage-route/command` | Stage routing matrix |

SDK: `runtime().config_command`, `runtime().stage_route_command`

---

## Config command (`RuntimeConfigCommandSpec`)

Used by TUI and automation to adjust runtime without editing files directly. Request/response types in `medousa_types::daemon_api`.

CLI/TUI: `src/bin/medousa_tui/daemon_commands.rs`

---

## Inference profiles

`PUT /v1/runtime/inference-profiles` sets provider/model/fallback chains for
`main`, `vision`, `image_generation`, and `stt`. Image generation is intentionally
separate from vision: vision consumes images as model input, while image
generation creates daemon-owned media. The initial adapter is OpenAI's Image API
and requires an OpenAI API key; `openai-codex` ChatGPT sign-in is not reused as
an Image API credential.

Env vars: [configuration-reference.md](../configuration-reference.md)  
Plan: [inference-profiles-and-model-catalog-plan.md](../../architecture/inference-profiles-and-model-catalog-plan.md)

---

## Stage routing

`POST /v1/runtime/stage-route/command` mutates which model/stage handles each turn phase. Types: `StageRouteCommandRequest`, `StageRouteCommandResponse`.

---

## Local engine

Daemon is **probe-only** for `medousa_local`. Spawn/load via:

- `medousa start daemon --inference`
- `medousa models engine-load`
- `medousa_host::spawn_medousa_local` (Rust)

Not via daemon `POST /v1/local/engine/load` (removed).
