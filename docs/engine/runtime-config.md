# Runtime configuration

**Audience:** integrator, operator

Runtime settings control inference profiles, stage routing, TUI defaults, and verification policy.

---

## HTTP routes

| Method | Path | Purpose |
|--------|------|---------|
| GET | `/v1/runtime/defaults` | Engine default config snapshot |
| GET/PUT | `/v1/runtime/tui-defaults` | Full `tui_defaults.json` blob |
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

`PUT /v1/runtime/inference-profiles` sets provider/model/fallback chains per task kind.

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
