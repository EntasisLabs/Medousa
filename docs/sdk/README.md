# Medousa SDK

Shared client libraries for talking to **medousa_daemon** without duplicating HTTP paths or serde types.

## Crates

| Crate | Role |
|-------|------|
| [`medousa-types`](../../crates/medousa-types/) | Serde DTOs for daemon API (`daemon_api`, `session`, `local`, …) |
| [`medousa-sdk`](../../crates/medousa-sdk/) | `MedousaClient` + `HttpTransport` |
| [`medousa-sdk-iroh`](../../crates/medousa-sdk-iroh/) | `WorkshopTransport` — LAN HTTP with auth headers (Tauri workshop routing) |
| [`medousa-host`](../../crates/medousa-host/) | Spawn `medousa_local`, binary resolution, bind probes |

## Quick start (async)

```rust
use std::sync::Arc;
use medousa_sdk::{HttpTransport, MedousaClient};

let client = MedousaClient::with_transport(
    Arc::new(HttpTransport::new()),
    "http://127.0.0.1:7419",
);

let health = client.health().get().await?;
let sessions = client.sessions().list(20).await?;
```

## API surface (`MedousaClient`)

| Method | Endpoints |
|--------|-----------|
| `health()` | `GET /health` |
| `ingest()` | `POST /v1/ingest` |
| `local_models()` | `GET/POST/DELETE /v1/local/*` |
| `jobs()` | `POST /v1/jobs/ask` |
| `recurring()` | `POST /v1/recurring/prompt` |
| `sessions()` | `/v1/sessions/*` |
| `interactive()` | `POST /v1/interactive/turn` |
| `runtime()` | `/v1/runtime/*/command` |
| `budget()` | `/v1/turns/budget-requests/*` |

Blocking CLI helpers: `medousa_sdk::BlockingLocalModelsClient`.

## Tauri desktop

`apps/medousa-home/src-tauri/src/daemon/sdk.rs` implements `Transport` by delegating to the existing LAN/Iroh `workshop_transport` (preserves mobile fallback). Local inference commands use `MedousaClient::local_models()`.

Spawn offline brain via `medousa_host` / Tauri `workshop_runtime::ensure_local_brain` — **not** `POST /v1/local/engine/load` (removed; daemon is probe-only).

## Daemon library layout

See [daemon-modules.md](../architecture/daemon-modules.md) for the `medousa::daemon` module split (`ingest`, `interactive`, `jobs`, `router`, …).

## Channel adapters

Telegram, Discord, and Slack bins use `client.ingest().post(&IngestRequest)`.

## TUI

`src/bin/medousa_tui/daemon_commands.rs` uses `MedousaClient` for health, jobs, sessions, interactive turns, runtime commands, and budget approval.

## Types

Import from `medousa_types` (or `medousa::daemon_api` re-exports on the server). Do **not** mirror structs in app `types.rs` files.

## Workshop LAN + Iroh

For simple LAN-only clients, use `medousa_sdk_iroh::WorkshopTransport::from_lan_base(url)` with bearer token config.

Tauri keeps the full LAN→Iroh failover in `workshop_transport.rs`; the SDK Iroh crate is the thin HTTP adapter for authenticated workshop base URLs.
