# medousa-app-core

Renderer-agnostic **application** core for Medousa Home.

## Ownership

| Layer | Owns |
|-------|------|
| Workshop daemon + `medousa-types` | Durable records, execution, permissions, filesystem authority |
| **`medousa-app-core`** | Chat lifecycle, live↔persisted reconciliation, stable identities, tool/agent relationships, recovery |
| Tauri `app_chat` host | Process-local `ChatCore`, IPC commands |
| Svelte / future renderers | Layout, a11y, scrolling, input widgets, paint |

Daemon API and application API stay separate. This crate does not become a competing workshop authority.

## First slice

Canonical chat timeline:

1. Load persisted history (`ApplyHistory` / `ProjectHistory`)
2. Incorporate live stream events (`ApplyStreamEvent`)
3. Reconcile live and persisted representations (`merge_transcript`)
4. Reconnect / session switch with revision guards
5. Stable turn/item identities and ordering
6. Tool and agent/A2A relationships on the snapshot

## Regenerate client types

```bash
cargo run -p medousa-app-core --features json-schema --bin medousa-app-core-schema
python3 scripts/gen-app-api-types.py
bash scripts/check-app-api.sh
```

## Headless tests

```bash
cargo test -p medousa-app-core --tests
```

## Host integration

Tauri commands: `app_chat_dispatch`, `app_chat_open_session`, `app_chat_snapshot`.

Vitest / Node (no Tauri internals) drive the same `ChatCore` through the
`app-chat-dispatch` stdio binary aliased in `browserCore.ts`.
